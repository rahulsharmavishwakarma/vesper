use std::{path::Path, sync::atomic::AtomicBool};

use vesper_audio::{MonoPcm, ASR_SAMPLE_RATE};
use vesper_core::{AppConfig, AsrModel, ComputeBackend, Language};

use crate::{check_cancelled, native, require_file, segmented_audio, Result};

pub(crate) const MODEL_FILENAME: &str = "nemotron-3.5-asr-streaming-0.6b.q8_0.gguf";
/// Parakeet v3 as one GGUF on every backend (`AsrEngine::UnifiedGguf`, #293).
pub(crate) const PARAKEET_GGUF_DIRECTORY: &str = "parakeet-tdt-0.6b-v3-gguf";
pub(crate) const PARAKEET_GGUF_FILENAME: &str = "parakeet-tdt-0.6b-v3.q8_0.gguf";
/// Same completed-audio part limit as the ONNX Parakeet path.
pub(crate) const PARAKEET_MAX_SEGMENT_SECONDS: usize = 30;

/// Cache-aware streaming checkpoints load with a streaming configuration; the
/// Parakeet GGUF is an offline model that the pinned runtime cannot stream.
fn streaming_model(config: &AppConfig) -> bool {
    !config.uses_nemo_gguf()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RecognizerKey {
    library: std::path::PathBuf,
    model: std::path::PathBuf,
    gpu: Option<u32>,
    requested_backend: ComputeBackend,
}

pub(crate) fn key(config: &AppConfig, model_root: &Path) -> Result<RecognizerKey> {
    config.validate()?;
    let backend = vesper_models::resolved_nemo_backend(config)
        .map_err(|error| native("selecting NeMo-Speech.cpp backend", error))?;
    Ok(RecognizerKey {
        library: vesper_models::nemo_speech_library(model_root),
        model: if config.uses_nemo_gguf() {
            model_root
                .join(PARAKEET_GGUF_DIRECTORY)
                .join(PARAKEET_GGUF_FILENAME)
        } else {
            model_root
                .join("nemotron-3.5-asr-streaming-0.6b")
                .join(MODEL_FILENAME)
        },
        gpu: (backend != "cpu").then_some(0),
        requested_backend: config.compute_backend,
    })
}

pub(crate) struct Recognizer {
    inner: vesper_nemo::Recognizer,
    pub(crate) initialized_provider: &'static str,
}

impl Recognizer {
    pub(crate) fn load(
        config: &AppConfig,
        model_root: &Path,
        cancelled: &AtomicBool,
    ) -> Result<Self> {
        check_cancelled(cancelled)?;
        let key = key(config, model_root)?;
        require_file(&key.library)?;
        require_file(&key.model)?;
        let mut initialized_provider = vesper_models::resolved_nemo_backend(config)
            .map_err(|e| native("backend selection", e))?;
        let streaming = streaming_model(config);
        let open = |gpu: Option<u32>| {
            if streaming {
                vesper_nemo::Recognizer::load_with_right_context(&key.library, &key.model, gpu, -1)
            } else {
                vesper_nemo::Recognizer::load(&key.library, &key.model, gpu)
            }
        };
        let inner = match open(key.gpu) {
            Ok(inner) => inner,
            Err(gpu_error)
                if key.gpu.is_some() && config.compute_backend == ComputeBackend::Auto =>
            {
                eprintln!("automatic NeMo GPU initialization failed; retrying on CPU: {gpu_error}");
                initialized_provider = "cpu";
                open(None).map_err(
                    |cpu_error| {
                        native(
                            "NeMo-Speech.cpp model load",
                            format!(
                                "automatic GPU attempt failed ({gpu_error}); CPU fallback failed ({cpu_error})"
                            ),
                        )
                    },
                )?
            }
            Err(error) => return Err(native("NeMo-Speech.cpp model load", error)),
        };
        check_cancelled(cancelled)?;
        Ok(Self {
            inner,
            initialized_provider,
        })
    }

    pub(crate) fn transcribe(
        &self,
        audio: &MonoPcm,
        config: &AppConfig,
        request_id: Option<&str>,
        cancelled: &AtomicBool,
    ) -> Result<String> {
        self.transcribe_with_phrases(audio, config, request_id, cancelled, &[], 2.0)
    }

    pub(crate) fn transcribe_with_phrases(
        &self,
        audio: &MonoPcm,
        config: &AppConfig,
        request_id: Option<&str>,
        cancelled: &AtomicBool,
        phrases: &[String],
        boost: f32,
    ) -> Result<String> {
        validate_audio(audio, config, cancelled)?;
        // Callers pass `recognizer_hints`, already gated by hook status.
        let phrases = if config.speech_adapter().capabilities.speech_hook.kind
            == vesper_core::SpeechHookKind::PhraseBias
        {
            phrases
        } else {
            &[]
        };
        if !streaming_model(config) {
            // Offline model: bounded parts with the shared planner, automatic language.
            audio.require_sample_rate(ASR_SAMPLE_RATE)?;
            return segmented_audio::decode_segments_with_limit(
                audio.samples(),
                cancelled,
                PARAKEET_MAX_SEGMENT_SECONDS * ASR_SAMPLE_RATE as usize,
                |samples| {
                    let result = self
                        .inner
                        .recognize_with_phrases(
                            samples,
                            ASR_SAMPLE_RATE,
                            None,
                            request_id,
                            &[],
                            2.0,
                        )
                        .map_err(|error| native("NeMo-Speech.cpp transcription", error))?;
                    check_cancelled(cancelled)?;
                    Ok(result.text)
                },
                &|_| {},
            );
        }
        let result = self
            .inner
            .recognize_with_phrases(
                audio.samples(),
                audio.sample_rate(),
                language_code(config.language),
                request_id,
                phrases,
                boost,
            )
            .map_err(|error| native("NeMo-Speech.cpp transcription", error))?;
        check_cancelled(cancelled)?;
        Ok(result.text)
    }

    pub(crate) fn stream_with_phrases<'a>(
        &'a self,
        config: &AppConfig,
        request_id: &str,
        cancelled: &AtomicBool,
        phrases: &[String],
        boost: f32,
    ) -> Result<vesper_nemo::Stream<'a>> {
        check_cancelled(cancelled)?;
        self.inner
            .stream_with_phrases(
                language_code(config.language),
                Some(request_id),
                phrases,
                boost,
            )
            .map_err(|error| native("NeMo-Speech.cpp streaming start", error))
    }

    #[allow(dead_code)]
    pub(crate) fn version(&self) -> String {
        self.inner.version()
    }
}

