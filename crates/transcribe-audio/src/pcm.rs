use std::path::Path;

use rubato::{FftFixedIn, Resampler};
use transcribe_core::Result;

use crate::{audio_error, MonoPcm, ASR_SAMPLE_RATE};

/// Interleaved capture PCM before channel mixing or resampling. Device samples
/// are normalized to f32 in the callback; WAV export preserves those samples,
/// their native sample rate, and their channel layout. Inputs with more than
/// 24 significant bits can lose precision during normalization to f32.
#[derive(Debug, Clone)]
pub struct CapturedAudio {
    samples: Vec<f32>,
    sample_rate: u32,
    channels: u16,
    capture_warning: Option<String>,
}

impl CapturedAudio {
    pub fn new(samples: Vec<f32>, sample_rate: u32, channels: u16) -> Result<Self> {
        validate_format(sample_rate, channels as usize)?;
        if samples.len() % usize::from(channels) != 0 {
            return Err(audio_error(
                "audio",
                "PCM ends in an incomplete audio frame",
            ));
        }
        if let Some(index) = samples.iter().position(|sample| !sample.is_finite()) {
            return Err(transcribe_core::Error::NonFiniteAudio { index });
        }
        Ok(Self {
            samples,
            sample_rate,
            channels,
            capture_warning: None,
        })
    }

    pub fn samples(&self) -> &[f32] {
        &self.samples
    }
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
    pub fn channels(&self) -> u16 {
        self.channels
    }
    /// A device error or overflow ended capture early. Samples remain usable,
    /// but callers must surface this warning and retain it with the recording.
    pub fn capture_warning(&self) -> Option<&str> {
        self.capture_warning.as_deref()
    }

    pub(crate) fn with_capture_warning(mut self, warning: Option<String>) -> Self {
        self.capture_warning = warning;
        self
    }
    pub fn duration_seconds(&self) -> f64 {
        self.samples.len() as f64 / f64::from(self.channels) / f64::from(self.sample_rate)
    }

    pub fn write_wav(&self, path: impl AsRef<Path>) -> Result<()> {
        let spec = hound::WavSpec {
            channels: self.channels,
            sample_rate: self.sample_rate,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut writer = hound::WavWriter::create(path, spec)
            .map_err(|err| audio_error("WAV storage", err.to_string()))?;
        for &sample in &self.samples {
            writer
                .write_sample(sample)
                .map_err(|err| audio_error("WAV storage", err.to_string()))?;
        }
        writer
            .finalize()
            .map_err(|err| audio_error("WAV storage", err.to_string()))
    }

    pub fn as_mono_16k(&self) -> Result<MonoPcm> {
        let mono: Vec<f32> = self
            .samples
            .chunks_exact(usize::from(self.channels))
            .map(mix_frame)
            .collect();
        if self.sample_rate == ASR_SAMPLE_RATE {
            return MonoPcm::new(mono, ASR_SAMPLE_RATE);
        }
        resample_mono(&mono, self.sample_rate)
    }
}

pub(crate) fn validate_format(sample_rate: u32, channels: usize) -> Result<()> {
    if sample_rate == 0 || sample_rate > 384_000 || channels == 0 || channels > 32 {
        return Err(audio_error("audio", format!(
            "unsupported audio format: {sample_rate} Hz, {channels} channels (maximum 384000 Hz and 32 channels)"
        )));
    }
    Ok(())
}

pub(crate) fn mix_frame(frame: &[f32]) -> f32 {
    // Accumulate in f64 so even finite, out-of-range float PCM cannot overflow a sum.
    (frame.iter().map(|&sample| f64::from(sample)).sum::<f64>() / frame.len() as f64) as f32
}

/// One recording's converter. Packet boundaries never restart the FFT filter.
/// The same converter is used by full-file conversion and live frontends.
pub struct StreamingResampler {
    source_rate: u32,
    filter: Option<FftFixedIn<f32>>,
    pending: Vec<f32>,
    delay_remaining: usize,
    input_samples: u64,
    output_samples: usize,
    finished: bool,
}

impl StreamingResampler {
    pub fn new(source_rate: u32) -> Result<Self> {
        validate_format(source_rate, 1)?;
        let filter = if source_rate == ASR_SAMPLE_RATE {
            None
        } else {
            Some(
                FftFixedIn::<f32>::new(source_rate as usize, ASR_SAMPLE_RATE as usize, 1024, 2, 1)
                    .map_err(|err| audio_error("audio resampling", err.to_string()))?,
            )
        };
        Ok(Self {
            source_rate,
            delay_remaining: filter.as_ref().map_or(0, |r| r.output_delay()),
            filter,
            pending: Vec::new(),
            input_samples: 0,
            output_samples: 0,
            finished: false,
        })
    }

