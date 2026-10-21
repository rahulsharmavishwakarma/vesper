//! Native speech recognition and GGUF transcript cleanup through model adapters.
//!
//! The desktop's bounded inference worker owns resident Nemotron and Qwen model
//! weights. Each recording gets a fresh NeMo stream and each cleanup gets a fresh
//! llama.cpp context so request state cannot cross operation boundaries.

mod adapters;
mod artifact_identity;
pub mod audio_frontend;
mod automatic;
mod backend;
mod capabilities;
pub mod diarization;
mod fidelity;
mod lfm;
mod live_preview;
mod llama_runtime;
mod manager;
#[cfg(feature = "mlx")]
mod mlx;
mod nemotron;
mod offline_asr;
mod parakeet;
mod personalization;
#[cfg(test)]
mod qualification;
mod qwen;
mod resident_sherpa;
mod resources;
mod segmented_audio;
mod whisper;

use serde::Serialize;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use vesper_audio::MonoPcm;
use vesper_core::{AdapterKind, AppConfig, RewriteQuantization};

pub use capabilities::{accelerator_capabilities, AcceleratorCapabilities};
pub use manager::{
    InferenceManager, StreamingFinal, StreamingSession, StreamingTranscript, StreamingUpdateSink,
};
pub use qwen::rewrite;
pub type Result<T> = std::result::Result<T, InferenceError>;

/// Completed audio coverage, without publishing a partial transcript as successful.
#[derive(Debug, Clone, Copy)]
pub struct TranscriptionProgress {
    pub processed_samples: usize,
    pub total_samples: usize,
}

pub type TranscriptionProgressSink =
    std::sync::Arc<dyn Fn(TranscriptionProgress) + Send + Sync + 'static>;

#[derive(Debug, thiserror::Error)]
pub enum InferenceError {
    #[error("accelerator initialization failed: {0}")]
    AcceleratorInitialization(String),
    #[error("inference was cancelled")]
    Cancelled,
    #[error(transparent)]
    Configuration(#[from] vesper_core::Error),
    #[error("speech inference for {0} is not implemented by the selected runtime")]
    UnsupportedAsr(&'static str),
    #[error("required model file is missing or empty: {0}")]
    MissingModel(PathBuf),
    #[error("model path must be valid UTF-8 without null bytes: {0}")]
    InvalidPath(PathBuf),
    #[error("{stage} failed: {message}")]
    Native {
        stage: &'static str,
        message: String,
    },
    #[error(
        "transcript is too long for the writing model context; the raw transcript is preserved"
    )]
    PromptTooLong,
    #[error("transcript or instruction contains reserved model control tokens; the raw transcript is preserved")]
    ReservedControlToken,
    #[error("the writing model reached its output limit; partial output has been discarded")]
    OutputLimit,
    #[error("the writing model returned no usable text")]
    EmptyRewrite,
    #[error("polishing took longer than {0} seconds; the raw transcript is preserved")]
    TimeLimit(u64),
    #[error(
        "the polished transcript failed the fidelity check ({0}); the raw transcript is preserved"
    )]
    Fidelity(&'static str),
    /// A bounded live-preview window stopped before its decoder completed. The
    /// draft is skipped; the owner stays healthy and the final is unaffected.
    #[error("preview window stopped before completion ({0}); the draft was skipped")]
    IncompletePreview(String),
}

