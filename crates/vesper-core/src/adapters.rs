//! Compiled architecture contracts. Model repositories supply data, never code.
//! Resolve at request/load boundaries; native runtimes own all frame/token kernels.
use crate::{AppConfig, AsrEngine, AsrModel, ComputeBackend, Language, ModelRole};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AdapterKind {
    NemoRnnt,
    NemoTdt,
    SherpaTransducer,
    QwenAsr,
    CohereAsr,
    Whisper,
    MoonshineAsr,
    GraniteCtc,
    PackedTdt,
    TernaryTdt,
    NeuralEngineTdt,
    LfmAudio,
    QwenHybrid,
    LlamaGguf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CacheState {
    Rnnt,
    EncoderDecoder,
    HybridRecurrentKv,
    ModelDefined,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdapterCapabilities {
    pub offline: bool,
    pub native_streaming: bool,
    pub contextual_phrases: bool,
    pub completion_report: bool,
    pub cache_state: CacheState,
    /// Runtime capability; activation still requires local equivalence tests.
    pub complete_prefix_state: bool,
    pub languages: &'static [Language],
    pub backends: &'static [ComputeBackend],
    pub resident_weights: bool,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdapterRegistration {
    pub kind: AdapterKind,
    pub task: ModelRole,
    pub architecture: &'static str,
    pub runtime: &'static str,
    pub runtime_revision: &'static str,
    pub capabilities: AdapterCapabilities,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveMode {
    Native,
    Buffered,
    Unavailable,
}

impl AdapterRegistration {
    pub fn live_mode(&self) -> LiveMode {
        if self.task != ModelRole::SpeechRecognition || !self.capabilities.offline {
            LiveMode::Unavailable
        } else if self.capabilities.native_streaming {
            LiveMode::Native
        } else {
            LiveMode::Buffered
        }
    }
}

const CPU: &[ComputeBackend] = &[ComputeBackend::Auto, ComputeBackend::Cpu];
const ALL_BACKENDS: &[ComputeBackend] = &[
    ComputeBackend::Auto,
    ComputeBackend::Cpu,
    ComputeBackend::Metal,
    ComputeBackend::Cuda,
    ComputeBackend::Vulkan,
    ComputeBackend::Rocm,
];
/// Every language mode the app can offer. Text adapters stay on this full
/// union; speech adapters use the per-model vendor-declared tables below.
const ALL_LANGUAGES: &[Language] = &[
    Language::Auto,
    Language::English,
    Language::Hindi,
    Language::French,
    Language::Bulgarian,
    Language::Croatian,
    Language::Czech,
    Language::Danish,
    Language::Dutch,
    Language::Estonian,
    Language::Finnish,
    Language::German,
    Language::Greek,
    Language::Hungarian,
    Language::Italian,
    Language::Latvian,
    Language::Lithuanian,
    Language::Maltese,
    Language::Polish,
    Language::Portuguese,
    Language::Romanian,
    Language::Slovak,
    Language::Slovenian,
    Language::Spanish,
    Language::Swedish,
    Language::Russian,
    Language::Ukrainian,
    Language::Arabic,
    Language::Japanese,
    Language::Korean,
    Language::Vietnamese,
    Language::Chinese,
    Language::Turkish,
    Language::Indonesian,
    Language::Malay,
    Language::Thai,
    Language::Persian,
    Language::Macedonian,
    Language::Cantonese,
    Language::Filipino,
    Language::NorwegianBokmal,
];
/// Adapter language sets generated from models/languages.json (see build.rs).
/// The manifest is the source of record; nothing here may drift from it.
use crate::generated::{
    COHERE_LANGUAGES, GRANITE_LANGUAGES, LFM_LANGUAGES, MOONSHINE_LANGUAGES, NEMOTRON_LANGUAGES,
    PARAKEET_LANGUAGES, PHONON2_LANGUAGES, QWEN_ASR_LANGUAGES, REDUX_LANGUAGES, WHISPER_LANGUAGES,
};
const SHERPA: &str = "11afbd009a7f8c08f4bcf2fc1b265d0df4670fbf";
const LLAMA: &str = "d81235049384534c167caea52b85a694f6103d14";
const SPEECH: ModelRole = ModelRole::SpeechRecognition;
const TEXT: ModelRole = ModelRole::TextReconstruction;
pub const MLX_RUNTIME: &str = "MLX-Audio-Swift";
pub const MLX_REVISION: &str = "d302a5c6080d2bb97bae38c7418f82abb76013b6";
const MLX_COMPILED: bool = cfg!(all(
    feature = "mlx",
    target_os = "macos",
    target_arch = "aarch64"
));
/// Backends for models whose only adapter is Vesper's own MLX port. Builds
/// without the MLX bridge offer none, so selecting such a model fails closed.
const MLX_ONLY_BACKENDS: &[ComputeBackend] = if MLX_COMPILED {
    &[ComputeBackend::Auto, ComputeBackend::Metal]
} else {
    &[]
};

/// Models without a CPU export. Automatic resolves to the Apple GPU for every
/// declared language and never falls back to a CPU model.
pub const fn mlx_speech_only(model: AsrModel) -> bool {
    matches!(
        model,
        AsrModel::GraniteSpeech5TurboCtc | AsrModel::Phonon2 | AsrModel::ParakeetRedux
    )
}

/// Models with a pinned NeMo-Speech.cpp GGUF that one file serves on every backend.
pub const fn nemo_gguf_supported(model: AsrModel) -> bool {
    matches!(model, AsrModel::ParakeetTdt06bV3)
}

/// Models with a pinned Core ML encoder for the Apple Neural Engine beside their
/// MLX decoder (#143). Builds without the MLX bridge offer none.
pub const fn neural_engine_supported(model: AsrModel) -> bool {
    MLX_COMPILED && matches!(model, AsrModel::ParakeetTdt06bV3)
}

/// Qualified completed-audio English variants. The models crate resolves saved
/// Automatic preferences against runtime/device availability and installed files.
pub fn mlx_speech_supported(model: AsrModel) -> bool {
    MLX_COMPILED
        && (mlx_speech_only(model)
            || matches!(
                model,
                AsrModel::ParakeetTdt06bV3 | AsrModel::Qwen3Asr06b | AsrModel::WhisperSmall
            ))
}

#[cfg(all(feature = "mlx", target_os = "macos", target_arch = "aarch64"))]
const fn mlx_registration(
    kind: AdapterKind,
    architecture: &'static str,
    state: CacheState,
) -> AdapterRegistration {
    AdapterRegistration {
        kind,
        task: SPEECH,
        architecture,
        runtime: MLX_RUNTIME,
        runtime_revision: MLX_REVISION,
        capabilities: AdapterCapabilities {
            resident_weights: true,
            completion_report: true,
            native_streaming: matches!(kind, AdapterKind::QwenAsr),
            ..caps(state, &[Language::English], &[ComputeBackend::Metal])
        },
    }
}

const fn caps(
    state: CacheState,
    languages: &'static [Language],
    backends: &'static [ComputeBackend],
) -> AdapterCapabilities {
    AdapterCapabilities {
        offline: true,
        native_streaming: false,
        contextual_phrases: false,
        completion_report: false,
        cache_state: state,
        complete_prefix_state: false,
        languages,
        backends,
        resident_weights: false,
    }
}

pub const ADAPTERS: &[AdapterRegistration] = &[
    // Keep the existing CPU registrations first: adapter(kind) is the default runtime.
    AdapterRegistration {
        kind: AdapterKind::NemoRnnt,
        task: SPEECH,
        architecture: "nemo_cache_aware_fastconformer_rnnt",
        runtime: "NeMo-Speech.cpp",
        runtime_revision: "4f9676226f667d14608487df744f375db87127f8",
        capabilities: AdapterCapabilities {
            native_streaming: true,
            contextual_phrases: true,
            resident_weights: true,
            ..caps(CacheState::Rnnt, NEMOTRON_LANGUAGES, ALL_BACKENDS)
        },
    },
    // Parakeet TDT as one GGUF on every backend; completed audio only, since the
    // pinned runtime rejects Parakeet v3 streaming. Live drafts stay buffered.
    AdapterRegistration {
        kind: AdapterKind::NemoTdt,
        task: SPEECH,
        architecture: "nemo_fastconformer_tdt",
        runtime: "NeMo-Speech.cpp",
        runtime_revision: "4f9676226f667d14608487df744f375db87127f8",
        capabilities: AdapterCapabilities {
            resident_weights: true,
            ..caps(CacheState::Rnnt, PARAKEET_LANGUAGES, ALL_BACKENDS)
        },
    },
    AdapterRegistration {
        kind: AdapterKind::SherpaTransducer,
        task: SPEECH,
        architecture: "sherpa_nemo_transducer",
        runtime: "sherpa-onnx",
        runtime_revision: SHERPA,
        capabilities: AdapterCapabilities {
            resident_weights: true,
            ..caps(CacheState::Rnnt, PARAKEET_LANGUAGES, CPU)
        },
    },
    AdapterRegistration {
        kind: AdapterKind::QwenAsr,
        task: SPEECH,
        architecture: "qwen3_asr",
        runtime: "sherpa-onnx",
        runtime_revision: SHERPA,
        capabilities: AdapterCapabilities {
            resident_weights: true,
            completion_report: true,
            ..caps(CacheState::EncoderDecoder, QWEN_ASR_LANGUAGES, CPU)
        },
    },
    AdapterRegistration {
        kind: AdapterKind::CohereAsr,
        task: SPEECH,
        architecture: "cohere_asr",
        runtime: "sherpa-onnx",
        runtime_revision: SHERPA,
        capabilities: AdapterCapabilities {
            resident_weights: true,
            completion_report: true,
            ..caps(CacheState::EncoderDecoder, COHERE_LANGUAGES, CPU)
        },
    },
    AdapterRegistration {
        kind: AdapterKind::Whisper,
        task: SPEECH,
        architecture: "whisper",
        runtime: "sherpa-onnx",
        runtime_revision: SHERPA,
        capabilities: AdapterCapabilities {
            resident_weights: true,
            completion_report: true,
            ..caps(CacheState::EncoderDecoder, WHISPER_LANGUAGES, CPU)
        },
    },
    AdapterRegistration {
        kind: AdapterKind::MoonshineAsr,
        task: SPEECH,
        architecture: "moonshine_v2",
        runtime: "sherpa-onnx",
        runtime_revision: SHERPA,
        capabilities: AdapterCapabilities {
            resident_weights: true,
            completion_report: true,
            ..caps(CacheState::EncoderDecoder, MOONSHINE_LANGUAGES, CPU)
        },
    },
    // Vesper's Swift port of IBM's reference; there is no CPU registration.
    AdapterRegistration {
        kind: AdapterKind::GraniteCtc,
        task: SPEECH,
        architecture: "granite_speech5_ctc",
        runtime: MLX_RUNTIME,
        runtime_revision: MLX_REVISION,
        capabilities: AdapterCapabilities {
            resident_weights: true,
            completion_report: true,
            ..caps(
                CacheState::ModelDefined,
                GRANITE_LANGUAGES,
                MLX_ONLY_BACKENDS,
            )
        },
    },
    // Parakeet TDT with packed five-value encoder layers; Vesper's Swift port.
    AdapterRegistration {
        kind: AdapterKind::PackedTdt,
        task: SPEECH,
        architecture: "phonon2",
        runtime: MLX_RUNTIME,
        runtime_revision: MLX_REVISION,
        capabilities: AdapterCapabilities {
            resident_weights: true,
            completion_report: true,
            ..caps(CacheState::Rnnt, PHONON2_LANGUAGES, MLX_ONLY_BACKENDS)
        },
    },
    // Parakeet TDT with a ternary encoder (Vesper's lossless MLX repacking).
    AdapterRegistration {
        kind: AdapterKind::TernaryTdt,
        task: SPEECH,
        architecture: "parakeet_redux",
        runtime: MLX_RUNTIME,
        runtime_revision: MLX_REVISION,
        capabilities: AdapterCapabilities {
            resident_weights: true,
            completion_report: true,
            ..caps(CacheState::Rnnt, REDUX_LANGUAGES, MLX_ONLY_BACKENDS)
        },
    },
    // Parakeet TDT with the encoder as Core ML on the Neural Engine and the
    // decoder in MLX, through Vesper's MLX bridge. English is the qualified mode.
    AdapterRegistration {
        kind: AdapterKind::NeuralEngineTdt,
        task: SPEECH,
        architecture: "parakeet_neural_engine",
        runtime: MLX_RUNTIME,
        runtime_revision: MLX_REVISION,
        capabilities: AdapterCapabilities {
            resident_weights: true,
            completion_report: true,
            ..caps(CacheState::Rnnt, &[Language::English], MLX_ONLY_BACKENDS)
        },
    },
    AdapterRegistration {
        kind: AdapterKind::LfmAudio,
        task: SPEECH,
        architecture: "lfm2_audio",
        runtime: "llama-cpp-2/mtmd",
        runtime_revision: LLAMA,
        capabilities: AdapterCapabilities {
            resident_weights: true,
            completion_report: true,
            ..caps(CacheState::HybridRecurrentKv, LFM_LANGUAGES, ALL_BACKENDS)
        },
    },
    AdapterRegistration {
        kind: AdapterKind::QwenHybrid,
        task: TEXT,
        architecture: "qwen35_hybrid_causal_lm",
        runtime: "llama-cpp-2",
        runtime_revision: LLAMA,
        capabilities: AdapterCapabilities {
            resident_weights: true,
            complete_prefix_state: true,
            ..caps(CacheState::HybridRecurrentKv, ALL_LANGUAGES, ALL_BACKENDS)
        },
    },
    // Architecture is discovered by pinned llama.cpp from GGUF. Do not assume KV-only state.
    AdapterRegistration {
        kind: AdapterKind::LlamaGguf,
        task: TEXT,
        architecture: "llama_supported_gguf",
        runtime: "llama-cpp-2",
        runtime_revision: LLAMA,
        capabilities: AdapterCapabilities {
            resident_weights: true,
            ..caps(CacheState::ModelDefined, ALL_LANGUAGES, ALL_BACKENDS)
        },
    },
    #[cfg(all(feature = "mlx", target_os = "macos", target_arch = "aarch64"))]
    mlx_registration(
        AdapterKind::SherpaTransducer,
        "sherpa_nemo_transducer",
        CacheState::Rnnt,
    ),
    #[cfg(all(feature = "mlx", target_os = "macos", target_arch = "aarch64"))]
    mlx_registration(
        AdapterKind::QwenAsr,
        "qwen3_asr",
        CacheState::EncoderDecoder,
    ),
    #[cfg(all(feature = "mlx", target_os = "macos", target_arch = "aarch64"))]
    mlx_registration(AdapterKind::Whisper, "whisper", CacheState::EncoderDecoder),
];

pub fn resolve_adapter(
    task: ModelRole,
    architecture: &str,
    runtime: &str,
) -> Option<&'static AdapterRegistration> {
    ADAPTERS
        .iter()
        .find(|a| a.task == task && a.architecture == architecture && a.runtime == runtime)
}

pub fn adapter(kind: AdapterKind) -> &'static AdapterRegistration {
    ADAPTERS
        .iter()
        .find(|a| a.kind == kind)
        .expect("compiled adapter registration")
}

impl AsrModel {
    pub const fn adapter_kind(self) -> AdapterKind {
        match self {
            Self::Nemotron35AsrStreaming06b => AdapterKind::NemoRnnt,
            Self::ParakeetTdt06bV3 => AdapterKind::SherpaTransducer,
            Self::Qwen3Asr06b => AdapterKind::QwenAsr,
            Self::CohereTranscribe => AdapterKind::CohereAsr,
            Self::WhisperSmall | Self::WhisperTurbo => AdapterKind::Whisper,
            Self::Lfm25Audio15b => AdapterKind::LfmAudio,
            Self::MoonshineBase => AdapterKind::MoonshineAsr,
            Self::GraniteSpeech5TurboCtc => AdapterKind::GraniteCtc,
            Self::Phonon2 => AdapterKind::PackedTdt,
            Self::ParakeetRedux => AdapterKind::TernaryTdt,
        }
    }
}

impl AppConfig {
    /// The selected speech model runs as one GGUF through NeMo-Speech.cpp.
    pub fn uses_nemo_gguf(&self) -> bool {
        self.models.custom_asr.is_none()
            && self.models.asr_engine == AsrEngine::UnifiedGguf
            && nemo_gguf_supported(self.models.asr)
    }
    /// The selected speech model runs its encoder on the Apple Neural Engine.
    pub fn uses_neural_engine(&self) -> bool {
        self.models.custom_asr.is_none()
            && self.models.asr_engine == AsrEngine::NeuralEngine
            && neural_engine_supported(self.models.asr)
    }
    /// The speech model loads through Vesper's MLX bridge: MLX speech, or the
    /// Neural Engine's Core ML encoder with its MLX decoder.
    pub fn uses_mlx_runtime(&self) -> bool {
        self.uses_mlx_speech() || self.uses_neural_engine()
    }
    pub fn uses_mlx_speech(&self) -> bool {
        self.models.custom_asr.is_none()
            && !self.uses_nemo_gguf()
            && !self.uses_neural_engine()
            && mlx_speech_supported(self.models.asr)
            && (self.compute_backend == ComputeBackend::Metal
                || (self.compute_backend == ComputeBackend::Auto
                    && mlx_speech_only(self.models.asr)))
    }
    pub fn speech_adapter(&self) -> &'static AdapterRegistration {
        if self.uses_nemo_gguf() {
            return adapter(AdapterKind::NemoTdt);
        }
        if self.uses_neural_engine() {
            return adapter(AdapterKind::NeuralEngineTdt);
        }
        if self.uses_mlx_speech() {
            return resolve_adapter(
                SPEECH,
                adapter(self.models.asr.adapter_kind()).architecture,
                MLX_RUNTIME,
            )
            .expect("compiled MLX architecture registration");
        }
        adapter(if self.models.custom_asr.is_some() {
            AdapterKind::SherpaTransducer
        } else {
            self.models.asr.adapter_kind()
        })
    }
    pub fn text_adapter(&self) -> &'static AdapterRegistration {
        adapter(if self.models.custom_rewriter.is_some() {
            AdapterKind::LlamaGguf
        } else {
            AdapterKind::QwenHybrid
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_builtin_asr_has_live_delivery_without_changing_language_contracts() {
        for model in AsrModel::ALL {
            let mut config = AppConfig::default();
            config.models.asr = model;
            config.compute_backend = ComputeBackend::Cpu;
            assert_ne!(config.speech_adapter().live_mode(), LiveMode::Unavailable);
            assert_eq!(config.text_adapter().live_mode(), LiveMode::Unavailable);
            assert_eq!(
                config.speech_adapter().live_mode() == LiveMode::Native,
                model == AsrModel::Nemotron35AsrStreaming06b
            );
        }
    }
    #[test]
    fn whisper_variants_share_one_cpu_completed_audio_contract() {
        for model in [AsrModel::WhisperSmall, AsrModel::WhisperTurbo] {
            let mut config = AppConfig::default();
            config.models.asr = model;
            assert_eq!(config.speech_adapter().kind, AdapterKind::Whisper);
            let capability = config.speech_adapter().capabilities;
            assert!(
                capability.offline && capability.resident_weights && capability.completion_report
            );
            assert!(!capability.native_streaming && !capability.contextual_phrases);
            for language in ALL_LANGUAGES {
                config.language = *language;
                assert_eq!(
                    config.validate().is_ok(),
                    WHISPER_LANGUAGES.contains(language)
                );
            }
            config.language = Language::English;
            for backend in ALL_BACKENDS {
                config.compute_backend = *backend;
                assert_eq!(
                    config.validate().is_ok(),
                    CPU.contains(backend)
                        || (mlx_speech_supported(model) && *backend == ComputeBackend::Metal)
                );
            }
        }
        assert_eq!(AppConfig::default().models.asr, AsrModel::CohereTranscribe);
    }
    #[test]
    fn lfm_is_resident_completed_audio_with_explicit_english_and_native_backends() {
        let mut config = AppConfig::default();
        config.models.asr = AsrModel::Lfm25Audio15b;
        let capability = config.speech_adapter().capabilities;
        assert!(capability.offline && capability.resident_weights && capability.completion_report);
        assert!(
            !capability.native_streaming
                && !capability.contextual_phrases
                && !capability.complete_prefix_state
        );
        for language in ALL_LANGUAGES {
            config.language = *language;
            assert_eq!(config.validate().is_ok(), *language == Language::English);
        }
        config.language = Language::English;
        for backend in ALL_BACKENDS {
            config.compute_backend = *backend;
            assert!(config.validate().is_ok());
        }
        config.compute_backend = ComputeBackend::Cpu;
        let restored: AppConfig =
            serde_json::from_str(&serde_json::to_string(&config).unwrap()).unwrap();
        assert_eq!(restored, config);
    }

    #[test]
    fn manifest_wiring_matches_adapter_capabilities() {
        // models/languages.json is the source of record and build.rs generates
        // the capability tables from it; this test guards the remaining hand
        // wiring, i.e. each model resolves to the adapter carrying its own set.
        let manifest: serde_json::Value =
            serde_json::from_str(include_str!("../../../models/languages.json")).unwrap();
        let table = manifest.as_object().unwrap();
        for model in AsrModel::ALL {
            let entry = &table[model.id()];
            let mut declared: std::collections::BTreeSet<&str> = entry["languages"]
                .as_array()
                .unwrap_or_else(|| panic!("manifest lacks languages for {}", model.id()))
                .iter()
                .map(|v| v.as_str().expect("manifest language codes are strings"))
                .collect();
            let auto = entry["autoDetect"]
                .as_bool()
                .unwrap_or_else(|| panic!("manifest lacks autoDetect for {}", model.id()));
            if auto {
                declared.insert("auto");
            }
            let kind = match model {
                AsrModel::ParakeetTdt06bV3 => AdapterKind::SherpaTransducer,
                AsrModel::Nemotron35AsrStreaming06b => AdapterKind::NemoRnnt,
                AsrModel::Qwen3Asr06b => AdapterKind::QwenAsr,
                AsrModel::CohereTranscribe => AdapterKind::CohereAsr,
                AsrModel::WhisperSmall | AsrModel::WhisperTurbo => AdapterKind::Whisper,
                AsrModel::Lfm25Audio15b => AdapterKind::LfmAudio,
                AsrModel::MoonshineBase => AdapterKind::MoonshineAsr,
                AsrModel::GraniteSpeech5TurboCtc => AdapterKind::GraniteCtc,
                AsrModel::Phonon2 => AdapterKind::PackedTdt,
                AsrModel::ParakeetRedux => AdapterKind::TernaryTdt,
            };
            let offered: std::collections::BTreeSet<&str> = adapter(kind)
                .capabilities
                .languages
                .iter()
                .map(|l| l.code().unwrap_or("auto"))
                .collect();
            assert_eq!(offered, declared, "{}", model.id());
        }
    }

    #[test]
    fn every_manifest_language_validates_per_model() {
        // Admission follows the manifest for every model and language;
        // extending either extends this coverage with no test edit.
        let manifest: serde_json::Value =
            serde_json::from_str(include_str!("../../../models/languages.json")).unwrap();
        let table = manifest.as_object().unwrap();
        for model in AsrModel::ALL {
            let entry = &table[model.id()];
            let mut codes: Vec<Language> = entry["languages"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| {
                    serde_json::from_value::<Language>(v.clone()).expect("manifest codes resolve")
                })
                .collect();
            if entry["autoDetect"].as_bool().unwrap() {
                codes.push(Language::Auto);
            }
            for language in &codes {
                let mut config = AppConfig::default();
                config.models.asr = model;
                config.language = *language;
                // A GPU-only model is admitted only by builds that carry its runtime.
                assert_eq!(
                    config.validate().is_ok(),
                    !mlx_speech_only(model) || MLX_COMPILED,
                    "{:?} {language:?}",
                    model
                );
            }
            // One undeclared mode still fails instead of falling back.
            if let Some(outside) = ALL_LANGUAGES.iter().find(|l| !codes.contains(l)) {
                let mut config = AppConfig::default();
                config.models.asr = model;
                config.language = *outside;
                assert!(config.validate().is_err(), "{:?} {outside:?}", model);
            }
        }
    }

    #[test]
    fn apple_gpu_only_models_never_resolve_to_a_cpu_adapter() {
        let mut config = AppConfig::default();
        config.models.asr = AsrModel::GraniteSpeech5TurboCtc;
        assert!(mlx_speech_only(config.models.asr));
        assert_eq!(config.speech_adapter().kind, AdapterKind::GraniteCtc);
        assert_eq!(config.speech_adapter().runtime, MLX_RUNTIME);
        for language in [Language::English, Language::German, Language::Japanese] {
            config.language = language;
            for backend in ALL_BACKENDS {
                config.compute_backend = *backend;
                let gpu = matches!(backend, ComputeBackend::Auto | ComputeBackend::Metal);
                assert_eq!(
                    config.validate().is_ok(),
                    MLX_COMPILED && gpu,
                    "{backend:?}"
                );
                assert_eq!(config.uses_mlx_speech(), MLX_COMPILED && gpu, "{backend:?}");
            }
        }
        config.compute_backend = ComputeBackend::Auto;
        config.language = Language::Hindi;
        assert!(config.validate().is_err());
    }

    #[test]
    fn generated_runtime_codes_match_the_manifest() {
        let manifest: serde_json::Value =
            serde_json::from_str(include_str!("../../../models/languages.json")).unwrap();
        let table = manifest.as_object().unwrap();
        let mut count = 0;
        for model in AsrModel::ALL {
            let entry = &table[model.id()];
            if let Some(runtimes) = entry.get("runtimeCodes").and_then(|v| v.as_object()) {
                for (runtime, map) in runtimes {
                    for (iso, expected) in map.as_object().unwrap() {
                        assert_eq!(
                            crate::generated::runtime_language_code(runtime, model.id(), iso),
                            Some(expected.as_str().unwrap()),
                            "{runtime} {} {iso}",
                            model.id()
                        );
                        count += 1;
                    }
                }
            }
        }
        assert!(count > 0, "manifest runtimeCodes must not be empty");
    }

    #[test]
    fn unique_keys_and_exact_task_resolution() {
        for a in ADAPTERS {
            assert_eq!(
                ADAPTERS
                    .iter()
                    .filter(|b| b.task == a.task
                        && b.architecture == a.architecture
                        && b.runtime == a.runtime)
                    .count(),
                1
            );
            assert_eq!(
                resolve_adapter(a.task, a.architecture, a.runtime)
                    .unwrap()
                    .kind,
                a.kind
            );
            assert!(resolve_adapter(a.task, a.architecture, "unregistered-runtime").is_none());
        }
        assert!(
            !adapter(AdapterKind::LlamaGguf)
                .capabilities
                .complete_prefix_state
        );
        assert!(
            adapter(AdapterKind::NemoRnnt)
                .capabilities
                .contextual_phrases
        );
        let default = AppConfig::default();
        default.validate().unwrap();
        assert_eq!(default.speech_adapter().kind, AdapterKind::CohereAsr);
        assert_eq!(default.text_adapter().kind, AdapterKind::QwenHybrid);
        assert_eq!(default.language, Language::English);
        assert!(default.cleanup_enabled);
        assert!(!default.speech_adapter().capabilities.native_streaming);
    }

    #[test]
    fn unified_gguf_runs_parakeet_on_every_backend_and_language() {
        let mut config = AppConfig::default();
        config.models.asr = AsrModel::ParakeetTdt06bV3;
        config.models.asr_engine = AsrEngine::UnifiedGguf;
        for backend in [
            ComputeBackend::Auto,
            ComputeBackend::Cpu,
            ComputeBackend::Metal,
            ComputeBackend::Cuda,
            ComputeBackend::Vulkan,
        ] {
            config.compute_backend = backend;
            for language in [Language::Auto, Language::English, Language::German] {
                config.language = language;
                assert!(config.uses_nemo_gguf());
                assert!(!config.uses_mlx_speech(), "{backend:?}");
                assert_eq!(config.speech_adapter().kind, AdapterKind::NemoTdt);
                config
                    .validate()
                    .unwrap_or_else(|e| panic!("{backend:?} {language:?}: {e}"));
            }
        }
        // Same declared languages as the standard engine; live drafts stay buffered.
        assert_eq!(
            adapter(AdapterKind::NemoTdt).capabilities.languages,
            adapter(AdapterKind::SherpaTransducer)
                .capabilities
                .languages
        );
        assert_eq!(
            adapter(AdapterKind::NemoTdt).live_mode(),
            LiveMode::Buffered
        );
    }

    #[test]
    fn unified_gguf_is_rejected_for_models_without_a_pinned_gguf() {
        let mut config = AppConfig::default();
        config.models.asr = AsrModel::CohereTranscribe;
        config.models.asr_engine = AsrEngine::UnifiedGguf;
        assert!(!config.uses_nemo_gguf());
        let error = config.validate().unwrap_err().to_string();
        assert!(error.contains("models.asrEngine"), "{error}");
    }

    #[test]
    fn neural_engine_runs_parakeet_in_english_on_apple_silicon_mlx_builds() {
        let mut config = AppConfig::default();
        config.models.asr = AsrModel::ParakeetTdt06bV3;
        config.models.asr_engine = AsrEngine::NeuralEngine;
        config.language = Language::English;
        assert_eq!(config.uses_neural_engine(), MLX_COMPILED);
        assert!(!config.uses_mlx_speech() && !config.uses_nemo_gguf());
        for backend in [ComputeBackend::Auto, ComputeBackend::Metal] {
            config.compute_backend = backend;
            assert_eq!(config.validate().is_ok(), MLX_COMPILED, "{backend:?}");
        }
        if MLX_COMPILED {
            assert_eq!(config.speech_adapter().kind, AdapterKind::NeuralEngineTdt);
            assert_eq!(
                adapter(AdapterKind::NeuralEngineTdt).live_mode(),
                LiveMode::Buffered
            );
        } else {
            let error = config.validate().unwrap_err().to_string();
            assert!(error.contains("models.asrEngine"), "{error}");
        }
        config.compute_backend = ComputeBackend::Cpu;
        assert!(config.validate().is_err());
        config.compute_backend = ComputeBackend::Auto;
        config.language = Language::German;
        assert!(config.validate().is_err());
        config.language = Language::English;
        config.models.asr = AsrModel::Qwen3Asr06b;
        assert!(!config.uses_neural_engine());
        let error = config.validate().unwrap_err().to_string();
        assert!(error.contains("models.asrEngine"), "{error}");
    }

    #[test]
    fn standard_engine_keeps_parakeet_on_its_existing_adapters() {
        let mut config = AppConfig::default();
        config.models.asr = AsrModel::ParakeetTdt06bV3;
        config.compute_backend = ComputeBackend::Cpu;
        config.language = Language::German;
        assert!(!config.uses_nemo_gguf());
        assert_eq!(config.speech_adapter().kind, AdapterKind::SherpaTransducer);
        config.compute_backend = ComputeBackend::Cuda;
        assert!(config.validate().is_err());
    }
}
