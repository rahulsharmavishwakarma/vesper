//! Task interfaces at request boundaries. No dynamic lookup inside native loops.
use crate::{
    lfm, native, nemotron, qwen, resident_sherpa, InferenceError, Result, TranscriptionProgressSink,
};
use std::{
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};
use vesper_audio::MonoPcm;
use vesper_core::{AdapterKind, AppConfig, PersonalizationProfile};

#[derive(PartialEq, Eq)]
pub(crate) enum SpeechKey {
    #[cfg(feature = "mlx")]
    Mlx(crate::mlx::RecognizerKey),
    Nemo(nemotron::RecognizerKey),
    Lfm(lfm::RecognizerKey),
    Sherpa(resident_sherpa::RecognizerKey),
    PerJob(AdapterKind, PathBuf, Box<AppConfig>),
}

pub(crate) fn speech_key(config: &AppConfig, root: &Path) -> Result<SpeechKey> {
    config.validate()?;
    require_runtime(config)?;
    #[cfg(feature = "mlx")]
    if config.uses_mlx_speech() {
        return Ok(SpeechKey::Mlx(crate::mlx::key(config, root)?));
    }
    Ok(match config.speech_adapter().kind {
        AdapterKind::NemoRnnt => SpeechKey::Nemo(nemotron::key(config, root)?),
        AdapterKind::LfmAudio => SpeechKey::Lfm(lfm::key(config, root)?),
        AdapterKind::CohereAsr
        | AdapterKind::Whisper
        | AdapterKind::QwenAsr
        | AdapterKind::MoonshineAsr
        | AdapterKind::SherpaTransducer => SpeechKey::Sherpa(resident_sherpa::key(config, root)?),
        kind => SpeechKey::PerJob(kind, root.to_owned(), Box::new(config.clone())),
    })
}

/// Native resources are constructed, used and dropped on their owning worker.
/// Per-job adapters keep their established lifetimes; registration never claims
/// residency or native streaming merely because a checkpoint supports it.
pub(crate) enum SpeechModel {
    #[cfg(feature = "mlx")]
    Mlx(crate::mlx::Recognizer),
    Nemo(nemotron::Recognizer),
    Lfm(lfm::Recognizer),
    Sherpa(resident_sherpa::Recognizer),
    PerJob(AdapterKind),
}

/// The manager handles PCM and snapshots, never runtime-specific handles.
pub(crate) enum NativeStream<'a> {
    Nemo(vesper_nemo::Stream<'a>),
    #[cfg(feature = "mlx")]
    Mlx(Box<crate::mlx::Live<'a>>),
}

pub(crate) struct LiveSnapshot {
    pub text: String,
    pub audio_processed_seconds: f32,
    pub is_final: bool,
}

impl NativeStream<'_> {
    pub fn push(&mut self, samples: &[f32], rate: u32) -> Result<()> {
        match self {
            #[cfg(feature = "mlx")]
            Self::Mlx(stream) => stream.push(samples, rate),
            Self::Nemo(stream) => stream
                .push(samples, rate)
                .map_err(|e| native("live audio", e)),
        }
    }
    pub fn finish(&mut self) -> Result<()> {
        match self {
            #[cfg(feature = "mlx")]
            Self::Mlx(stream) => {
                stream.finish();
                Ok(())
            }
            Self::Nemo(stream) => stream.finish().map_err(|e| native("live finalization", e)),
        }
    }
    pub fn poll_result(&mut self) -> Result<Option<LiveSnapshot>> {
        match self {
            #[cfg(feature = "mlx")]
            Self::Mlx(stream) => stream.poll(),
            Self::Nemo(stream) => stream
                .poll_result()
                .map(|value| {
                    value.map(|result| LiveSnapshot {
                        text: result.text,
                        audio_processed_seconds: result.audio_processed_seconds,
                        is_final: result.is_final,
                    })
                })
                .map_err(|e| native("live decode", e)),
        }
    }

    /// Input a provisional native preview skipped under backlog.
    pub fn dropped_seconds(&self) -> f32 {
        match self {
            Self::Nemo(_) => 0.0,
            #[cfg(feature = "mlx")]
            Self::Mlx(stream) => stream.dropped_seconds(),
        }
    }

    pub fn needs_completed_audio(&self) -> bool {
        match self {
            Self::Nemo(_) => false,
            #[cfg(feature = "mlx")]
            Self::Mlx(_) => true,
        }
    }
}

