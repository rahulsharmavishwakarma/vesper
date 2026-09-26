//! Declarative model packs: data files only, never repository code.
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use crate::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelRuntime {
    SherpaOnnxTransducer,
    LlamaCpp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelFileRole {
    Encoder,
    Decoder,
    Joiner,
    Tokens,
    Weights,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HuggingFaceFile {
    pub role: ModelFileRole,
    pub path: String,
    pub sha256: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HuggingFaceProfile {
    pub id: String,
    pub name: String,
    pub repository: String,
    pub revision: String,
    pub runtime: ModelRuntime,
    pub files: Vec<HuggingFaceFile>,
    pub license: String,
    /// This adapter currently accepts the NeMo transducer export layout only.
    #[serde(default)]
    pub model_type: Option<String>,
    #[serde(default)]
    pub languages: Vec<String>,
}

fn invalid(message: impl Into<String>) -> Error {
    Error::InvalidConfig {
        field: "models.customProfile",
        message: message.into(),
    }
}

fn safe_component(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
}

fn lowercase_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}

impl HuggingFaceProfile {
    pub fn validate(&self) -> Result<()> {
        if self.id.len() > 80 || !safe_component(&self.id) {
            return Err(invalid(
                "profile id must be 1–80 ASCII letters, numbers, dots, underscores or hyphens",
            ));
        }
        if self.name.trim().is_empty()
            || self.name.chars().count() > 120
            || self.name.chars().any(char::is_control)
        {
            return Err(invalid("profile name must be 1–120 printable characters"));
        }
        let repo: Vec<_> = self.repository.split('/').collect();
        if repo.len() != 2
            || self.repository.len() > 200
            || !repo.iter().all(|part| safe_component(part))
        {
            return Err(invalid(
                "repository must be an owner/name Hugging Face model repository, not a URL",
            ));
        }
        if !lowercase_hex(&self.revision, 40) {
            return Err(invalid("revision must be an immutable 40-character lowercase Git commit hash; branches and tags are not accepted"));
        }
        if self.license.trim().is_empty()
            || self.license.len() > 500
            || self.license.chars().any(char::is_control)
        {
            return Err(invalid("record the model license in 1–500 printable bytes"));
        }
        if self.languages.len() > 64
            || self.languages.iter().any(|language| {
                language.is_empty()
                    || language.len() > 24
                    || !language
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || c == b'-')
            })
        {
            return Err(invalid("languages must be language codes such as en or hi"));
        }
        let expected: &[ModelFileRole] = match self.runtime {
            ModelRuntime::SherpaOnnxTransducer => {
                if self.model_type.as_deref() != Some("nemo_transducer")
                    || self.languages.is_empty()
                {
                    return Err(invalid("the speech adapter requires modelType nemo_transducer, declared languages, and a sherpa-onnx compatible 16 kHz / 80-feature NeMo transducer export"));
                }
                &[
                    ModelFileRole::Encoder,
                    ModelFileRole::Decoder,
                    ModelFileRole::Joiner,
                    ModelFileRole::Tokens,
                ]
            }
            ModelRuntime::LlamaCpp => {
                if self.model_type.is_some() {
                    return Err(invalid("modelType applies only to speech profiles; GGUF architecture comes from model metadata"));
                }
                &[ModelFileRole::Weights]
            }
        };
        let roles: HashSet<_> = self.files.iter().map(|file| file.role).collect();
        if self.files.len() != expected.len()
            || roles.len() != expected.len()
            || !expected.iter().all(|role| roles.contains(role))
        {
            return Err(invalid(match self.runtime {
                ModelRuntime::SherpaOnnxTransducer => {
                    "speech profiles require exactly one encoder, decoder, joiner and tokens file"
                }
                ModelRuntime::LlamaCpp => {
                    "writing model profiles require exactly one weights file (a complete, unsplit GGUF)"
                }
            }));
        }
        let mut paths = HashSet::new();
        for file in &self.files {
            if file.path.len() > 512
                || !file.path.split('/').all(safe_component)
                || !paths.insert(&file.path)
            {
                return Err(invalid("file paths must be unique relative repository paths using ASCII letters, numbers, dots, underscores, hyphens and slashes; traversal, URLs and escapes are forbidden"));
            }
            if !lowercase_hex(&file.sha256, 64) || !(1..=50_000_000_000).contains(&file.size_bytes)
            {
                return Err(invalid("each file requires its exact SHA-256 in lowercase hex and a size of 1–50,000,000,000 bytes"));
            }
            let extension = match file.role {
                ModelFileRole::Weights => ".gguf",
                ModelFileRole::Tokens => ".txt",
                _ => ".onnx",
            };
            if !file.path.ends_with(extension) {
                return Err(invalid(format!(
                    "{:?} requires a {extension} file",
                    file.role
                )));
            }
            if file.role == ModelFileRole::Weights
                && file
                    .path
                    .strip_suffix(".gguf")
                    .and_then(|stem| stem.rsplit_once("-of-"))
                    .is_some_and(|(prefix, count)| {
                        count.len() == 5
                            && count.bytes().all(|c| c.is_ascii_digit())
                            && prefix.rsplit_once('-').is_some_and(|(_, index)| {
                                index.len() == 5 && index.bytes().all(|c| c.is_ascii_digit())
                            })
                    })
            {
                return Err(invalid(
                    "split GGUF files are unsupported; select one complete, unsplit GGUF",
                ));
            }
        }
        Ok(())
    }

    /// Call validate before resolving paths. No user-supplied URL is accepted.
    pub fn relative_path(&self, file: &HuggingFaceFile) -> String {
        format!(
            "huggingface/{}/{}/{}",
            self.repository, self.revision, file.path
        )
    }

    pub fn download_url(&self, file: &HuggingFaceFile) -> String {
        format!(
            "https://huggingface.co/{}/resolve/{}/{}",
            self.repository, self.revision, file.path
        )
    }

    pub fn file(&self, role: ModelFileRole) -> Option<&HuggingFaceFile> {
        self.files.iter().find(|file| file.role == role)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> HuggingFaceProfile {
        HuggingFaceProfile {
            id: "example".into(),
            name: "Example cleanup".into(),
            repository: "owner/model".into(),
            revision: "a".repeat(40),
            runtime: ModelRuntime::LlamaCpp,
            files: vec![HuggingFaceFile {
                role: ModelFileRole::Weights,
                path: "weights/model.gguf".into(),
                sha256: "b".repeat(64),
                size_bytes: 42,
            }],
            license: "Apache-2.0".into(),
            model_type: None,
            languages: vec![],
        }
    }
    #[test]
    fn rejects_traversal_urls_escapes_and_mutable_revisions() {
        fixture().validate().unwrap();
        for path in [
            "../model.gguf",
            "/model.gguf",
            "x//model.gguf",
            "x/./model.gguf",
            "x/../model.gguf",
            "x\\model.gguf",
            "%2e%2e/model.gguf",
            "model.gguf?token=x",
            "model.gguf#fragment",
            "C:/model.gguf",
            "\0.gguf",
        ] {
            let mut profile = fixture();
            profile.files[0].path = path.into();
            assert!(profile.validate().is_err(), "accepted {path:?}");
        }
        for repository in [
            "https://huggingface.co/a/b",
            "../repo",
            "owner/..",
            "a/b/c",
            "a/b?x=y",
        ] {
            let mut profile = fixture();
            profile.repository = repository.into();
            assert!(profile.validate().is_err());
        }
        for revision in ["main".into(), "v1".into(), "A".repeat(40), "../".into()] {
            let mut profile = fixture();
            profile.revision = revision;
            assert!(profile.validate().is_err());
        }
    }
    #[test]
    fn rejects_missing_duplicate_and_incompatible_roles() {
        let mut profile = fixture();
        profile.files.clear();
        assert!(profile.validate().is_err());
        let mut profile = fixture();
        profile.files.push(profile.files[0].clone());
        assert!(profile.validate().is_err());
        let mut profile = fixture();
        profile.runtime = ModelRuntime::SherpaOnnxTransducer;
        profile.model_type = Some("nemo_transducer".into());
        profile.languages = vec!["en".into()];
        assert!(profile.validate().is_err());
        let mut profile = fixture();
        profile.files[0].sha256 = "0".repeat(63);
        assert!(profile.validate().is_err());
        let mut profile = fixture();
        profile.files[0].size_bytes = 0;
        assert!(profile.validate().is_err());
        let mut profile = fixture();
        profile.files[0].path = "model-00001-of-00002.gguf".into();
        assert!(profile.validate().is_err());
    }
}