pub fn transcribe(
    audio: &MonoPcm,
    config: &AppConfig,
    model_root: &Path,
    cancelled: &AtomicBool,
) -> Result<String> {
    config.validate()?;
    let progress: TranscriptionProgressSink = std::sync::Arc::new(|_| {});
    let mut frontend = audio_frontend::FrontendCache::default();
    let processed = if config.audio_frontend.enabled() {
        Some(
            audio_frontend::process_completed(&mut frontend, audio, config, model_root, cancelled)?
                .0,
        )
    } else {
        None
    };
    let mut selection = vesper_models::resolve_speech(config, model_root)
        .map_err(resident_sherpa::verification_error)?;
    let model = automatic::InitializationFallback::default().load(
        &mut selection,
        model_root,
        cancelled,
        |effective| {
            if effective.uses_mlx_speech() {
                adapters::SpeechModel::load(effective, model_root, cancelled).map(Some)
            } else {
                // Retain each existing one-shot adapter's admission and exact-zero
                // shortcuts. Only MLX needs a separately retryable initialization.
                Ok(None)
            }
        },
    )?;
    let Some(model) = model else {
        return adapters::transcribe_once(
            selection.config.speech_adapter().kind,
            processed.as_ref().unwrap_or(audio),
            &selection.config,
            model_root,
            cancelled,
            &progress,
        );
    };
    model.transcribe(
        processed.as_ref().unwrap_or(audio),
        &selection.config,
        model_root,
        "standalone",
        cancelled,
        &progress,
        &Default::default(),
    )
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PipelineOutput {
    pub raw: String,
    pub cleaned: Option<String>,
    pub cleanup_error: Option<String>,
    pub model_versions: Value,
    pub speaker_timeline: Option<vesper_core::SpeakerTimeline>,
    pub speaker_error: Option<String>,
}

/// Convenience pipeline. Applications can call each stage directly to save the
/// raw transcript before beginning cleanup or to emit progress events.
pub fn run(
    audio: &MonoPcm,
    config: &AppConfig,
    model_root: &Path,
    cancelled: &AtomicBool,
) -> Result<PipelineOutput> {
    let raw = transcribe(audio, config, model_root, cancelled)?;
    let (cleaned, cleanup_error) = if config.cleanup_enabled && !raw.trim().is_empty() {
        match rewrite(&raw, config, model_root, cancelled) {
            Ok(text) => (Some(text), None),
            Err(error) => (None, Some(error.to_string())),
        }
    } else {
        (None, None)
    };
    let (speaker_timeline, speaker_error) =
        if vesper_models::diarization::available(model_root, config.compute_backend) {
            match diarization::analyze(audio, config, model_root, cancelled) {
                Ok(timeline) => (Some(timeline), None),
                Err(InferenceError::Cancelled) => return Err(InferenceError::Cancelled),
                Err(error) => (None, Some(error.to_string())),
            }
        } else {
            (None, None)
        };
    Ok(PipelineOutput {
        raw,
        cleaned,
        cleanup_error,
        model_versions: model_versions(config),
        speaker_timeline,
        speaker_error,
    })
}

/// Selected runtime/artifact identifiers, not an installation attestation.
/// The model installer records actual artifact hashes separately.
pub fn model_versions(config: &AppConfig) -> Value {
    let requested = config;
    let preferred = vesper_models::preferred_speech_config(config);
    let config = preferred.as_ref().unwrap_or(config);
    let asr = if config.uses_mlx_speech() {
        json!({"modelId":config.models.asr.id(),"runtime":config.speech_adapter().runtime,"runtimeRevision":config.speech_adapter().runtime_revision,"provider":"metal","modelLifetime":"resident-in-manager-fresh-segment-decoder", "streamingMode":"completed-recordings", "language":config.language,"artifacts":vesper_models::selected(config).unwrap_or_default().into_iter().filter(|a| a.role == "asr").collect::<Vec<_>>()})
    } else if config.models.custom_asr.is_none() {
        match config.speech_adapter().kind {
            AdapterKind::Whisper => {
                json!({"modelId":config.models.asr.id(), "revision":whisper::export(config.models.asr).1,
                    "runtime":"sherpa-onnx", "runtimeVersion":"1.13.8",
                    "runtimeBuildId":env!("VESPER_SHERPA_BUILD_ID"), "runtimePatchSha256":env!("VESPER_SHERPA_PATCH_SHA256"),
                    "completionReportVersion":1, "provider":"cpu", "modelLifetime":"resident-in-manager-fresh-stream-per-segment",
                    "streamingMode":"completed-recordings", "maxRecordingSeconds":0,
                    "decoderBudget":{"policy":"half-text-context-with-kv-bound", "maxNewTokens":224},
                    "tailPaddingFrames":whisper::TAIL_PADDING_FRAMES,
                    "silencePolicy":{"policy":"combined-native-no-speech-and-greedy-logprob", "noSpeechThreshold":whisper::NO_SPEECH_THRESHOLD, "logProbabilityThreshold":whisper::LOG_PROBABILITY_THRESHOLD},
                    "segmentation":{"policy":"balanced-low-energy-v1", "maxSegmentSeconds":whisper::MAX_SEGMENT_SECONDS, "boundaryLookbackMs":1000, "energyWindowMs":100, "boundaryStepMs":10, "overlapSamples":0},
                    "language":config.language,
                    "artifacts":vesper_models::selected(config).unwrap_or_default().into_iter().filter(|file| file.role == "asr").collect::<Vec<_>>()})
            }
            AdapterKind::QwenAsr | AdapterKind::CohereAsr => {
                json!({"modelId":config.models.asr.id(), "revision":if config.speech_adapter().kind == AdapterKind::QwenAsr { offline_asr::QWEN_REVISION } else { offline_asr::COHERE_REVISION }, "runtime":"sherpa-onnx", "runtimeVersion":"1.13.8", "runtimeRevision":"11afbd009a7f8c08f4bcf2fc1b265d0df4670fbf", "runtimeBuildId":env!("VESPER_SHERPA_BUILD_ID"), "runtimePatchSha256":env!("VESPER_SHERPA_PATCH_SHA256"), "completionReportVersion":1, "provider":"cpu", "modelLifetime":if config.speech_adapter().kind == AdapterKind::CohereAsr { "resident-in-manager-fresh-stream-per-segment" } else { "per-job" }, "maxRecordingSeconds":0, "decoderBudget":if config.speech_adapter().kind == AdapterKind::QwenAsr { json!({"maxTotalTokens":offline_asr::QWEN_CONTEXT_TOKENS,"maxNewTokens":offline_asr::QWEN_CONTEXT_TOKENS}) } else { serde_json::Value::Null }, "segmentation":{"policy":"balanced-low-energy-v1", "maxSegmentSeconds":offline_asr::MAX_SEGMENT_SECONDS, "boundaryLookbackMs":1000, "energyWindowMs":100, "boundaryStepMs":10, "overlapSamples":0, "streamLifetime":"per-segment"}, "language":config.language, "artifacts":vesper_models::selected(config).unwrap_or_default().into_iter().filter(|file| file.role == "asr").collect::<Vec<_>>()})
            }
            AdapterKind::LfmAudio => {
                json!({"modelId":config.models.asr.id(), "revision":lfm::REVISION, "runtime":"llama-cpp-2/mtmd", "runtimeVersion":"0.1.159", "provider":qwen::provider(config), "modelLifetime":"resident-weights-fresh-context-per-segment", "contextTokens":lfm::CONTEXT_TOKENS, "maxOutputTokens":lfm::MAX_OUTPUT_TOKENS, "sampling":{"strategy":"greedy"}, "languageMode":"explicit-english", "streamingMode":"completed-recordings", "segmentation":{"policy":"balanced-low-energy-v1", "maxSegmentSeconds":lfm::MAX_SEGMENT_SECONDS, "boundaryLookbackMs":1000, "energyWindowMs":100, "boundaryStepMs":10, "overlapSamples":0}, "cancellation":"before-and-after-audio-evaluation-and-between-text-tokens", "artifacts":vesper_models::selected(config).unwrap_or_default().into_iter().filter(|f| f.role == "asr").collect::<Vec<_>>()})
            }
            AdapterKind::NemoRnnt => {
                let provider =
                    vesper_models::resolved_nemo_backend(config).unwrap_or("unavailable");
                json!({"modelId": config.models.asr.id(), "runtime":"NeMo-Speech.cpp", "runtimeVersion":"0.1.0", "interface":"versioned-c-abi", "provider":provider, "languageMode":"prompted-or-automatic", "streamingMode":"native-live-microphone-and-offline-file", "modelLifetime":"resident-until-selection-change-or-exit", "artifactFiles":[nemotron::MODEL_FILENAME]})
            }
            AdapterKind::SherpaTransducer => {
                json!({"modelId": config.models.asr.id(), "runtime":"sherpa-onnx", "runtimeVersion":"1.13.8", "provider":"cpu", "languageMode":"automatic", "segmentation":{"policy":"balanced-low-energy-v1", "maxSegmentSeconds":parakeet::MAX_SEGMENT_SECONDS, "boundaryLookbackMs":1000, "energyWindowMs":100, "boundaryStepMs":10, "overlapSamples":0, "streamLifetime":"per-segment"}, "artifactFiles":parakeet::BUILTIN_FILES})
            }
            _ => unreachable!("speech task registration"),
        }
    } else {
        json!({"modelId": config.models.asr.id(), "runtime":"sherpa-onnx", "runtimeVersion":"1.13.8", "provider":"cpu", "languageMode":"automatic", "segmentation":{"policy":"balanced-low-energy-v1", "maxSegmentSeconds":parakeet::MAX_SEGMENT_SECONDS, "boundaryLookbackMs":1000, "energyWindowMs":100, "boundaryStepMs":10, "overlapSamples":0, "streamLifetime":"per-segment"}, "artifactFiles":parakeet::BUILTIN_FILES})
    };
    let speech_registration = config.speech_adapter();
    let config = requested;
    let mut versions = json!({
        "selectionError": preferred.as_ref().err().map(ToString::to_string),
        "audioFrontend": {"settings":config.audio_frontend,"sampleRate":16000,"provider":"cpu","nativeGraphThreads":1,
            "runtimeBuildId":env!("VESPER_SHERPA_BUILD_ID"), "runtimePatchSha256":env!("VESPER_SHERPA_PATCH_SHA256"),
            "vadPolicy":audio_frontend::vad_policy(&config.audio_frontend), "denoiserApi":"online-in-both-modes",
            "artifacts":vesper_models::selected(config).unwrap_or_default().into_iter().filter(|a| a.role == "audio_frontend").collect::<Vec<_>>()},
        "asr": asr,
        "rewriter": {"modelId":config.models.rewriter.id(), "runtime":"llama-cpp-2", "runtimeVersion":"0.1.159", "artifactFile":rewrite_filename(config.rewrite_quantization), "provider":qwen::provider(config), "modelLifetime":"resident-weights-fresh-context-per-request", "thinkingEnabled":false, "contextTokens":qwen::CONTEXT_TOKENS, "maxOutputTokens":qwen::MAX_OUTPUT_TOKENS, "sampling":{"strategy":"greedy"}, "fidelityGuard":"lexical-v1"}
    });
    if let Some(profile) = &config.models.custom_asr {
        versions["asr"]["modelId"] = json!(profile.repository);
        versions["asr"]["profileId"] = json!(profile.id);
        versions["asr"]["revision"] = json!(profile.revision);
        versions["asr"]["modelType"] = json!(profile.model_type);
        versions["asr"]["artifactFiles"] = json!(profile.files);
    }
    if !config.cleanup_enabled {
        versions["rewriter"] = Value::Null;
    } else if let Some(profile) = &config.models.custom_rewriter {
        versions["rewriter"]["modelId"] = json!(profile.repository);
        versions["rewriter"]["profileId"] = json!(profile.id);
        versions["rewriter"]["revision"] = json!(profile.revision);
        versions["rewriter"]["artifactFile"] = json!(profile
            .file(vesper_core::ModelFileRole::Weights)
            .map(|file| &file.path));
        versions["rewriter"]["thinkingEnabled"] = Value::Null;
        versions["rewriter"]["chatTemplate"] =
            json!("embedded GGUF template; bundled llama.cpp template engine");
        versions["rewriter"]["contextTokens"] = Value::Null;
        versions["rewriter"]["maxContextTokens"] = json!(qwen::CONTEXT_TOKENS);
    }
    for (role, registration) in [
        ("asr", speech_registration),
        ("rewriter", config.text_adapter()),
    ] {
        if !versions[role].is_null() {
            versions[role]["adapter"] = json!(registration);
            versions[role]["runtimeRevision"] = json!(registration.runtime_revision);
            versions[role]["requestedBackend"] = json!(config.compute_backend);
            versions[role]["selectedProvider"] = versions[role]["provider"].clone();
            versions[role]["provider"] = Value::Null;
            versions[role]["executionEvidence"] = json!("not-observed");
        }
    }

    versions
}

pub fn rewrite_filename(quantization: RewriteQuantization) -> &'static str {
    match quantization {
        RewriteQuantization::Q4_0 => "Qwen3.5-0.8B-Q4_0.gguf",
        RewriteQuantization::Q8_0 => "Qwen3.5-0.8B-Q8_0.gguf",
    }
}

