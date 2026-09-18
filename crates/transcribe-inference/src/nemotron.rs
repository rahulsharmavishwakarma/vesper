use std::{path::Path, sync::atomic::AtomicBool};

use transcribe_audio::MonoPcm;
use transcribe_core::{AppConfig, ComputeBackend, Language};

use crate::{check_cancelled, native, require_file, Result};

pub(crate) const MODEL_FILENAME: &str = "nemotron-3.5-asr-streaming-0.6b.q8_0.gguf";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RecognizerKey {
    library: std::path::PathBuf,
    model: std::path::PathBuf,
    gpu: Option<u32>,
    requested_backend: ComputeBackend,
}

pub(crate) fn key(config: &AppConfig, model_root: &Path) -> Result<RecognizerKey> {
    config.validate()?;
    let backend = transcribe_models::resolved_nemo_backend(config)
        .map_err(|error| native("selecting NeMo-Speech.cpp backend", error))?;
    Ok(RecognizerKey {
        library: transcribe_models::nemo_speech_library(model_root),
        model: model_root
            .join("nemotron-3.5-asr-streaming-0.6b")
            .join(MODEL_FILENAME),
        gpu: (backend != "cpu").then_some(0),
        requested_backend: config.compute_backend,
    })
}

pub(crate) struct Recognizer {
    inner: transcribe_nemo::Recognizer,
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
        let mut initialized_provider = transcribe_models::resolved_nemo_backend(config)
            .map_err(|e| native("backend selection", e))?;
        let inner = match transcribe_nemo::Recognizer::load_with_right_context(
            &key.library,
            &key.model,
            key.gpu,
            -1,
        ) {
            Ok(inner) => inner,
            Err(gpu_error)
                if key.gpu.is_some() && config.compute_backend == ComputeBackend::Auto =>
            {
                eprintln!("automatic NeMo GPU initialization failed; retrying on CPU: {gpu_error}");
                initialized_provider = "cpu";
                transcribe_nemo::Recognizer::load_with_right_context(&key.library, &key.model, None, -1).map_err(
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
        self.transcribe_with_phrases(audio, config, request_id, cancelled, &[])
    }

    pub(crate) fn transcribe_with_phrases(
        &self,
        audio: &MonoPcm,
        config: &AppConfig,
        request_id: Option<&str>,
        cancelled: &AtomicBool,
        phrases: &[String],
    ) -> Result<String> {
        validate_audio(audio, config, cancelled)?;
        let result = self
            .inner
            .recognize_with_phrases(
                audio.samples(),
                audio.sample_rate(),
                language_code(config.language),
                request_id,
                phrases,
                2.0,
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
    ) -> Result<transcribe_nemo::Stream<'a>> {
        check_cancelled(cancelled)?;
        self.inner
            .stream_with_phrases(
                language_code(config.language),
                Some(request_id),
                phrases,
                2.0,
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

pub(crate) fn language_code(language: Language) -> Option<&'static str> {
    match language {
        Language::Auto => None,
        Language::English => Some("en-US"),
        Language::Hindi => Some("hi-IN"),
        Language::French => Some("fr-FR"),
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