    pub fn source_rate(&self) -> u32 {
        self.source_rate
    }

    pub fn push(&mut self, samples: &[f32]) -> Result<Vec<f32>> {
        if self.finished {
            return Err(audio_error(
                "audio resampling",
                "recording is already finalized",
            ));
        }
        if let Some(index) = samples.iter().position(|x| !x.is_finite()) {
            return Err(transcribe_core::Error::NonFiniteAudio { index });
        }
        self.input_samples = self
            .input_samples
            .checked_add(samples.len() as u64)
            .ok_or_else(|| audio_error("audio resampling", "sample count overflow"))?;
        let Some(filter) = &mut self.filter else {
            self.output_samples += samples.len();
            return Ok(samples.to_vec());
        };
        let mut out = Vec::new();
        // Buffer only a single native input frame even for a full recording.
        let mut rest = samples;
        while !rest.is_empty() {
            let take = (filter.input_frames_next() - self.pending.len()).min(rest.len());
            self.pending.extend_from_slice(&rest[..take]);
            rest = &rest[take..];
            if self.pending.len() == filter.input_frames_next() {
                let block = filter
                    .process(&[self.pending.as_slice()], None)
                    .map_err(|err| audio_error("audio resampling", err.to_string()))?;
                out.extend_from_slice(&block[0]);
                self.pending.clear();
            }
        }
        self.accept_output(out, None)
    }

    pub fn finish(&mut self) -> Result<Vec<f32>> {
        if self.finished {
            return Err(audio_error(
                "audio resampling",
                "recording is already finalized",
            ));
        }
        self.finished = true;
        let desired = self
            .input_samples
            .checked_mul(u64::from(ASR_SAMPLE_RATE))
            .and_then(|v| v.checked_add(u64::from(self.source_rate) / 2))
            .ok_or_else(|| audio_error("audio resampling", "sample count overflow"))?
            / u64::from(self.source_rate);
        let desired =
            usize::try_from(desired).map_err(|e| audio_error("audio resampling", e.to_string()))?;
        let remaining = desired
            .checked_sub(self.output_samples)
            .ok_or_else(|| audio_error("audio resampling", "too many samples emitted"))?;
        let Some(filter) = &mut self.filter else {
            return Ok(Vec::new());
        };
        let mut out = Vec::new();
        if !self.pending.is_empty() {
            let block = filter
                .process_partial(Some(&[self.pending.as_slice()]), None)
                .map_err(|err| audio_error("audio resampling", err.to_string()))?;
            out.extend_from_slice(&block[0]);
            self.pending.clear();
        }
        while out.len() < remaining + self.delay_remaining {
            let block = filter
                .process_partial::<&[f32]>(None, None)
                .map_err(|err| audio_error("audio resampling", err.to_string()))?;
            if block[0].is_empty() {
                return Err(audio_error(
                    "audio resampling",
                    "resampler could not flush its final samples",
                ));
            }
            out.extend_from_slice(&block[0]);
        }
        self.accept_output(out, Some(remaining))
    }

