use std::{fs::File, io::ErrorKind, path::Path};

use symphonia::core::{
    codecs::audio::AudioDecoderOptions,
    errors::Error as DecodeError,
    formats::{probe::Hint, FormatOptions, TrackType},
    io::MediaSourceStream,
    meta::MetadataOptions,
};
use vesper_core::Result;

use crate::{
    audio_error,
    pcm::{resample_mono, validate_format, ChannelMixer},
    validate_limit, MonoPcm,
};

/// Decode a supported audio track and return antialiased mono 16 kHz PCM.
/// WAV, MP3, FLAC, AAC/MP4, ALAC, AIFF, and Vorbis are supported by Symphonia.
/// An Ogg/WebM container can contain unsupported codecs such as Opus; errors are
/// returned explicitly. A zero duration limit disables the timer quota; file and
/// decoded-memory bounds still apply. Duration overflow is never truncated.
pub fn decode_file(path: impl AsRef<Path>, max_seconds: u32) -> Result<MonoPcm> {
    validate_limit(max_seconds)?;
    let path = path.as_ref();
    let file = File::open(path).map_err(|err| audio_error("audio decoding", err.to_string()))?;
    let length = file
        .metadata()
        .map_err(|err| audio_error("audio decoding", err.to_string()))?
        .len();
    if length > 512 * 1024 * 1024 {
        return Err(audio_error(
            "audio decoding",
            "audio file exceeds the 512 MiB import limit",
        ));
    }
    let mut hint = Hint::new();
    if let Some(extension) = path.extension().and_then(|value| value.to_str()) {
        hint.with_extension(extension);
    }
    let source = MediaSourceStream::new(Box::new(file), Default::default());
    let mut format = symphonia::default::get_probe()
        .probe(
            &hint,
            source,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .map_err(|err| {
            audio_error(
                "audio decoding",
                format!("cannot open this audio format: {err}"),
            )
        })?;
    // Gapless trimming moved from the format to the decoder options in
    // symphonia 0.6 (enabled by default); keep it explicit.
    let options = AudioDecoderOptions::default().gapless(true);
    // MP4/WebM can place a video track first. Prefer the container default, then
    // the first track for which an audio decoder actually exists.
    let (track_id, mut decoder) = format.default_track(TrackType::Audio).into_iter()
        .chain(format.tracks().iter())
        .find_map(|track| {
            let params = track.codec_params.as_ref()?.audio()?;
            symphonia::default::get_codecs()
                .make_audio_decoder(params, &options).ok()
                .map(|decoder| (track.id, decoder))
        })
        .ok_or_else(|| audio_error("audio decoding", "file contains no supported audio codec (Opus is not supported); convert it to WAV or FLAC first"))?;
    let mut source_format = None;
    let mut mixer = None;
    let mut mono = Vec::new();
    loop {
        let packet = match format.next_packet() {
            Ok(Some(packet)) => packet,
            Ok(None) => break,
            Err(DecodeError::IoError(err)) if err.kind() == ErrorKind::UnexpectedEof => break,
            Err(err) => return Err(audio_error("audio decoding", err.to_string())),
        };
        if packet.track_id != track_id {
            continue;
        }
        let decoded = decoder
            .decode(&packet)
            .map_err(|err| audio_error("audio decoding", format!("invalid audio packet: {err}")))?;
        let rate = decoded.spec().rate();
        let channels = decoded.spec().channels().count();
        validate_format(rate, channels)?;
        let current = (rate, channels);
        if let Some(previous) = source_format {
            if previous != current {
                return Err(audio_error("audio decoding", "sample rate or channels change within this file; convert it to a single-format WAV first"));
            }
        } else {
            source_format = Some(current);
        }
        let max_frames = u64::from(rate) * u64::from(max_seconds);
        if max_seconds > 0 && mono.len() as u64 + decoded.frames() as u64 > max_frames {
            return Err(audio_error(
                "audio decoding",
                format!("audio exceeds the configured {max_seconds}-second limit"),
            ));
        }
        if mono.len() as u64 + decoded.frames() as u64 > 512 * 1024 * 1024 / 4 {
            return Err(audio_error(
                "audio decoding",
                "decoded audio exceeds the 512 MiB buffer limit",
            ));
        }
        mono.try_reserve(decoded.frames()).map_err(|err| {
            audio_error(
                "audio decoding",
                format!("cannot allocate decoded audio buffer: {err}"),
            )
        })?;
        let mut buffer = Vec::<f32>::new();
        decoded.copy_to_vec_interleaved(&mut buffer);
        for frame in buffer.chunks_exact(channels) {
            if frame.iter().any(|sample| !sample.is_finite()) {
                return Err(audio_error(
                    "audio decoding",
                    "decoded audio contains non-finite samples",
                ));
            }
            mono.push(
                mixer
                    .get_or_insert_with(|| ChannelMixer::new(channels, spec.rate))
                    .mix(frame),
            );
        }
    }
    if mono.is_empty() {
        return Err(audio_error(
            "audio decoding",
            "file contains no decodable audio samples",
        ));
    }
    let (rate, _) = source_format.expect("nonempty audio has a format");
    resample_mono(&mono, rate)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CapturedAudio;

    #[test]
    fn no_duration_quota_imports_beyond_the_old_ceiling_without_truncation() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("long.wav");
        CapturedAudio::new(vec![0.0; 8_000 * 601], 8_000, 1)
            .unwrap()
            .write_wav(&path)
            .unwrap();
        let audio = decode_file(&path, 0).unwrap();
        assert_eq!(audio.samples().len(), 16_000 * 601);
        assert_eq!(audio.duration_seconds(), 601.0);
        assert!(decode_file(&path, 10)
            .unwrap_err()
            .to_string()
            .contains("10-second limit"));
    }

    #[test]
    fn imports_stereo_pcm_and_rejects_over_limit_and_invalid_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("stereo.wav");
        let samples: Vec<f32> = (0..44_100).flat_map(|_| [0.25, 0.75]).collect();
        CapturedAudio::new(samples, 44_100, 2)
            .unwrap()
            .write_wav(&path)
            .unwrap();
        let mono = decode_file(&path, 1).unwrap();
        assert_eq!(mono.sample_rate(), 16_000);
        assert_eq!(mono.samples().len(), 16_000);
        assert!((mono.samples()[8000] - 0.5).abs() < 0.001);
        CapturedAudio::new(vec![0.0; 44_101], 44_100, 1)
            .unwrap()
            .write_wav(&path)
            .unwrap();
        assert!(decode_file(&path, 1)
            .unwrap_err()
            .to_string()
            .contains("1-second limit"));
        std::fs::write(&path, b"not an audio file").unwrap();
        assert!(decode_file(&path, 1).is_err());
        assert!(decode_file(&path, 0).is_err());
    }

    #[test]
    fn decodes_real_compressed_formats_without_external_runtime_tools() {
        // Synthetic 440 Hz tones, encoded once using FFmpeg. The test itself
        // uses only the embedded Rust decoders; FFmpeg is not a dependency.
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        for file in ["tone.mp3", "tone.flac", "tone.ogg", "tone.m4a"] {
            let mono =
                decode_file(root.join(file), 1).unwrap_or_else(|err| panic!("{file}: {err}"));
            assert_eq!(mono.sample_rate(), 16_000);
            assert!(
                (0.16..0.21).contains(&mono.duration_seconds()),
                "{file}: {}",
                mono.duration_seconds()
            );
            assert!(
                mono.samples().iter().any(|sample| sample.abs() > 0.05),
                "{file}"
            );
        }
    }
}
