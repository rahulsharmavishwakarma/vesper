use crate::{
    check_cancelled, native, path_string, require_file, segmented_audio, worker_threads,
    InferenceError, Result, TranscriptionProgress,
};
use sherpa_onnx::{
    OfflineModelConfig, OfflineRecognizer, OfflineRecognizerConfig, OfflineTransducerModelConfig,
};
use std::path::Path;
use std::sync::atomic::AtomicBool;
use vesper_audio::{MonoPcm, ASR_SAMPLE_RATE};
use vesper_core::{AppConfig, AsrModel, ModelFileRole};

/// Bounds each native decode call, as every other offline adapter does. One
/// whole-recording call silently lost about half the words of 60–120 s
/// recordings (#153). 30 s keeps shorter recordings on a single call and
/// matches the MLX Parakeet limit. Update provenance if this policy changes.
pub(crate) const MAX_SEGMENT_SECONDS: usize = 30;

/// Recognize mono 16 kHz PCM through NVIDIA Parakeet v3's ONNX export.
/// Cancellation is observed around native calls; sherpa's offline decode cannot
/// be interrupted mid-call by this API. Supported languages are autodetected.
pub fn transcribe(
    audio: &MonoPcm,
    config: &AppConfig,
    model_root: &Path,
    cancelled: &AtomicBool,
) -> Result<String> {
    check_cancelled(cancelled)?;
    config.validate()?;
    if config.models.custom_asr.is_none() && config.models.asr != AsrModel::ParakeetTdt06bV3 {
        return Err(InferenceError::UnsupportedAsr(config.models.asr.id()));
    }
    audio.require_sample_rate(ASR_SAMPLE_RATE)?;
    if audio.samples().is_empty() {
        return Ok(String::new());
    }
    let recognizer_config = recognizer_config(config, model_root)?;
    check_cancelled(cancelled)?;
    let recognizer = OfflineRecognizer::create(&recognizer_config).ok_or_else(|| {
        native(
            "speech model load",
            "sherpa-onnx could not create the transducer recognizer",
        )
    })?;
    decode_with_recognizer(&recognizer, audio, cancelled, &|_| {})
}

pub(crate) fn recognizer_config(
    config: &AppConfig,
    model_root: &Path,
) -> Result<OfflineRecognizerConfig> {
    let [encoder, decoder, joiner, tokens] = speech_paths(config, model_root)?;
    for path in [&encoder, &decoder, &joiner, &tokens] {
        require_file(path)?;
    }
    // Matches the official sherpa-onnx 1.13.8 Parakeet-v3 Rust example.
    Ok(OfflineRecognizerConfig {
        model_config: OfflineModelConfig {
            transducer: OfflineTransducerModelConfig {
                encoder: Some(path_string(&encoder)?),
                decoder: Some(path_string(&decoder)?),
                joiner: Some(path_string(&joiner)?),
            },
            tokens: Some(path_string(&tokens)?),
            model_type: Some("nemo_transducer".into()),
            provider: Some("cpu".into()),
            num_threads: worker_threads(),
            debug: false,
            ..Default::default()
        },
        decoding_method: Some("greedy_search".into()),
        ..Default::default()
    })
}

pub(crate) fn decode_with_recognizer(
    recognizer: &OfflineRecognizer,
    audio: &MonoPcm,
    cancelled: &AtomicBool,
    progress: &dyn Fn(TranscriptionProgress),
) -> Result<String> {
    audio.require_sample_rate(ASR_SAMPLE_RATE)?;
    check_cancelled(cancelled)?;
    if audio.samples().is_empty() {
        return Ok(String::new());
    }
    segmented_audio::decode_segments_with_limit(
        audio.samples(),
        cancelled,
        MAX_SEGMENT_SECONDS * ASR_SAMPLE_RATE as usize,
        |samples| {
            // Fresh stream per segment; only the recognizer weights persist.
            let stream = recognizer.create_stream();
            stream.accept_waveform(ASR_SAMPLE_RATE as i32, samples);
            check_cancelled(cancelled)?;
            recognizer.decode(&stream);
            check_cancelled(cancelled)?;
            let result = stream
                .get_result()
                .ok_or_else(|| native("speech decode", "no recognition result was returned"))?;
            Ok(result.text.trim().to_owned())
        },
        progress,
    )
}

/// Built-in Parakeet v3 CPU artifacts: 4-bit weight-only (MatMulNBits) exports.
/// Dynamic INT8 lost accuracy on held-out FLEURS (#251).
pub(crate) const BUILTIN_DIRECTORY: &str = "parakeet-tdt-0.6b-v3-wo4";
pub(crate) const BUILTIN_FILES: [&str; 4] =
    ["encoder.onnx", "decoder.onnx", "joiner.onnx", "tokens.txt"];

pub(crate) fn speech_paths(config: &AppConfig, root: &Path) -> Result<[std::path::PathBuf; 4]> {
    config.validate()?;
    let roles = [
        ModelFileRole::Encoder,
        ModelFileRole::Decoder,
        ModelFileRole::Joiner,
        ModelFileRole::Tokens,
    ];
    if let Some(profile) = &config.models.custom_asr {
        Ok(roles.map(|role| {
            root.join(profile.relative_path(profile.file(role).expect("validated profile roles")))
        }))
    } else {
        Ok(BUILTIN_FILES.map(|file| root.join(BUILTIN_DIRECTORY).join(file)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancellation_and_unsupported_models_fail_before_native_loading() {
        let audio = MonoPcm::new(vec![0.0; 16], ASR_SAMPLE_RATE).unwrap();
        assert!(matches!(
            transcribe(
                &audio,
                &AppConfig::default(),
                Path::new("missing"),
                &AtomicBool::new(true)
            ),
            Err(InferenceError::Cancelled)
        ));
        let mut config = AppConfig::default();
        config.models.asr = AsrModel::Nemotron35AsrStreaming06b;
        assert!(matches!(
            transcribe(
                &audio,
                &config,
                Path::new("missing"),
                &AtomicBool::new(false)
            ),
            Err(InferenceError::UnsupportedAsr(_))
        ));
    }
    #[test]
    fn wrong_sample_rate_and_missing_models_return_actionable_errors() {
        let cancelled = AtomicBool::new(false);
        let mut config = AppConfig::default();
        config.models.asr = AsrModel::ParakeetTdt06bV3;
        let wrong_rate = MonoPcm::new(vec![0.0; 16], 48_000).unwrap();
        assert!(matches!(
            transcribe(&wrong_rate, &config, Path::new("missing"), &cancelled),
            Err(InferenceError::Configuration(_))
        ));
        let audio = MonoPcm::new(vec![0.0; 16], ASR_SAMPLE_RATE).unwrap();
        assert!(matches!(
            transcribe(&audio, &config, Path::new("missing"), &cancelled),
            Err(InferenceError::MissingModel(_))
        ));
    }
}