fn validate_audio(_audio: &MonoPcm, config: &AppConfig, cancelled: &AtomicBool) -> Result<()> {
    config.validate()?;
    check_cancelled(cancelled)?;
    Ok(())
}

/// Vendor locale per language from models/languages.json (the first-listed
/// locale for each base code). Unmapped modes fall back to automatic
/// detection; AppConfig::validate rejects undeclared combinations first.
pub(crate) fn language_code(language: Language) -> Option<&'static str> {
    match language {
        Language::Auto => None,
        language => vesper_core::runtime_language_code(
            "nemo",
            AsrModel::Nemotron35AsrStreaming06b.id(),
            language.code().expect("explicit language has an ISO code"),
        ),
    }
}

/// One-shot compatibility API. The desktop manager retains this recognizer.
pub(crate) fn transcribe(
    audio: &MonoPcm,
    config: &AppConfig,
    model_root: &Path,
    cancelled: &AtomicBool,
) -> Result<String> {
    Recognizer::load(config, model_root, cancelled)?.transcribe(audio, config, None, cancelled)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_prompts_are_explicit_and_auto_remains_unforced() {
        assert_eq!(language_code(Language::Auto), None);
        assert_eq!(language_code(Language::English), Some("en-US"));
        assert_eq!(language_code(Language::Hindi), Some("hi-IN"));
        assert_eq!(language_code(Language::French), Some("fr-FR"));
        // First-listed vendor locale per base code; undeclared checkpoints
        // fall back to automatic detection.
        assert_eq!(language_code(Language::Spanish), Some("es-US"));
        assert_eq!(language_code(Language::Portuguese), Some("pt-BR"));
        assert_eq!(language_code(Language::Chinese), Some("zh-CN"));
        assert_eq!(language_code(Language::NorwegianBokmal), Some("nb-NO"));
        assert_eq!(language_code(Language::Arabic), Some("ar-AR"));
        assert_eq!(language_code(Language::Slovenian), None);
        assert_eq!(language_code(Language::Cantonese), None);
    }

    #[test]
    fn language_codes_match_the_manifest_for_every_nemotron_language() {
        // Manifest-driven: extending Nemotron support extends this coverage
        // with no test edit.
        let manifest: serde_json::Value =
            serde_json::from_str(include_str!("../../../models/languages.json")).unwrap();
        let entry = &manifest["nvidia/nemotron-3.5-asr-streaming-0.6b"];
        let mut count = 0;
        for (iso, expected) in entry["runtimeCodes"]["nemo"].as_object().unwrap() {
            let language: Language = serde_json::from_value(iso.clone().into()).unwrap();
            assert_eq!(
                language_code(language),
                Some(expected.as_str().unwrap()),
                "{iso}"
            );
            count += 1;
        }
        assert_eq!(
            count,
            entry["languages"].as_array().unwrap().len(),
            "every offered language needs a NeMo locale"
        );
    }

    #[test]
    fn cancellation_precedes_native_loading() {
        let audio = MonoPcm::new(vec![], 16_000).unwrap();
        assert!(matches!(
            transcribe(
                &audio,
                &AppConfig::default(),
                Path::new("missing"),
                &AtomicBool::new(true)
            ),
            Err(crate::InferenceError::Cancelled)
        ));
    }
}
