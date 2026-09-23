//! Native microphone capture, original PCM WAV storage, and imported audio decoding.

mod capture;
mod decode;
mod meter;
mod pcm;
mod pitch;
mod segmentation;

pub use capture::{
    list_input_devices, AudioChunk, AudioChunkStream, AudioFeatures, AudioRecorder, InputDevice,
    RecordingStatus,
};
pub use decode::decode_file;
pub use pcm::{CapturedAudio, StreamingResampler};
pub use segmentation::completed_segment_end;

use std::path::Path;
use transcribe_core::{Error, Result};

pub const ASR_SAMPLE_RATE: u32 = 16_000;

/// Single-channel floating point PCM with a known sample rate.
///
/// Construction rejects invalid rates and non-finite samples. Adapters must
/// explicitly resample before passing audio to an engine requiring another rate.
#[derive(Debug, Clone)]
pub struct MonoPcm {
    samples: Vec<f32>,
    sample_rate: u32,
}

impl MonoPcm {
    pub fn new(samples: Vec<f32>, sample_rate: u32) -> Result<Self> {
        if sample_rate == 0 {
            return Err(Error::InvalidSampleRate { sample_rate });
        }
        if let Some(index) = samples.iter().position(|sample| !sample.is_finite()) {
            return Err(Error::NonFiniteAudio { index });
        }
        Ok(Self {
            samples,
            sample_rate,
        })
    }

    pub fn samples(&self) -> &[f32] {
        &self.samples
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn duration_seconds(&self) -> f64 {
        self.samples.len() as f64 / f64::from(self.sample_rate)
    }

    pub fn require_sample_rate(&self, expected: u32) -> Result<()> {
        if self.sample_rate != expected {
            return Err(Error::SampleRateMismatch {
                expected,
                actual: self.sample_rate,
            });
        }
        Ok(())
    }

    /// Write signed 16-bit PCM accepted by native speech-runtime CLIs.
    /// Original captures remain lossless float WAVs through `CapturedAudio`.
    pub fn write_wav(&self, path: impl AsRef<Path>) -> Result<()> {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: self.sample_rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(path, spec)
            .map_err(|error| audio_error("WAV storage", error.to_string()))?;
        for &sample in &self.samples {
            writer
                .write_sample((sample.clamp(-1.0, 1.0) * f32::from(i16::MAX)).round() as i16)
                .map_err(|error| audio_error("WAV storage", error.to_string()))?;
        }
        writer
            .finalize()
            .map_err(|error| audio_error("WAV storage", error.to_string()))
    }
}

/// Alternative capture implementations may expose a mono-only boundary.
/// `AudioRecorder` additionally preserves the original channel layout and rate.
pub trait AudioCapture {
    fn start(&mut self) -> Result<()>;
    fn stop(&mut self) -> Result<MonoPcm>;
    fn cancel(&mut self) -> Result<()>;
}

fn audio_error(component: &'static str, message: impl Into<String>) -> Error {
    Error::Backend {
        component,
        message: message.into(),
    }
}

fn validate_limit(max_seconds: u32) -> Result<()> {
    if max_seconds > 600 {
        return Err(audio_error(
            "audio",
            "adapter duration limit must be zero (no cutoff) or at most 600 seconds",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sidecar_wav_is_mono_pcm16_and_clamps_samples() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("sidecar.wav");
        MonoPcm::new(vec![-2.0, -1.0, 0.0, 0.5, 1.0, 2.0], ASR_SAMPLE_RATE)
            .unwrap()
            .write_wav(&path)
            .unwrap();
        let mut reader = hound::WavReader::open(path).unwrap();
        assert_eq!(reader.spec().channels, 1);
        assert_eq!(reader.spec().bits_per_sample, 16);
        assert_eq!(reader.spec().sample_format, hound::SampleFormat::Int);
        assert_eq!(
            reader
                .samples::<i16>()
                .collect::<std::result::Result<Vec<_>, _>>()
                .unwrap(),
            vec![-i16::MAX, -i16::MAX, 0, 16_384, i16::MAX, i16::MAX]
        );
    }
}
