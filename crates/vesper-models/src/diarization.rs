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

pub fn supports_backend(backend: ComputeBackend) -> bool {
    supported()
        && matches!(
            backend,
            ComputeBackend::Auto | ComputeBackend::Cpu | ComputeBackend::Metal
        )
}

/// Cheap admission for automatic analysis. Existing files are fully verified by
/// `verify` before loading; presence is never an integrity attestation.
pub fn available(root: &Path, backend: ComputeBackend) -> bool {
    supports_backend(backend)
        && root.join(artifact().relative_path).is_file()
        && runtime_ready(root)
}
pub fn artifact() -> Artifact {
    Artifact {
        name: "Nemotron-3-Diarization.q8_0.gguf".into(),
        relative_path: "nemotron-3-diarization/Nemotron-3-Diarization.q8_0.gguf".into(),
        url: format!("https://huggingface.co/{MODEL_ID}/resolve/{MODEL_REVISION}/Nemotron-3-Diarization.q8_0.gguf"),
        revision: MODEL_REVISION.into(),
        sha256:"08456d9e22cd9a323c0364d98375f3746d6e68507ebb705cd46438c534c7a3a1".into(),
        size_bytes:107_012_128, role:"diarization".into(),model_id:Some(MODEL_ID.into()),
        platform:None,backend:None,archive:None,engine:None,quantization:None,license:"OpenMDW-1.1".into(),
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

/// Bundled runtime source for first-run installs. Inside a packaged .app this
/// is `Contents/Resources/diarization-runtime`; in development it is the
/// build-time staging directory. Missing source is not an error: the install
/// step skips and speaker timelines stay unavailable.
pub fn bundled_runtime_directory() -> Option<PathBuf> {
    if let Ok(executable) = std::env::current_exe() {
        if let Some(parent) = executable
            .parent()
            .filter(|p| p.file_name() == Some(std::ffi::OsStr::new("MacOS")))
        {
            return Some(parent.join("../Resources/diarization-runtime"));
        }
    }
    let staging = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../.tooling/diarization-runtime/nemotron-diarization-runtime");
    staging.is_dir().then_some(staging)
}

/// First-run pre-check: install the bundled speaker runtime into the model
/// directory when it is missing or incomplete. Returns true when the runtime
/// is ready afterwards. Never touches an existing installation when no usable
/// bundled source exists; verifies every installed byte before reporting
/// success and removes a failed install rather than leaving it behind.
pub fn ensure_bundled_runtime(root: &Path, cancelled: &AtomicBool) -> Result<bool> {
    if !supported() {
        return Ok(false);
    }
    if runtime_ready(root) {
        return Ok(true);
    }
    let Some(bundled) = bundled_runtime_directory() else {
        return Ok(false);
    };
    install_bundled_runtime(root, &bundled, cancelled)
}

/// Install a staged runtime bundle into the model directory. The bundled
/// directory mirrors the model-root layout: it must contain a
/// `<RUNTIME_REVISION>` child holding the manifest and libraries.
pub fn install_bundled_runtime(
    root: &Path,
    bundled: &Path,
    cancelled: &AtomicBool,
) -> Result<bool> {
    let source = bundled.join(RUNTIME_REVISION);
    if !source.is_dir() {
        return Ok(false);
    }
    let destination = root.join(runtime_relative());
    if destination.exists() {
        std::fs::remove_dir_all(&destination)?;
    }
    std::fs::create_dir_all(destination.parent().ok_or(ModelError::Integrity(
        "diarization runtime has no parent directory".into(),
    ))?)?;
    let outcome = copy_runtime_tree(&source, &destination, cancelled)
        .and_then(|_| verify_runtime_bundle(root, cancelled));
    if outcome.is_err() && destination.exists() {
        let _ = std::fs::remove_dir_all(&destination);
    }
    outcome.map(|_| true)
}

fn copy_runtime_tree(source: &Path, destination: &Path, cancelled: &AtomicBool) -> Result<()> {
    std::fs::create_dir_all(destination)?;
    let mut entries: Vec<_> = std::fs::read_dir(source)?.collect::<std::io::Result<_>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        if cancelled.load(std::sync::atomic::Ordering::Acquire) {
            return Err(ModelError::Cancelled);
        }
        let file_type = entry.file_type()?;
        if !file_type.is_file() {
            continue;
        }
        let target = destination.join(entry.file_name());
        std::fs::copy(entry.path(), &target)?;
        File::open(&target)?.sync_all()?;
    }
    Ok(())
}

/// Hash verification for the runtime bundle alone, without requiring the
/// weight file. Used to validate a first-run install before reporting it.
pub fn verify_runtime_bundle(root: &Path, cancelled: &AtomicBool) -> Result<()> {
    if !supported() {
        return Err(ModelError::Unsupported(
            "experimental diarization requires Apple silicon macOS".into(),
        ));
    }
    verify_bundle(root, cancelled)
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
    fn automatic_admission_requires_model_complete_runtime_and_supported_backend() {
        let root = tempfile::tempdir().unwrap();
        assert!(!available(root.path(), ComputeBackend::Auto));
        let model = root.path().join(artifact().relative_path);
        std::fs::create_dir_all(model.parent().unwrap()).unwrap();
        std::fs::write(&model, b"not verified weights").unwrap();
        assert!(!available(root.path(), ComputeBackend::Auto));
        let bundle = root.path().join(runtime_relative());
        std::fs::create_dir_all(&bundle).unwrap();
        std::fs::write(bundle.join("manifest.json"), b"{}").unwrap();
        for component in COMPONENTS {
            assert!(!available(root.path(), ComputeBackend::Auto));
            std::fs::write(
                bundle.join(format!("{LIBRARY_PREFIX}{component}.dylib")),
                b"fixture",
            )
            .unwrap();
        }
        for backend in [
            ComputeBackend::Auto,
            ComputeBackend::Cpu,
            ComputeBackend::Metal,
        ] {
            assert_eq!(available(root.path(), backend), supported());
        }
        for backend in [
            ComputeBackend::Cuda,
            ComputeBackend::Vulkan,
            ComputeBackend::Rocm,
        ] {
            assert!(!available(root.path(), backend));
        }
        // Admission does not allow corrupt files through the verification gate.
        assert!(verify(root.path(), &AtomicBool::new(false)).is_err());
        std::fs::remove_file(model).unwrap();
        assert!(!available(root.path(), ComputeBackend::Cpu));
    }

    #[test]
    fn first_run_install_copies_verifies_and_cleans_up() {
        let idle = AtomicBool::new(false);
        let staged = tempfile::tempdir().unwrap();
        let revision = staged.path().join(RUNTIME_REVISION);
        std::fs::create_dir_all(&revision).unwrap();
        let mut files = Vec::new();
        for component in COMPONENTS {
            let name = format!("{LIBRARY_PREFIX}{component}.dylib");
            std::fs::write(revision.join(&name), b"fixture").unwrap();
            files.push(serde_json::json!({"name":name, "sizeBytes":7,
                "sha256":format!("{:x}", Sha256::digest(b"fixture"))}));
        }
        std::fs::write(
            revision.join("manifest.json"),
            serde_json::to_vec(
                &serde_json::json!({"sourceRevision":RUNTIME_REVISION,"files":files}),
            )
            .unwrap(),
        )
        .unwrap();
        // A bundled directory without the pinned revision is a skip that
        // leaves the model root untouched.
        let elsewhere = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        assert!(!install_bundled_runtime(root.path(), elsewhere.path(), &idle).unwrap());
        assert!(!root.path().join(runtime_relative()).exists());
        assert!(install_bundled_runtime(root.path(), staged.path(), &idle).unwrap());
        assert_eq!(runtime_ready(root.path()), supported());
        if supported() {
            assert!(verify_runtime_bundle(root.path(), &idle).is_ok());
            // A second install over a complete bundle stays ready.
            assert!(install_bundled_runtime(root.path(), staged.path(), &idle).unwrap());
            assert!(runtime_ready(root.path()));
        }
        // A tampered staged file fails closed and removes the destination.
        std::fs::write(
            revision.join(format!("{LIBRARY_PREFIX}api.dylib")),
            b"tampered",
        )
        .unwrap();
        let dirty = tempfile::tempdir().unwrap();
        assert!(install_bundled_runtime(dirty.path(), staged.path(), &idle).is_err());
        assert!(!dirty.path().join(runtime_relative()).exists());
        // Cancellation aborts the install without reporting success.
        let busy = AtomicBool::new(true);
        let pending = tempfile::tempdir().unwrap();
        assert!(matches!(
            install_bundled_runtime(pending.path(), staged.path(), &busy),
            Err(ModelError::Cancelled)
        ));
    }
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