fn check_cancelled(cancelled: &AtomicBool) -> Result<()> {
    if cancelled.load(Ordering::Relaxed) {
        Err(InferenceError::Cancelled)
    } else {
        Ok(())
    }
}

fn require_file(path: &Path) -> Result<()> {
    if !path
        .metadata()
        .is_ok_and(|metadata| metadata.is_file() && metadata.len() > 0)
    {
        return Err(InferenceError::MissingModel(path.to_owned()));
    }
    Ok(())
}

fn path_string(path: &Path) -> Result<String> {
    path.to_str()
        .filter(|value| !value.contains('\0'))
        .map(str::to_owned)
        .ok_or_else(|| InferenceError::InvalidPath(path.to_owned()))
}

fn native(stage: &'static str, error: impl std::fmt::Display) -> InferenceError {
    InferenceError::Native {
        stage,
        message: error.to_string(),
    }
}

fn worker_threads() -> i32 {
    std::thread::available_parallelism()
        .map_or(2, |count| count.get().saturating_sub(2).clamp(1, 8) as i32)
}

#[cfg(test)]
mod profile_tests {
    use super::*;
    use vesper_core::{HuggingFaceFile, HuggingFaceProfile, ModelFileRole, ModelRuntime};

    #[test]
    fn runtime_paths_and_versions_follow_selected_profiles() {
        let root = Path::new("local-models");
        let profile = HuggingFaceProfile {
            id: "alternative".into(),
            name: "Alternative".into(),
            repository: "owner/alternative".into(),
            revision: "c".repeat(40),
            runtime: ModelRuntime::LlamaCpp,
            files: vec![HuggingFaceFile {
                role: ModelFileRole::Weights,
                path: "quantized/tiny.gguf".into(),
                sha256: "d".repeat(64),
                size_bytes: 100,
            }],
            license: "fixture".into(),
            model_type: None,
            languages: vec!["en".into()],
        };
        let mut config = AppConfig::default();
        config.models.custom_rewriter = Some(profile.clone());
        assert_eq!(
            qwen::rewrite_path(&config, root).unwrap(),
            root.join(profile.relative_path(&profile.files[0]))
        );
        let versions = model_versions(&config);
        assert_eq!(versions["rewriter"]["modelId"], "owner/alternative");
        assert_eq!(versions["rewriter"]["artifactFile"], "quantized/tiny.gguf");
        assert!(versions["rewriter"]["thinkingEnabled"].is_null());
        let mut speech = profile;
        speech.runtime = ModelRuntime::SherpaOnnxTransducer;
        speech.model_type = Some("nemo_transducer".into());
        speech.files = [
            (ModelFileRole::Encoder, "a.onnx"),
            (ModelFileRole::Decoder, "b.onnx"),
            (ModelFileRole::Joiner, "c.onnx"),
            (ModelFileRole::Tokens, "d.txt"),
        ]
        .map(|(role, path)| HuggingFaceFile {
            role,
            path: path.into(),
            sha256: "d".repeat(64),
            size_bytes: 100,
        })
        .to_vec();
        config.models.custom_asr = Some(speech.clone());
        let expected = speech
            .files
            .iter()
            .map(|file| root.join(speech.relative_path(file)))
            .collect::<Vec<_>>();
        assert_eq!(
            parakeet::speech_paths(&config, root).unwrap().as_slice(),
            expected
        );
        assert_eq!(model_versions(&config)["asr"]["revision"], "c".repeat(40));
        config.cleanup_enabled = false;
        assert!(model_versions(&config)["rewriter"].is_null());
    }
}