impl SpeechModel {
    pub(crate) fn healthy(&self) -> bool {
        match self {
            Self::Sherpa(model) => model.healthy.get(),
            #[cfg(feature = "mlx")]
            Self::Mlx(model) => model.healthy.get(),
            _ => true,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn preview(
        &self,
        audio: &MonoPcm,
        config: &AppConfig,
        root: &Path,
        id: &str,
        cancelled: &AtomicBool,
        profile: &PersonalizationProfile,
    ) -> Result<String> {
        if let Self::Sherpa(model) = self {
            return model.preview(audio, config, root, cancelled);
        }
        let progress: TranscriptionProgressSink = std::sync::Arc::new(|_| {});
        self.transcribe(audio, config, root, id, cancelled, &progress, profile)
    }

    pub(crate) fn load(config: &AppConfig, root: &Path, cancelled: &AtomicBool) -> Result<Self> {
        #[cfg(feature = "mlx")]
        if config.uses_mlx_speech() {
            return Ok(Self::Mlx(crate::mlx::Recognizer::load(
                config, root, cancelled,
            )?));
        }
        require_runtime(config)?;
        match config.speech_adapter().kind {
            AdapterKind::CohereAsr
            | AdapterKind::Whisper
            | AdapterKind::QwenAsr
            | AdapterKind::MoonshineAsr
            | AdapterKind::SherpaTransducer => Ok(Self::Sherpa(resident_sherpa::Recognizer::load(
                config, root, cancelled,
            )?)),
            AdapterKind::NemoRnnt => {
                vesper_models::verify_asr(config, root, cancelled)
                    .map_err(|e| native("speech model verification", e))?;
                Ok(Self::Nemo(nemotron::Recognizer::load(
                    config, root, cancelled,
                )?))
            }
            AdapterKind::LfmAudio => {
                vesper_models::verify_asr(config, root, cancelled)
                    .map_err(|e| native("speech model verification", e))?;
                Ok(Self::Lfm(lfm::Recognizer::load(config, root, cancelled)?))
            }
            kind => Ok(Self::PerJob(kind)),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn transcribe(
        &self,
        audio: &MonoPcm,
        config: &AppConfig,
        root: &Path,
        id: &str,
        cancelled: &AtomicBool,
        progress: &TranscriptionProgressSink,
        profile: &PersonalizationProfile,
    ) -> Result<String> {
        match self {
            #[cfg(feature = "mlx")]
            Self::Mlx(model) => model.transcribe(audio, config, root, cancelled, &**progress),
            Self::Nemo(model) => model.transcribe_with_phrases(
                audio,
                config,
                Some(id),
                cancelled,
                &profile.speech_phrases(config.language),
            ),
            Self::Lfm(model) => model.transcribe(audio, cancelled, &**progress),
            Self::Sherpa(model) => model.transcribe(audio, config, root, cancelled, &**progress),
            Self::PerJob(kind) => {
                vesper_models::verify_asr(config, root, cancelled)
                    .map_err(|e| native("speech model verification", e))?;
                transcribe_once(*kind, audio, config, root, cancelled, progress)
            }
        }
    }

    pub(crate) fn stream<'a>(
        &'a self,
        config: &AppConfig,
        id: &str,
        cancelled: &AtomicBool,
        profile: &PersonalizationProfile,
    ) -> Result<NativeStream<'a>> {
        match self {
            #[cfg(feature = "mlx")]
            Self::Mlx(model) => model.stream().map(Box::new).map(NativeStream::Mlx),
            Self::Nemo(model) => model
                .stream_with_phrases(
                    config,
                    id,
                    cancelled,
                    &profile.speech_phrases(config.language),
                )
                .map(NativeStream::Nemo),
            Self::Lfm(_) | Self::Sherpa(_) | Self::PerJob(_) => {
                Err(InferenceError::UnsupportedAsr(config.models.asr.id()))
            }
        }
    }

    pub(crate) fn evidence(
        &self,
        config: &AppConfig,
        profile: &PersonalizationProfile,
    ) -> serde_json::Value {
        #[cfg(feature = "mlx")]
        if let Self::Mlx(model) = self {
            return model.evidence(config);
        }
        if let Self::Sherpa(model) = self {
            return serde_json::json!({"requestedBackend":config.compute_backend,
                "initializedProvider":"cpu", "operatorPlacement":"unknown",
                "nativeGraphThreads":crate::worker_threads(), "residentWeights":true,
                "residentLoadOrdinal":model.load_ordinal, "verificationUs":model.verification_us,
                "modelLoadUs":model.load_us, "metrics":*model.metrics.borrow(),
                "contextualPhrasesSupported":false, "contextualPhraseCount":0,
                "evidence":"verified-resident-sherpa-fresh-segment-streams"});
        }
        if let Self::Lfm(model) = self {
            return serde_json::json!({"requestedBackend":config.compute_backend,
                "initializedProvider":model.plan.provider, "deviceIndex":model.plan.device,
                "fallbackReason":model.fallback_reason,
                "projectorProvider":if model.plan.device.is_some() { "gpu-requested" } else { "cpu" },
                "operatorPlacement":if model.plan.device.is_some() { "unknown" } else { "explicit-cpu-only" },
                "nativeGraphThreads":crate::worker_threads(), "modelLoadUs":model.load_us,
                "metrics":*model.metrics.borrow(), "contextualPhrasesSupported":false,
                "contextualPhraseCount":0, "evidence":"detected-backbone-device-and-context-probe;mtmd-placement-not-exposed"});
        }
        let initialized = match self {
            #[cfg(feature = "mlx")]
            Self::Mlx(_) => Some("metal"),
            Self::Nemo(m) => Some(m.initialized_provider),
            Self::PerJob(
                AdapterKind::SherpaTransducer
                | AdapterKind::QwenAsr
                | AdapterKind::CohereAsr
                | AdapterKind::MoonshineAsr,
            ) => Some("cpu"),
            Self::Lfm(_) | Self::Sherpa(_) | Self::PerJob(_) => None,
        };
        serde_json::json!({"requestedBackend":config.compute_backend, "initializedProvider":initialized, "nativeGraphThreads":if matches!(self, Self::Nemo(_)) { Some(4) } else { None }, "streamingPolicy":if matches!(self, Self::Nemo(_)) { serde_json::json!({"rightContextPolicy":"artifact-default", "artifactRightContextFrames":3, "artifactLeftContextFrames":56, "nominalEncoderAdvanceMs":320, "evidence":"pinned-model-and-runtime-source"}) } else { serde_json::Value::Null }, "operatorPlacement":"unknown", "profileRevision":profile.revision, "contextualPhrasesSupported":config.speech_adapter().capabilities.contextual_phrases, "contextualPhraseCount":if config.speech_adapter().capabilities.contextual_phrases { profile.speech_phrases(config.language).len() } else { 0 }, "phraseBoost":2.0, "evidence":"adapter-initialization-or-explicit-cpu-contract"})
    }
}

pub(crate) fn transcribe_once(
    kind: AdapterKind,
    audio: &MonoPcm,
    config: &AppConfig,
    root: &Path,
    cancelled: &AtomicBool,
    progress: &TranscriptionProgressSink,
) -> Result<String> {
    #[cfg(feature = "mlx")]
    if config.uses_mlx_speech() {
        return crate::mlx::Recognizer::load(config, root, cancelled)?.transcribe(
            audio,
            config,
            root,
            cancelled,
            &**progress,
        );
    }
    require_runtime(config)?;
    match kind {
        AdapterKind::NemoRnnt => nemotron::transcribe(audio, config, root, cancelled),
        AdapterKind::LfmAudio => {
            vesper_models::verify_asr(config, root, cancelled)
                .map_err(|e| native("speech model verification", e))?;
            lfm::Recognizer::load(config, root, cancelled)?.transcribe(
                audio,
                cancelled,
                &**progress,
            )
        }
        AdapterKind::SherpaTransducer => {
            crate::parakeet::transcribe(audio, config, root, cancelled)
        }
        AdapterKind::QwenAsr
        | AdapterKind::CohereAsr
        | AdapterKind::Whisper
        | AdapterKind::MoonshineAsr => crate::offline_asr::transcribe_with_progress(
            audio,
            config,
            root,
            cancelled,
            &**progress,
        ),
        _ => Err(InferenceError::UnsupportedAsr(config.models.asr.id())),
    }
}

pub(crate) enum TextModel {
    Llama(qwen::Rewriter),
}
impl TextModel {
    pub(crate) fn load(config: &AppConfig, root: &Path, cancelled: &AtomicBool) -> Result<Self> {
        vesper_models::verify_rewriter(config, root, cancelled)
            .map_err(|e| native("writing model verification", e))?;
        Ok(Self::Llama(qwen::Rewriter::load(config, root, cancelled)?))
    }
    pub(crate) fn rewrite(
        &self,
        raw: &str,
        config: &AppConfig,
        cancelled: &AtomicBool,
        profile: &PersonalizationProfile,
    ) -> Result<String> {
        match self {
            Self::Llama(model) => model.rewrite_personalized(raw, config, cancelled, profile),
        }
    }
    pub(crate) fn evidence(&self) -> serde_json::Value {
        match self {
            Self::Llama(model) => {
                serde_json::json!({"initializedProvider":model.initialized_provider, "fallbackReason":model.fallback_reason, "operatorPlacement":"unknown", "metrics":*model.metrics.borrow()})
            }
        }
    }
}

// Cargo may unify core capability features through another workspace consumer.
fn require_runtime(config: &AppConfig) -> Result<()> {
    if config.uses_mlx_speech() && !cfg!(feature = "mlx") {
        return Err(InferenceError::UnsupportedAsr(config.models.asr.id()));
    }
    Ok(())
}
