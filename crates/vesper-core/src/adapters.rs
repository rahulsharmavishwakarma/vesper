//! Compiled architecture contracts. Model repositories supply data, never code.
//! Resolve at request/load boundaries; native runtimes own all frame/token kernels.
use crate::{AppConfig, AsrModel, ComputeBackend, Language, ModelRole};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AdapterKind {
    NemoRnnt,
    SherpaTransducer,
    QwenAsr,
    CohereAsr,
    Whisper,
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
const EN_FR: &[Language] = &[Language::Auto, Language::English, Language::French];
const EXPLICIT_EN_FR: &[Language] = &[Language::English, Language::French];
/// Parakeet TDT v3: 25 vendor-declared ASR languages (NeMo sherpa path
/// auto-detects; the list gates what the app offers and validates).
const PARAKEET_LANGUAGES: &[Language] = &[
    Language::Auto,
    Language::English,
    Language::Bulgarian,
    Language::Croatian,
    Language::Czech,
    Language::Danish,
    Language::Dutch,
    Language::Estonian,
    Language::Finnish,
    Language::French,
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
];
/// Nemotron 3.5 streaming: 32 locales that transcribe out of the box
/// (8 adaptation-ready locales need fine-tuning and stay excluded).
const NEMOTRON_LANGUAGES: &[Language] = &[
    Language::Auto,
    Language::English,
    Language::Spanish,
    Language::French,
    Language::Italian,
    Language::Portuguese,
    Language::Dutch,
    Language::German,
    Language::Turkish,
    Language::Russian,
    Language::Arabic,
    Language::Hindi,
    Language::Japanese,
    Language::Korean,
    Language::Vietnamese,
    Language::Ukrainian,
    Language::Polish,
    Language::Swedish,
    Language::Czech,
    Language::NorwegianBokmal,
    Language::Danish,
    Language::Bulgarian,
    Language::Finnish,
    Language::Croatian,
    Language::Slovak,
    Language::Chinese,
    Language::Hungarian,
    Language::Romanian,
    Language::Estonian,
];
/// Qwen3-ASR: 30 vendor-declared recognition languages (dialects excluded).
const QWEN_ASR_LANGUAGES: &[Language] = &[
    Language::Auto,
    Language::Chinese,
    Language::English,
    Language::Cantonese,
    Language::Arabic,
    Language::German,
    Language::French,
    Language::Spanish,
    Language::Portuguese,
    Language::Indonesian,
    Language::Italian,
    Language::Korean,
    Language::Russian,
    Language::Thai,
    Language::Vietnamese,
    Language::Japanese,
    Language::Turkish,
    Language::Hindi,
    Language::Malay,
    Language::Dutch,
    Language::Swedish,
    Language::Danish,
    Language::Finnish,
    Language::Polish,
    Language::Czech,
    Language::Filipino,
    Language::Persian,
    Language::Greek,
    Language::Hungarian,
    Language::Macedonian,
    Language::Romanian,
];
/// Cohere Transcribe: 14 vendor-declared languages, no auto-detect, no Hindi.
/// ISO codes are standard mappings of the vendor's language names.
const COHERE_LANGUAGES: &[Language] = &[
    Language::English,
    Language::French,
    Language::German,
    Language::Italian,
    Language::Spanish,
    Language::Portuguese,
    Language::Greek,
    Language::Dutch,
    Language::Polish,
    Language::Chinese,
    Language::Japanese,
    Language::Korean,
    Language::Vietnamese,
    Language::Arabic,
];
const SHERPA: &str = "11afbd009a7f8c08f4bcf2fc1b265d0df4670fbf";
const LLAMA: &str = "e79e4bf660e19f2ad851e06c6913f7a8c5852621";
const SPEECH: ModelRole = ModelRole::SpeechRecognition;
const TEXT: ModelRole = ModelRole::TextReconstruction;
pub const MLX_RUNTIME: &str = "MLX-Audio-Swift";
pub const MLX_REVISION: &str = "d302a5c6080d2bb97bae38c7418f82abb76013b6";

/// Qualified completed-audio English variants. The models crate resolves saved
/// Automatic preferences against runtime/device availability and installed files.
pub fn mlx_speech_supported(model: AsrModel) -> bool {
    cfg!(all(
        feature = "mlx",
        target_os = "macos",
        target_arch = "aarch64"
    )) && matches!(
        model,
        AsrModel::ParakeetTdt06bV3 | AsrModel::Qwen3Asr06b | AsrModel::WhisperSmall
    )
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
            ..caps(CacheState::EncoderDecoder, EN_FR, CPU)
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
            ..caps(
                CacheState::HybridRecurrentKv,
                &[Language::English],
                ALL_BACKENDS,
            )
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
        }
    }
}

impl AppConfig {
    pub fn uses_mlx_speech(&self) -> bool {
        self.models.custom_asr.is_none()
            && self.compute_backend == ComputeBackend::Metal
            && mlx_speech_supported(self.models.asr)
    }
    pub fn speech_adapter(&self) -> &'static AdapterRegistration {
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
                assert_eq!(config.validate().is_ok(), EN_FR.contains(language));
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
    fn language_manifest_matches_adapter_capability_tables() {
        // models/languages.json is the human-edited source of record; the
        // const tables above must mirror it exactly (Auto <-> autoDetect).
        let manifest: serde_json::Value =
            serde_json::from_str(include_str!("../../../models/languages.json")).unwrap();
        let table = manifest.as_object().unwrap();
        for model in AsrModel::ALL {
            let entry = &table[model.id()];
            let declared: std::collections::BTreeSet<&str> = entry["languages"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap())
                .collect();
            let auto = entry["autoDetect"].as_bool().unwrap();
            let (kind, expected): (AdapterKind, Vec<&str>) = match model {
                AsrModel::ParakeetTdt06bV3 => (
                    AdapterKind::SherpaTransducer,
                    PARAKEET_LANGUAGES
                        .iter()
                        .map(|l| l.code().unwrap_or("auto"))
                        .collect(),
                ),
                AsrModel::Nemotron35AsrStreaming06b => (
                    AdapterKind::NemoRnnt,
                    NEMOTRON_LANGUAGES
                        .iter()
                        .map(|l| l.code().unwrap_or("auto"))
                        .collect(),
                ),
                AsrModel::Qwen3Asr06b => (
                    AdapterKind::QwenAsr,
                    QWEN_ASR_LANGUAGES
                        .iter()
                        .map(|l| l.code().unwrap_or("auto"))
                        .collect(),
                ),
                AsrModel::CohereTranscribe => (
                    AdapterKind::CohereAsr,
                    COHERE_LANGUAGES
                        .iter()
                        .map(|l| l.code().unwrap_or("auto"))
                        .collect(),
                ),
                AsrModel::WhisperSmall | AsrModel::WhisperTurbo => (
                    AdapterKind::Whisper,
                    EN_FR.iter().map(|l| l.code().unwrap_or("auto")).collect(),
                ),
                AsrModel::Lfm25Audio15b => (AdapterKind::LfmAudio, vec!["en"]),
            };
            let expected_set: std::collections::BTreeSet<&str> = expected.into_iter().collect();
            let mut manifest_set = declared;
            if auto {
                manifest_set.insert("auto");
            }
            assert_eq!(manifest_set, expected_set, "{}", model.id());
            assert_eq!(
                adapter(kind).capabilities.languages.len(),
                expected_set.len(),
                "{}",
                model.id()
            );
        }
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
}
