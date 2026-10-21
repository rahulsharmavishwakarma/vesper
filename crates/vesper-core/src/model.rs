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
    #[serde(rename = "moonshine-ai/moonshine-base")]
    MoonshineBase,
}

impl AsrModel {
    pub const ALL: [Self; 8] = [
        Self::ParakeetTdt06bV3,
        Self::Nemotron35AsrStreaming06b,
        Self::Qwen3Asr06b,
        Self::CohereTranscribe,
        Self::Lfm25Audio15b,
        Self::WhisperSmall,
        Self::WhisperTurbo,
        Self::MoonshineBase,
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
            Self::MoonshineBase => "moonshine-ai/moonshine-base",
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
            Self::MoonshineBase => "Moonshine Base (English)",
        }
    }

    pub const fn requires_completion_report(self) -> bool {
        matches!(
            self,
            Self::Qwen3Asr06b
                | Self::CohereTranscribe
                | Self::WhisperSmall
                | Self::WhisperTurbo
                | Self::MoonshineBase
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

/// Display catalog built from models/catalog.json with language counts from
/// models/languages.json. Speech entries come first in catalog order, then
/// writers. Display stays data-driven; the enums and dispatch arms stay code.
fn catalog_specs() -> Vec<ModelSpec> {
    let catalog: serde_json::Value =
        serde_json::from_str(include_str!("../../../models/catalog.json"))
            .expect("models/catalog.json must be valid");
    let manifest: serde_json::Value =
        serde_json::from_str(include_str!("../../../models/languages.json"))
            .expect("models/languages.json must be valid");
    let mut specs = Vec::new();
    for role in ["speech_recognition", "text_reconstruction"] {
        for entry in catalog["models"]
            .as_array()
            .expect("catalog needs a models array")
        {
            if entry["role"] != role {
                continue;
            }
            let id = entry["id"]
                .as_str()
                .expect("catalog entries need string ids");
            let kind = match role {
                "speech_recognition" => ModelRole::SpeechRecognition,
                _ => ModelRole::TextReconstruction,
            };
            let languages = if role == "speech_recognition" {
                let declared = &manifest[id]["languages"];
                let count = declared
                    .as_array()
                    .unwrap_or_else(|| panic!("languages.json lacks '{id}'"))
                    .len();
                let auto = manifest[id]["autoDetect"].as_bool().unwrap_or(false);
                if count == 1 {
                    "1 vendor-declared language, explicit selection only.".to_string()
                } else if auto {
                    format!("{count} vendor-declared languages with automatic detection.")
                } else {
                    format!("{count} vendor-declared languages, explicit selection only.")
                }
            } else {
                "Multilingual; evaluate writing quality for the product's target languages."
                    .to_string()
            };
            specs.push(ModelSpec {
                id: id.into(),
                name: entry["name"]
                    .as_str()
                    .expect("catalog entries need names")
                    .into(),
                role: kind,
                languages,
                description: entry["description"]
                    .as_str()
                    .expect("catalog entries need descriptions")
                    .into(),
            });
        }
    }
    specs
}

static CATALOG_SPECS: std::sync::OnceLock<Vec<ModelSpec>> = std::sync::OnceLock::new();

pub fn default_model_catalog() -> Vec<ModelSpec> {
    CATALOG_SPECS.get_or_init(catalog_specs).clone()
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

    #[test]
    fn catalog_display_matches_manifest_counts_and_names() {
        // models/catalog.json is the display source; languages.json owns the
        // counts. Extending either extends this coverage with no test edit.
        let catalog: serde_json::Value =
            serde_json::from_str(include_str!("../../../models/catalog.json")).unwrap();
        let manifest: serde_json::Value =
            serde_json::from_str(include_str!("../../../models/languages.json")).unwrap();
        let specs = default_model_catalog();
        assert_eq!(specs.len(), catalog["models"].as_array().unwrap().len());
        for entry in catalog["models"].as_array().unwrap() {
            let id = entry["id"].as_str().unwrap();
            let spec = specs
                .iter()
                .find(|s| s.id == id)
                .unwrap_or_else(|| panic!("catalog model {id} missing from default_model_catalog"));
            assert_eq!(spec.name, entry["name"].as_str().unwrap());
            assert_eq!(spec.description, entry["description"].as_str().unwrap());
            if spec.role == ModelRole::SpeechRecognition {
                let count = manifest[id]["languages"].as_array().unwrap().len();
                assert!(
                    spec.languages.starts_with(&count.to_string()),
                    "{id} clause must lead with its manifest count"
                );
            }
        }
    }
}