    fn accept_output(&mut self, mut samples: Vec<f32>, limit: Option<usize>) -> Result<Vec<f32>> {
        let skip = self.delay_remaining.min(samples.len());
        self.delay_remaining -= skip;
        samples.drain(..skip);
        if let Some(limit) = limit {
            samples.truncate(limit);
        }
        if samples.iter().any(|x| !x.is_finite()) {
            return Err(audio_error(
                "audio resampling",
                "non-finite resampled audio",
            ));
        }
        self.output_samples += samples.len();
        Ok(samples)
    }
}

pub(crate) fn resample_mono(samples: &[f32], source_rate: u32) -> Result<MonoPcm> {
    let mut converter = StreamingResampler::new(source_rate)?;
    let mut output = converter.push(samples)?;
    output.extend(converter.finish()?);
    MonoPcm::new(output, ASR_SAMPLE_RATE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arbitrary_packets_preserve_full_conversion_and_final_boundaries() {
        for rate in [16000, 44100, 48000] {
            for length in [0, 1, 159, 160, 161, 511, 512, 513, 1024, 49137] {
                let input: Vec<f32> = (0..length)
                    .map(|i| (i as f32 * 0.13).sin() * 0.25)
                    .collect();
                let whole = resample_mono(&input, rate).unwrap();
                let mut stream = StreamingResampler::new(rate).unwrap();
                let mut got = Vec::new();
                let mut offset = 0;
                for size in [1, 319, 17, 1600, 511].into_iter().cycle() {
                    if offset == input.len() {
                        break;
                    }
                    let end = (offset + size).min(input.len());
                    got.extend(stream.push(&input[offset..end]).unwrap());
                    offset = end;
                }
                got.extend(stream.finish().unwrap());
                assert_eq!(got, whole.samples(), "rate {rate}, length {length}");
                assert_eq!(
                    got.len(),
                    ((length as u64 * 16000 + u64::from(rate) / 2) / u64::from(rate)) as usize
                );
                assert!(stream.finish().is_err());
                assert!(stream.push(&[0.0]).is_err());
            }
        }
        assert!(StreamingResampler::new(0).is_err());
        assert!(StreamingResampler::new(48000)
            .unwrap()
            .push(&[f32::NAN])
            .is_err());
    }

    #[test]
    fn captured_16k_mixing_preserves_frames_without_resampling() {
        for (channels, samples, expected) in [
            (1, vec![0.5, -0.25, 1.125], vec![0.5, -0.25, 1.125]),
            (
                2,
                vec![1.0, -1.0, 0.5, 0.25, -0.5, -0.25],
                vec![0.0, 0.375, -0.375],
            ),
            (
                2,
                vec![f32::MAX, f32::MAX, -f32::MAX, f32::MAX],
                vec![f32::MAX, 0.0],
            ),
            (2, vec![], vec![]),
        ] {
            let capture = CapturedAudio::new(samples.clone(), ASR_SAMPLE_RATE, channels).unwrap();
            let output = capture.as_mono_16k().unwrap();
            assert_eq!(output.sample_rate(), ASR_SAMPLE_RATE);
            assert_eq!(output.samples(), expected);
            assert_eq!(capture.samples(), samples, "archival input stays intact");
        }
    }

    #[test]
    fn float_wav_preserves_original_samples_and_channel_layout() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("original.wav");
        let samples = vec![-1.0, 0.25, 0.125, -0.5, 1.125, 0.0];
        CapturedAudio::new(samples.clone(), 44_100, 2)
            .unwrap()
            .write_wav(&path)
            .unwrap();
        let mut reader = hound::WavReader::open(path).unwrap();
        assert_eq!(reader.spec().channels, 2);
        assert_eq!(reader.spec().sample_rate, 44_100);
        assert_eq!(
            reader
                .samples::<f32>()
                .collect::<std::result::Result<Vec<_>, _>>()
                .unwrap(),
            samples
        );
    }

    #[test]
    fn resampling_preserves_duration_tail_and_rejects_aliases() {
        for rate in [16_000, 44_100, 48_000] {
            let length = rate as usize / 2 + 137;
            let mut samples = vec![0.0; length];
            // A final burst spanning the partial chunk must survive filter flushing.
            for (i, sample) in samples.iter_mut().enumerate().skip(length - 100) {
                *sample = (i as f32 * std::f32::consts::TAU * 1000.0 / rate as f32).sin();
            }
            let output = resample_mono(&samples, rate).unwrap();
            let expected =
                ((length as u64 * 16_000 + u64::from(rate) / 2) / u64::from(rate)) as usize;
            assert_eq!(output.samples().len(), expected);
            assert!(output.samples()[expected - 32..]
                .iter()
                .any(|x| x.abs() > 0.1));
        }
        let tone = |hz: f32| {
            (0..48_000)
                .map(|i| (i as f32 * std::f32::consts::TAU * hz / 48_000.0).sin())
                .collect::<Vec<_>>()
        };
        let rms = |samples: &[f32]| {
            (samples.iter().map(|x| x * x).sum::<f32>() / samples.len() as f32).sqrt()
        };
        let pass = resample_mono(&tone(1000.0), 48_000).unwrap();
        let reject = resample_mono(&tone(12_000.0), 48_000).unwrap();
        assert!(rms(&pass.samples()[100..15_900]) > 0.6);
        assert!(rms(&reject.samples()[100..15_900]) < 0.01);
    }
}
