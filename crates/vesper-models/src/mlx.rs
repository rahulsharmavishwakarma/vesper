//! Application-built code and pinned model data have separate trust/installation paths.
use super::*;

pub fn runtime_directory() -> PathBuf {
    // A bundle must never fall back to developer files when its resources are missing.
    if let Ok(executable) = std::env::current_exe() {
        if let Some(parent) = executable
            .parent()
            .filter(|p| p.file_name() == Some(OsStr::new("MacOS")))
        {
            return parent.join("../Resources/mlx-runtime");
        }
    }
    PathBuf::from(env!("VESPER_MLX_BUILD_DIRECTORY"))
}

pub fn runtime_artifacts() -> Vec<Artifact> {
    serde_json::from_str(include_str!(concat!(env!("OUT_DIR"), "/mlx-runtime.json")))
        .expect("compiled MLX build identity")
}

pub fn runtime_ready() -> bool {
    let root = runtime_directory();
    runtime_artifacts()
        .iter()
        .all(|a| artifact_file_installed(&root, a))
}

/// The app runtime is immutable for a process. Cache the device probe, not model
/// verification; every model load still checks current runtime and model hashes.
pub(super) fn available() -> Result<bool> {
    static DEVICE: OnceLock<std::result::Result<bool, String>> = OnceLock::new();
    // Feature-off builds never enter here. A feature-on package missing or
    // altering its own code is a broken installation and stays a hard error.
    // The operating system refusing to load or probe the intact library (for
    // example library validation under the hardened runtime, #245) is an
    // environment problem: the GPU path is unavailable, the reason is kept, and
    // the same model runs on its CPU export.
    DEVICE
        .get_or_init(|| {
            verify_runtime(&AtomicBool::new(false)).map_err(|e| e.to_string())?;
            let (available, failure) = probe_outcome(vesper_mlx::device_available(
                &runtime_directory().join("libVesperMLX.dylib"),
            ));
            if let Some(reason) = failure {
                let _ = LOAD_FAILURE.set(reason);
            }
            Ok(available)
        })
        .clone()
        .map_err(ModelError::RuntimeUnavailable)
}

static LOAD_FAILURE: OnceLock<String> = OnceLock::new();

/// The full operating-system reason the GPU library could not be loaded, if any.
pub(super) fn load_failure() -> Option<String> {
    LOAD_FAILURE.get().cloned()
}

fn probe_outcome(probe: std::result::Result<bool, vesper_mlx::Error>) -> (bool, Option<String>) {
    match probe {
        Ok(available) => (available, None),
        Err(error) => (false, Some(error.to_string())),
    }
}

pub fn verify_runtime(cancelled: &AtomicBool) -> Result<()> {
    let root = runtime_directory();
    for a in runtime_artifacts() {
        validate_reader(
            File::open(artifact_path(&root, &a.relative_path)?)?,
            &a,
            cancelled,
        )?;
    }
    Ok(())
}

pub fn model_subdirectory(model: AsrModel) -> Result<&'static str> {
    match model {
        AsrModel::ParakeetTdt06bV3 => Ok("mlx/parakeet"),
        AsrModel::Qwen3Asr06b => Ok("mlx/qwen"),
        AsrModel::WhisperSmall => Ok("mlx/whisper-small"),
        AsrModel::GraniteSpeech5TurboCtc => Ok("mlx/granite-speech-5.0-turboctc"),
        _ => Err(ModelError::Unsupported(
            "this MLX variant has not passed qualification".into(),
        )),
    }
}

pub(super) fn prepare_tokenizer(
    root: &Path,
    artifact: &Artifact,
    cancelled: &AtomicBool,
) -> Result<()> {
    check_cancel(cancelled)?;
    verify_runtime(cancelled)?;
    let mut config = AppConfig::default();
    config.models.asr = AsrModel::Qwen3Asr06b;
    config.compute_backend = ComputeBackend::Metal;
    config.cleanup_enabled = false;
    // Never run a generator against unchecked inputs or an existing corrupt output.
    let directory = artifact_path(root, "mlx/qwen")?;
    std::fs::create_dir_all(&directory)?;
    let staging = tempfile::tempdir_in(&directory)?;
    for input in selected(&config)?.into_iter().filter(|a| {
        ["vocab.json", "merges.txt", "tokenizer_config.json"]
            .iter()
            .any(|name| a.relative_path == format!("mlx/qwen/{name}"))
    }) {
        let source = artifact_path(root, &input.relative_path)?;
        validate_reader(File::open(&source)?, &input, cancelled)?;
        let target = staging
            .path()
            .join(Path::new(&input.relative_path).file_name().unwrap());
        std::fs::copy(&source, &target)?;
        validate_reader(File::open(&target)?, &input, cancelled)?;
    }
    let mut child = Command::new(runtime_directory().join("prepare-qwen-tokenizer"))
        .arg(staging.path())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    let status = loop {
        if cancelled.load(Ordering::Acquire) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(ModelError::Cancelled);
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => std::thread::sleep(Duration::from_millis(10)),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error.into());
            }
        }
    };
    if !status.success() {
        return Err(ModelError::RuntimeUnavailable(
            "Qwen tokenizer preparation failed".into(),
        ));
    }
    let output = staging.path().join("tokenizer.json");
    validate_reader(File::open(&output)?, artifact, cancelled)?;
    check_cancel(cancelled)?;
    File::open(&output)?.sync_all()?;
    std::fs::rename(output, artifact_path(root, &artifact.relative_path)?)?;
    Ok(())
}

