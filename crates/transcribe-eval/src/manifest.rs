use serde::{Deserialize, Serialize};
use std::{collections::HashSet, path::Path};

pub const MANIFEST_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BenchmarkManifest {
    pub schema_version: u32,
    pub sources: Vec<CorpusSource>,
    pub samples: Vec<BenchmarkSample>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CorpusSource {
    pub id: String,
    pub name: String,
    pub url: String,
    pub revision: String,
    pub license: String,
    pub attribution: String,
    pub redistribution: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BenchmarkSample {
    pub id: String,
    pub source_id: String,
    pub split: String,
    pub language: String,
    pub audio_path: String,
    pub sha256: String,
    pub reference: String,
    pub duration_seconds: f64,
    pub scenario: Scenario,
    pub replay_kind: ReplayKind,
    #[serde(default)]
    pub speaker_id: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Scenario {
    Read,
    Conversational,
    MixedSpeech,
    TechnicalLecture,
    MeetingCloseTalk,
    MeetingFarField,
    Singing,
    Noise,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReplayKind {
    Natural,
    SyntheticSoak,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BenchmarkEnvironment {
    pub label: String,
    pub hardware: String,
    pub operating_system: String,
    pub power_mode: String,
    pub backend: String,
    pub precision: String,
    pub threads: u32,
    pub application_revision: String,
    pub runtime_revision: String,
    pub driver: String,
    pub notes: String,
}

impl BenchmarkManifest {
    pub fn from_path(path: &Path) -> Result<Self, String> {
        let bytes = std::fs::read(path)
            .map_err(|error| format!("cannot read manifest {}: {error}", path.display()))?;
        let manifest: Self = serde_json::from_slice(&bytes)
            .map_err(|error| format!("cannot decode manifest {}: {error}", path.display()))?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != MANIFEST_SCHEMA_VERSION {
            return Err(format!(
                "unsupported benchmark manifest schema {}; expected {MANIFEST_SCHEMA_VERSION}",
                self.schema_version
            ));
        }
        if self.sources.is_empty() || self.samples.is_empty() {
            return Err("benchmark manifests need at least one source and sample".into());
        }
        let mut source_ids = HashSet::new();
        for source in &self.sources {
            require_id(&source.id, "source")?;
            if !source_ids.insert(source.id.as_str()) {
                return Err(format!("duplicate source id: {}", source.id));
            }
            if !source.url.starts_with("https://") {
                return Err(format!("source {} must use an HTTPS URL", source.id));
            }
            for (name, value) in [
                ("name", source.name.as_str()),
                ("revision", source.revision.as_str()),
                ("license", source.license.as_str()),
                ("attribution", source.attribution.as_str()),
                ("redistribution", source.redistribution.as_str()),
            ] {
                if value.trim().is_empty() {
                    return Err(format!("source {} has an empty {name}", source.id));
                }
            }
        }
        let mut sample_ids = HashSet::new();
        for sample in &self.samples {
            require_id(&sample.id, "sample")?;
            if !sample_ids.insert(sample.id.as_str()) {
                return Err(format!("duplicate sample id: {}", sample.id));
            }
            if !source_ids.contains(sample.source_id.as_str()) {
                return Err(format!(
                    "sample {} references unknown source {}",
                    sample.id, sample.source_id
                ));
            }
            if !matches!(sample.split.as_str(), "test" | "validation") {
                return Err(format!(
                    "sample {} must use a test or validation split",
                    sample.id
                ));
            }
            if !valid_language_profile(&sample.language)
                || (sample.reference.trim().is_empty() && sample.scenario != Scenario::Noise)
            {
                return Err(format!(
                    "sample {} needs a language and reference text (only noise may have an empty reference)",
                    sample.id
                ));
            }
            validate_relative_path(&sample.audio_path, &sample.id)?;
            if sample.sha256.len() != 64
                || !sample
                    .sha256
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
            {
                return Err(format!(
                    "sample {} has an invalid lowercase SHA-256",
                    sample.id
                ));
            }
            if !sample.duration_seconds.is_finite() || sample.duration_seconds <= 0.0 {
                return Err(format!("sample {} has an invalid duration", sample.id));
            }
        }
        Ok(())
    }
}

fn valid_language_profile(value: &str) -> bool {
    let languages = value.split('+').collect::<Vec<_>>();
    !languages.is_empty()
        && languages.len() <= 2
        && languages.iter().all(|language| {
            let mut parts = language.split('-');
            let Some(primary) = parts.next() else {
                return false;
            };
            (2..=3).contains(&primary.len())
                && primary.bytes().all(|byte| byte.is_ascii_lowercase())
                && parts.all(|part| {
                    (2..=8).contains(&part.len())
                        && part.bytes().all(|byte| byte.is_ascii_alphanumeric())
                })
        })
}

impl BenchmarkEnvironment {
    pub fn from_path(path: &Path) -> Result<Self, String> {
        let bytes = std::fs::read(path)
            .map_err(|error| format!("cannot read environment {}: {error}", path.display()))?;
        let environment: Self = serde_json::from_slice(&bytes)
            .map_err(|error| format!("cannot decode environment {}: {error}", path.display()))?;
        environment.validate()?;
        Ok(environment)
    }

    pub fn validate(&self) -> Result<(), String> {
        for (name, value) in [
            ("label", self.label.as_str()),
            ("hardware", self.hardware.as_str()),
            ("operatingSystem", self.operating_system.as_str()),
            ("powerMode", self.power_mode.as_str()),
            ("backend", self.backend.as_str()),
            ("precision", self.precision.as_str()),
            ("applicationRevision", self.application_revision.as_str()),
            ("runtimeRevision", self.runtime_revision.as_str()),
            ("driver", self.driver.as_str()),
        ] {
            if value.trim().is_empty() {
                return Err(format!("benchmark environment has an empty {name}"));
            }
        }
        if self.threads == 0 {
            return Err("benchmark environment threads must be positive".into());
        }
        Ok(())
    }
}

fn require_id(value: &str, kind: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 120
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(format!("invalid {kind} id: {value}"));
    }
    Ok(())
}

fn validate_relative_path(value: &str, sample_id: &str) -> Result<(), String> {
    let path = Path::new(value);
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        return Err(format!(
            "sample {sample_id} audioPath must stay below the corpus root"
        ));
    }
    if value.is_empty() {
        return Err(format!("sample {sample_id} has an empty audioPath"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest() -> BenchmarkManifest {
        BenchmarkManifest {
            schema_version: 1,
            sources: vec![CorpusSource {
                id: "fleurs".into(),
                name: "FLEURS".into(),
                url: "https://example.test/fleurs".into(),
                revision: "abc".into(),
                license: "CC-BY-4.0".into(),
                attribution: "Example".into(),
                redistribution: "Attribution required".into(),
            }],
            samples: vec![BenchmarkSample {
                id: "en-1".into(),
                source_id: "fleurs".into(),
                split: "test".into(),
                language: "en".into(),
                audio_path: "fleurs/en-1.wav".into(),
                sha256: "a".repeat(64),
                reference: "hello".into(),
                duration_seconds: 1.0,
                scenario: Scenario::Read,
                replay_kind: ReplayKind::Natural,
                speaker_id: None,
            }],
        }
    }

    #[test]
    fn validates_reproducible_manifest_and_rejects_escaping_paths() {
        manifest().validate().unwrap();
        let mut invalid = manifest();
        invalid.samples[0].audio_path = "../private.wav".into();
        assert!(invalid.validate().unwrap_err().contains("corpus root"));
    }

    #[test]
    fn empty_reference_requires_explicit_noise_scenario() {
        let mut value = manifest();
        value.samples[0].reference.clear();
        assert!(value.validate().is_err());
        value.samples[0].scenario = Scenario::Noise;
        value.validate().unwrap();
        value.samples[0].language.clear();
        assert!(value.validate().is_err());
    }

    #[test]
    fn accepts_locales_and_code_switch_pairs_but_rejects_invalid_profiles() {
        let mut value = manifest();
        for language in ["hi-IN", "en-IN", "en-US", "hi+en"] {
            value.samples[0].language = language.into();
            value.validate().unwrap();
        }
        for language in ["", "Hindi", "hi+en+fr", "hi+english", "../hi"] {
            value.samples[0].language = language.into();
            assert!(value.validate().is_err(), "accepted {language}");
        }
    }

    #[test]
    fn rejects_synthetic_training_or_unknown_source_inputs() {
        let mut invalid = manifest();
        invalid.samples[0].split = "train".into();
        assert!(invalid
            .validate()
            .unwrap_err()
            .contains("test or validation"));
        invalid = manifest();
        invalid.samples[0].source_id = "missing".into();
        assert!(invalid.validate().unwrap_err().contains("unknown source"));
    }
}
