use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AsrModel {
    #[serde(rename = "nvidia/parakeet-tdt-0.6b-v3")]
    ParakeetTdt06bV3,
    #[serde(rename = "nvidia/nemotron-3.5-asr-streaming-0.6b")]
    Nemotron35AsrStreaming06b,
    #[serde(rename = "Qwen/Qwen3-ASR-0.6B")]
    Qwen3Asr06b,
    #[default]
    #[serde(rename = "CohereLabs/cohere-transcribe-03-2026")]
    CohereTranscribe,
    #[serde(rename = "LiquidAI/LFM2.5-Audio-1.5B")]
    Lfm25Audio15b,
    #[serde(rename = "openai/whisper-small")]
    WhisperSmall,
    #[serde(rename = "openai/whisper-large-v3-turbo")]
    WhisperTurbo,
}

impl AsrModel {
    pub const ALL: [Self; 7] = [
        Self::ParakeetTdt06bV3,
        Self::Nemotron35AsrStreaming06b,
        Self::Qwen3Asr06b,
        Self::CohereTranscribe,
        Self::Lfm25Audio15b,
        Self::WhisperSmall,
        Self::WhisperTurbo,
    ];

    pub const fn id(self) -> &'static str {
        match self {
            Self::ParakeetTdt06bV3 => "nvidia/parakeet-tdt-0.6b-v3",
            Self::Nemotron35AsrStreaming06b => "nvidia/nemotron-3.5-asr-streaming-0.6b",
            Self::Qwen3Asr06b => "Qwen/Qwen3-ASR-0.6B",
            Self::CohereTranscribe => "CohereLabs/cohere-transcribe-03-2026",
            Self::Lfm25Audio15b => "LiquidAI/LFM2.5-Audio-1.5B",
            Self::WhisperSmall => "openai/whisper-small",
            Self::WhisperTurbo => "openai/whisper-large-v3-turbo",
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::ParakeetTdt06bV3 => "NVIDIA Parakeet TDT 0.6B v3",
            Self::Nemotron35AsrStreaming06b => "NVIDIA Nemotron 3.5 ASR Streaming 0.6B",
            Self::Qwen3Asr06b => "Qwen3-ASR 0.6B",
            Self::CohereTranscribe => "Cohere Transcribe",
            Self::Lfm25Audio15b => "Liquid LFM2.5 Audio 1.5B",
            Self::WhisperSmall => "OpenAI Whisper Small",
            Self::WhisperTurbo => "OpenAI Whisper Turbo",
        }
    }

    pub const fn requires_completion_report(self) -> bool {
        matches!(
            self,
            Self::Qwen3Asr06b | Self::CohereTranscribe | Self::WhisperSmall | Self::WhisperTurbo
        )
    }

    pub const fn is_whisper(self) -> bool {
        matches!(self, Self::WhisperSmall | Self::WhisperTurbo)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum RewriteModel {
    #[default]
    #[serde(rename = "Qwen/Qwen3.5-0.8B")]
    Qwen35_08b,
}

impl RewriteModel {
    pub const ALL: [Self; 1] = [Self::Qwen35_08b];

    pub const fn id(self) -> &'static str {
        match self {
            Self::Qwen35_08b => "Qwen/Qwen3.5-0.8B",
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Qwen35_08b => "Qwen3.5 0.8B",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelRole {
    AudioFrontend,
    SpeechRecognition,
    TextReconstruction,
}

/// Informational catalog entry, not runtime installation or readiness state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelSpec {
    pub id: String,
    pub name: String,
    pub role: ModelRole,
    pub languages: String,
    pub description: String,
}

pub fn default_model_catalog() -> Vec<ModelSpec> {
    vec![
        ModelSpec {
            id: AsrModel::ParakeetTdt06bV3.id().into(),
            name: AsrModel::ParakeetTdt06bV3.name().into(),
            role: ModelRole::SpeechRecognition,
            languages: "25 European languages, including English; Hindi is not supported.".into(),
            description: "Optional offline compatibility model for European languages.".into(),
        },
        ModelSpec {
            id: AsrModel::Nemotron35AsrStreaming06b.id().into(),
            name: AsrModel::Nemotron35AsrStreaming06b.name().into(),
            role: ModelRole::SpeechRecognition,
            languages: "32 locales work out of the box, including English and Hindi; eight additional locales require adaptation.".into(),
            description: "Default multilingual, cache-aware streaming model; mixed-language accuracy needs evaluation.".into(),
        },
        ModelSpec {
            id: AsrModel::Qwen3Asr06b.id().into(),
            name: AsrModel::Qwen3Asr06b.name().into(),
            role: ModelRole::SpeechRecognition,
            languages: "Automatic, English, French and Hindi in this app.".into(),
            description: "Local CPU transcription with automatic processing of longer recordings.".into(),
        },
        ModelSpec {
            id: AsrModel::CohereTranscribe.id().into(),
            name: AsrModel::CohereTranscribe.name().into(),
            role: ModelRole::SpeechRecognition,
            languages: "Select English or French; automatic detection and Hindi are unavailable.".into(),
            description: "Local CPU transcription with automatic processing of longer recordings.".into(),
        },
        ModelSpec {
            id: AsrModel::Lfm25Audio15b.id().into(),
            name: AsrModel::Lfm25Audio15b.name().into(),
            role: ModelRole::SpeechRecognition,
            languages: "English only; select English explicitly.".into(),
            description: "Local CPU transcription of completed recordings using resident Q4 weights. Longer audio is processed in segments.".into(),
        },
        ModelSpec {
            id: AsrModel::WhisperSmall.id().into(),
            name: AsrModel::WhisperSmall.name().into(),
            role: ModelRole::SpeechRecognition,
            languages: "English and French, explicit or automatic. Hindi is not qualified.".into(),
            description: "INT8 CPU transcription of completed recordings; longer audio is processed in segments.".into(),
        },
        ModelSpec {
            id: AsrModel::WhisperTurbo.id().into(),
            name: AsrModel::WhisperTurbo.name().into(),
            role: ModelRole::SpeechRecognition,
            languages: "English and French, explicit or automatic. Hindi is not qualified.".into(),
            description: "INT8 CPU transcription of completed recordings with a larger encoder and four decoder layers.".into(),
        },
        ModelSpec {
            id: RewriteModel::Qwen35_08b.id().into(),
            name: RewriteModel::Qwen35_08b.name().into(),
            role: ModelRole::TextReconstruction,
            languages: "Multilingual; evaluate writing quality for the product's target languages.".into(),
            description: "Small writing model candidate with thinking disabled; preserve meaning and the original transcript.".into(),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AppConfig;

    #[test]
    fn default_selection_resolves_to_unique_catalog_entries_with_correct_roles() {
        let catalog = default_model_catalog();
        let config = AppConfig::default();
        for (id, role) in [
            (config.models.asr.id(), ModelRole::SpeechRecognition),
            (config.models.rewriter.id(), ModelRole::TextReconstruction),
        ] {
            let entries: Vec<_> = catalog.iter().filter(|model| model.id == id).collect();
            assert_eq!(entries.len(), 1, "model {id} must resolve unambiguously");
            assert_eq!(entries[0].role, role);
        }
        assert_eq!(
            catalog
                .iter()
                .map(|model| &model.id)
                .collect::<std::collections::HashSet<_>>()
                .len(),
            catalog.len(),
            "catalog IDs must be unique"
        );
    }

    #[test]
    fn installable_model_lists_are_fully_represented_in_the_catalog() {
        let catalog = default_model_catalog();
        for model in AsrModel::ALL {
            assert_eq!(
                catalog
                    .iter()
                    .filter(|entry| {
                        entry.id == model.id() && entry.role == ModelRole::SpeechRecognition
                    })
                    .count(),
                1
            );
        }
        for model in RewriteModel::ALL {
            assert_eq!(
                catalog
                    .iter()
                    .filter(|entry| {
                        entry.id == model.id() && entry.role == ModelRole::TextReconstruction
                    })
                    .count(),
                1
            );
        }
    }

    #[test]
    fn removed_models_are_not_silently_migrated() {
        assert!(serde_json::from_str::<AsrModel>(r#""kyutai/stt-1b-en_fr-candle""#).is_err());
    }
}
