//! GA weights plus a separately namespaced, locally built macOS runtime.
use super::*;

pub const MODEL_ID: &str = "nvidia/Nemotron-3-Diarization";
pub const MODEL_REVISION: &str = "f667ed73aee57d40cc39428eb768b4fd87a0a29e";
pub const RUNTIME_REVISION: &str = "97a15afa5caa9bce5baaa86c1184103877af4101";
pub const LIBRARY_PREFIX: &str = "libvesper_diar_97a15afa_";
const COMPONENTS: [&str; 7] = ["api", "asr", "ggml", "base", "cpu", "blas", "metal"];

pub fn supported() -> bool {
    cfg!(all(target_os = "macos", target_arch = "aarch64"))
}
pub fn artifact() -> Artifact {
    Artifact {
        name: "Nemotron-3-Diarization.q8_0.gguf".into(),
        relative_path: "nemotron-3-diarization/Nemotron-3-Diarization.q8_0.gguf".into(),
        url: format!("https://huggingface.co/{MODEL_ID}/resolve/{MODEL_REVISION}/Nemotron-3-Diarization.q8_0.gguf"),
        revision: MODEL_REVISION.into(),
        sha256:"08456d9e22cd9a323c0364d98375f3746d6e68507ebb705cd46438c534c7a3a1".into(),
        size_bytes:107_012_128, role:"diarization".into(),model_id:Some(MODEL_ID.into()),
        platform:None,backend:None,archive:None,quantization:None,license:"OpenMDW-1.1".into(),
    }
}
pub fn runtime_relative() -> String {
    format!("nemotron-diarization-runtime/{RUNTIME_REVISION}")
}
pub fn library_path(root: &Path) -> PathBuf {
    root.join(runtime_relative())
        .join(format!("{LIBRARY_PREFIX}api.dylib"))
}
pub fn runtime_ready(root: &Path) -> bool {
    supported()
        && root
            .join(runtime_relative())
            .join("manifest.json")
            .is_file()
        && COMPONENTS.iter().all(|c| {
            root.join(runtime_relative())
                .join(format!("{LIBRARY_PREFIX}{c}.dylib"))
                .is_file()
        })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Bundle {
    source_revision: String,
    files: Vec<BundleFile>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BundleFile {
    name: String,
    sha256: String,
    size_bytes: u64,
}

/// Full verification on every load. The local-build manifest records generated
/// binary hashes, not a publisher signature or published downloadable runtime.
pub fn verify(root: &Path, cancelled: &AtomicBool) -> Result<()> {
    if !supported() {
        return Err(ModelError::Unsupported(
            "experimental diarization requires Apple silicon macOS".into(),
        ));
    }
    let weights = artifact();
    validate_reader(
        File::open(artifact_path(root, &weights.relative_path)?)?,
        &weights,
        cancelled,
    )?;
    verify_bundle(root, cancelled)
}

fn verify_bundle(root: &Path, cancelled: &AtomicBool) -> Result<()> {
    let weights = artifact();
    let relative = runtime_relative();
    let path = artifact_path(root, &format!("{relative}/manifest.json"))?;
    if std::fs::metadata(&path)?.len() > 16_384 {
        return Err(ModelError::Integrity(
            "diarization manifest too large".into(),
        ));
    }
    let bundle: Bundle = serde_json::from_slice(&std::fs::read(path)?)
        .map_err(|e| ModelError::Integrity(e.to_string()))?;
    let expected: std::collections::BTreeSet<_> = COMPONENTS
        .iter()
        .map(|c| format!("{LIBRARY_PREFIX}{c}.dylib"))
        .collect();
    let found: std::collections::BTreeSet<_> =
        bundle.files.iter().map(|f| f.name.clone()).collect();
    if bundle.source_revision != RUNTIME_REVISION || found != expected || bundle.files.len() != 7 {
        return Err(ModelError::Integrity(
            "diarization runtime bundle identity".into(),
        ));
    }
    for file in bundle.files {
        if file.size_bytes == 0
            || file.sha256.len() != 64
            || !file.sha256.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(ModelError::Integrity(
                "diarization runtime file digest".into(),
            ));
        }
        let entry = Artifact {
            name: file.name.clone(),
            relative_path: format!("{relative}/{}", file.name),
            sha256: file.sha256,
            size_bytes: file.size_bytes,
            ..weights.clone()
        };
        validate_reader(
            File::open(artifact_path(root, &entry.relative_path)?)?,
            &entry,
            cancelled,
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_identity_and_every_byte_must_match_before_loading() {
        for damage in [
            "none",
            "revision",
            "duplicate",
            "traversal",
            "digest",
            "bytes",
            "missing",
            "oversized",
        ] {
            let temp = tempfile::tempdir().unwrap();
            let directory = temp.path().join(runtime_relative());
            std::fs::create_dir_all(&directory).unwrap();
            let mut files = Vec::new();
            for component in COMPONENTS {
                let name = format!("{LIBRARY_PREFIX}{component}.dylib");
                std::fs::write(directory.join(&name), b"fixture").unwrap();
                files.push(serde_json::json!({"name":name, "sizeBytes":7,
                    "sha256":format!("{:x}", Sha256::digest(b"fixture"))}));
            }
            let first = directory.join(files[0]["name"].as_str().unwrap());
            match damage {
                "duplicate" => files[1] = files[0].clone(),
                "traversal" => files[0]["name"] = "../outside.dylib".into(),
                "digest" => files[0]["sha256"] = "not-a-hash".into(),
                "bytes" => std::fs::write(first, b"corrupt").unwrap(),
                "missing" => std::fs::remove_file(first).unwrap(),
                _ => {}
            }
            let revision = if damage == "revision" {
                "old-runtime"
            } else {
                RUNTIME_REVISION
            };
            let manifest = serde_json::json!({"sourceRevision":revision,"files":files});
            let bytes = if damage == "oversized" {
                vec![b' '; 16_385]
            } else {
                serde_json::to_vec(&manifest).unwrap()
            };
            std::fs::write(directory.join("manifest.json"), bytes).unwrap();
            assert_eq!(
                verify_bundle(temp.path(), &AtomicBool::new(false)).is_ok(),
                damage == "none",
                "{damage}"
            );
            if damage == "none" {
                assert!(matches!(
                    verify_bundle(temp.path(), &AtomicBool::new(true)),
                    Err(ModelError::Cancelled)
                ));
                #[cfg(unix)]
                {
                    let path = directory.join(format!("{LIBRARY_PREFIX}api.dylib"));
                    let target = temp.path().join("untrusted.dylib");
                    std::fs::write(&target, b"fixture").unwrap();
                    std::fs::remove_file(&path).unwrap();
                    std::os::unix::fs::symlink(target, path).unwrap();
                    assert!(verify_bundle(temp.path(), &AtomicBool::new(false)).is_err());
                }
            }
        }
    }
}