pub(super) fn verify_directory(config: &AppConfig, root: &Path) -> Result<()> {
    let directory = artifact_path(root, model_subdirectory(config.models.asr)?)?;
    let allowed: std::collections::BTreeSet<_> = selected(config)?
        .into_iter()
        .filter(|a| a.role == "asr")
        .map(|a| root.join(a.relative_path))
        .collect();
    // Upstream loaders enumerate safetensors. A verified manifest must never be
    // supplemented by an unverified shard or an alternate tokenizer directory.
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        if entry.file_name() != ".DS_Store" && !allowed.contains(&entry.path()) {
            return Err(ModelError::Integrity(
                "unexpected file in the MLX model directory".into(),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refused_library_load_degrades_to_cpu_with_the_reason() {
        let (available, reason) = probe_outcome(Err(vesper_mlx::Error::Runtime(
            "dlopen failed: different Team IDs".into(),
        )));
        assert!(!available);
        assert!(reason.unwrap().contains("different Team IDs"));
        assert_eq!(probe_outcome(Ok(true)), (true, None));
        assert_eq!(probe_outcome(Ok(false)), (false, None));
    }

    #[test]
    fn unexpected_weight_shards_are_rejected_before_native_loading() {
        let root = tempfile::tempdir().unwrap();
        let mut config = AppConfig::default();
        config.models.asr = AsrModel::ParakeetTdt06bV3;
        config.compute_backend = ComputeBackend::Metal;
        config.cleanup_enabled = false;
        let directory = root.path().join("mlx/parakeet");
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("config.json"), "fixture").unwrap();
        // Layout validation alone doesn't attest hashes; verify_asr does both.
        verify_directory(&config, root.path()).unwrap();
        std::fs::write(directory.join("unverified.safetensors"), "fixture").unwrap();
        assert!(matches!(
            verify_directory(&config, root.path()),
            Err(ModelError::Integrity(_))
        ));
    }

    #[test]
    fn selected_metal_files_never_mix_with_cpu_exports() {
        for model in [
            AsrModel::ParakeetTdt06bV3,
            AsrModel::Qwen3Asr06b,
            AsrModel::WhisperSmall,
        ] {
            let mut config = AppConfig::default();
            config.models.asr = model;
            config.cleanup_enabled = false;
            config.compute_backend = ComputeBackend::Metal;
            let gpu = selected(&config).unwrap();
            assert!(!gpu.is_empty());
            assert!(gpu
                .iter()
                .all(|a| a.relative_path.starts_with("mlx/")
                    && a.backend.as_deref() == Some("metal")));
            assert_eq!(built_in_asr_spec(&config, model).artifacts.len(), gpu.len());
            config.compute_backend = ComputeBackend::Auto;
            let automatic = selected(&config).unwrap();
            assert_eq!(
                automatic
                    .iter()
                    .any(|a| a.relative_path.starts_with("mlx/")),
                available().unwrap()
            );
            config.compute_backend = ComputeBackend::Cpu;
            assert!(selected(&config)
                .unwrap()
                .iter()
                .all(|a| !a.relative_path.starts_with("mlx/")));
        }
        for model in [AsrModel::CohereTranscribe, AsrModel::WhisperTurbo] {
            let mut config = AppConfig::default();
            config.models.asr = model;
            config.compute_backend = ComputeBackend::Metal;
            assert!(selected(&config).is_err());
        }
    }

    #[test]
    #[ignore = "pinned installed Qwen inputs and application-built tokenizer tool"]
    fn tokenizer_preparation_repairs_corruption_and_checks_inputs() {
        let installed =
            PathBuf::from(vesper_core::environment::var_os("VESPER_SMOKE_MODELS").expect("models"));
        let scratch = tempfile::tempdir().unwrap();
        let directory = scratch.path().join("mlx/qwen");
        std::fs::create_dir_all(&directory).unwrap();
        for name in ["vocab.json", "merges.txt", "tokenizer_config.json"] {
            std::fs::copy(installed.join("mlx/qwen").join(name), directory.join(name)).unwrap();
        }
        let artifact = catalog()
            .into_iter()
            .find(|a| a.url == "builtin:qwen3-tokenizer-v1")
            .unwrap();
        let flag = AtomicBool::new(false);
        for _ in 0..2 {
            std::fs::write(directory.join("tokenizer.json"), "corrupt old output").unwrap();
            prepare_tokenizer(scratch.path(), &artifact, &flag).unwrap();
            validate_reader(
                File::open(directory.join("tokenizer.json")).unwrap(),
                &artifact,
                &flag,
            )
            .unwrap();
        }
        std::fs::write(directory.join("vocab.json"), "corrupt source").unwrap();
        assert!(matches!(
            prepare_tokenizer(scratch.path(), &artifact, &flag),
            Err(ModelError::Integrity(_))
        ));
        assert!(matches!(
            prepare_tokenizer(scratch.path(), &artifact, &AtomicBool::new(true)),
            Err(ModelError::Cancelled)
        ));
    }
}
