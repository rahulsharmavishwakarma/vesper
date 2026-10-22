//! Pinned model artifacts, explicit installation, and integrity verification.
use std::{
    ffi::OsStr,
    fs::File,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicBool, Ordering},
    sync::OnceLock,
    time::Duration,
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use vesper_core::{
    AppConfig, AsrModel, ComputeBackend, HuggingFaceProfile, Language, ModelRole, RewriteModel,
    RewriteQuantization,
};

#[cfg(feature = "mlx")]
pub mod mlx;
mod selection;
pub use selection::{preferred_speech_config, resolve_speech, SpeechSelection};
pub mod diarization;
mod hub_cache;
mod storage;
pub use storage::{remove_model, ModelRemoval};

#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    #[error("model I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("model download failed: {0}")]
    Download(#[from] reqwest::Error),
    #[error("model integrity check failed: {0}; install the model again")]
    Integrity(String),
    #[error("operation cancelled")]
    Cancelled,
    #[error("the selected built-in speech model is unavailable on {0}")]
    Unsupported(String),
    #[error("model runtime archive is invalid: {0}")]
    InvalidArchive(String),
    #[error("required model runtime is unavailable: {0}")]
    RuntimeUnavailable(String),
    #[error("unknown installable model: {0}")]
    UnknownModel(String),
    #[error("invalid model installation request: {0}")]
    InvalidRequest(String),
    #[error(transparent)]
    Configuration(#[from] vesper_core::Error),
    #[error(
        "model storage path contains a symbolic link below the configured model directory: {0}"
    )]
    UnsafePath(PathBuf),
}
pub type Result<T> = std::result::Result<T, ModelError>;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Artifact {
    pub name: String,
    pub relative_path: String,
    pub url: String,
    pub revision: String,
    pub sha256: String,
    pub size_bytes: u64,
    pub role: String,
    #[serde(default)]
    pub model_id: Option<String>,
    #[serde(default)]
    pub platform: Option<String>,
    #[serde(default)]
    pub backend: Option<String>,
    #[serde(default)]
    pub archive: Option<String>,
    /// Native engine for a model with more than one; `nemo_gguf` marks the
    /// one-file NeMo-Speech.cpp export. Absent means the standard export.
    #[serde(default)]
    pub engine: Option<String>,
    pub quantization: Option<String>,
    pub license: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledFile {
    pub name: String,
    pub installed: bool,
    pub size_bytes: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Installation {
    /// Required inference files are present. Optional frontends may fall back.
    pub ready: bool,
    pub model_directory: String,
    pub files: Vec<InstalledFile>,
    pub supported: bool,
    pub message: String,
}

/// Status of a built-in, shipped preset, or currently configured imported model.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInstallation {
    pub model_id: String,
    pub name: String,
    pub role: ModelRole,
    pub quantization: Option<RewriteQuantization>,
    pub selected: bool,
    pub ready: bool,
    pub artifacts_installed: bool,
    pub runtime_ready: bool,
    pub model_directory: String,
    pub files: Vec<InstalledFile>,
    pub supported: bool,
    pub message: String,
    pub local_file_count: usize,
    pub downloaded_bytes: u64,
    pub removable_bytes: u64,
    pub removal_error: Option<String>,
}

pub fn catalog() -> Vec<Artifact> {
    let mut artifacts: Vec<Artifact> =
        serde_json::from_str(include_str!("../../../models/artifacts.json"))
            .expect("checked-in model artifact manifest must be valid");
    // Inventory must find previous accelerator downloads even in a portable
    // build. Executable selection remains gated by AppConfig and backend below.
    artifacts.extend(
        serde_json::from_str::<Vec<Artifact>>(include_str!("../../../models/mlx-artifacts.json"))
            .expect("pinned MLX artifacts"),
    );
    for artifact in &mut artifacts {
        if artifact.role == "rewriter" && artifact.model_id.is_none() {
            artifact.model_id = Some(RewriteModel::Qwen35_08b.id().into());
        }
    }
    artifacts.push(diarization::artifact());
    artifacts
}

pub const NEMO_SPEECH_VERSION: &str = "0.1.0";
static CUDA_DEVICE_AVAILABLE: OnceLock<bool> = OnceLock::new();

fn cuda_is_visible(value: Option<&OsStr>) -> bool {
    value.is_none_or(|value| {
        let value = value.to_string_lossy();
        let value = value.trim();
        !value.is_empty() && value != "-1" && !value.eq_ignore_ascii_case("none")
    })
}

/// Conservative process-level CUDA probe used by `Auto`. An explicit CUDA
/// selection remains available when a container exposes CUDA without nvidia-smi.
pub fn cuda_device_available() -> bool {
    if !matches!(std::env::consts::OS, "linux" | "windows")
        || !cuda_is_visible(std::env::var_os("CUDA_VISIBLE_DEVICES").as_deref())
    {
        return false;
    }
    *CUDA_DEVICE_AVAILABLE.get_or_init(|| {
        Command::new("nvidia-smi")
            .args(["--query-gpu=index", "--format=csv,noheader,nounits"])
            .output()
            .is_ok_and(|output| output.status.success() && !output.stdout.is_empty())
    })
}

fn target_platform() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => Some("macos-aarch64"),
        ("macos", "x86_64") => Some("macos-x86_64"),
        ("linux", "aarch64") => Some("linux-aarch64"),
        ("linux", "x86_64") => Some("linux-x86_64"),
        ("windows", "x86_64") => Some("windows-x86_64"),
        _ => None,
    }
}

pub fn nemo_speech_executable(root: &Path) -> PathBuf {
    root.join("nemo-speech-runtime")
        .join(NEMO_SPEECH_VERSION)
        .join("nemo-speech")
        .join("bin")
        .join(if cfg!(windows) {
            "nemo-speech.exe"
        } else {
            "nemo-speech"
        })
}

/// Versioned NeMo-Speech.cpp C ABI library installed beside the CLI.
pub fn nemo_speech_library(root: &Path) -> PathBuf {
    if let Some(path) = vesper_core::environment::var_os("VESPER_NEMO_SPEECH_LIB") {
        return PathBuf::from(path);
    }
    nemo_speech_library_in_runtime(
        &root
            .join("nemo-speech-runtime")
            .join(NEMO_SPEECH_VERSION)
            .join("nemo-speech"),
    )
}

fn nemo_speech_library_in_runtime(runtime: &Path) -> PathBuf {
    let name = if cfg!(target_os = "windows") {
        "nemo_speech_asr_c.dll"
    } else if cfg!(target_os = "macos") {
        "libnemo_speech_asr_c.1.dylib"
    } else {
        "libnemo_speech_asr_c.so.1"
    };
    runtime
        .join(if cfg!(target_os = "windows") {
            "bin"
        } else {
            "lib"
        })
        .join(name)
}

pub fn resolved_nemo_backend(config: &AppConfig) -> Result<&'static str> {
    let platform = target_platform().ok_or_else(|| {
        ModelError::Unsupported(format!(
            "{}-{}",
            std::env::consts::OS,
            std::env::consts::ARCH
        ))
    })?;
    let backend = match config.compute_backend {
        ComputeBackend::Auto if platform == "macos-aarch64" => "metal",
        ComputeBackend::Auto if cuda_device_available() => "cuda",
        ComputeBackend::Auto | ComputeBackend::Cpu => "cpu",
        ComputeBackend::Metal if platform == "macos-aarch64" => "metal",
        ComputeBackend::Cuda
            if matches!(
                platform,
                "linux-aarch64" | "linux-x86_64" | "windows-x86_64"
            ) =>
        {
            "cuda"
        }
        ComputeBackend::Vulkan
            if matches!(
                platform,
                "linux-aarch64" | "linux-x86_64" | "windows-x86_64"
            ) =>
        {
            "vulkan"
        }
        // NeMo-Speech.cpp does not publish a ROCm runtime. On an AMD profile,
        // speech uses its Vulkan package while llama.cpp cleanup uses HIP/ROCm.
        ComputeBackend::Rocm if matches!(platform, "linux-x86_64" | "windows-x86_64") => "vulkan",
        requested => {
            return Err(ModelError::Unsupported(format!(
                "{platform} with {requested:?}"
            )))
        }
    };
    Ok(backend)
}

/// `engine` value of the one-file NeMo-Speech.cpp GGUF export in the manifest.
pub const NEMO_GGUF_ENGINE: &str = "nemo_gguf";
/// `engine` and `backend` values of the Core ML encoder plus MLX decoder files.
pub const NEURAL_ENGINE_ENGINE: &str = "coreml_ane";
pub const NEURAL_ENGINE_BACKEND: &str = "neural_engine";

/// The selected built-in speech model runs on the bundled NeMo-Speech.cpp runtime.
pub fn uses_nemo_runtime(config: &AppConfig) -> bool {
    config.models.custom_asr.is_none()
        && (config.models.asr == AsrModel::Nemotron35AsrStreaming06b || config.uses_nemo_gguf())
}

/// NeMo runtime package to install. On Apple silicon the Metal package also
/// contains the CPU backend, so one download serves both devices and switching
/// between them, or an automatic CPU retry, needs no further files.
pub fn nemo_package_backend(config: &AppConfig) -> Result<&'static str> {
    if target_platform() == Some("macos-aarch64") {
        return Ok("metal");
    }
    resolved_nemo_backend(config)
}

pub fn selected(config: &AppConfig) -> Result<Vec<Artifact>> {
    selected_exact(&preferred_speech_config(config)?)
}

fn selected_exact(config: &AppConfig) -> Result<Vec<Artifact>> {
    config.validate()?;
    let platform = target_platform();
    let nemo = uses_nemo_runtime(config);
    if nemo && platform.is_none() {
        return Err(ModelError::Unsupported(format!(
            "{}-{}",
            std::env::consts::OS,
            std::env::consts::ARCH
        )));
    }
    let backend = if config.uses_mlx_speech() {
        Some("metal")
    } else if config.uses_neural_engine() {
        Some(NEURAL_ENGINE_BACKEND)
    } else if nemo {
        Some(nemo_package_backend(config)?)
    } else {
        None
    };
    let engine = if config.uses_nemo_gguf() {
        Some(NEMO_GGUF_ENGINE)
    } else if config.uses_neural_engine() {
        Some(NEURAL_ENGINE_ENGINE)
    } else {
        None
    };
    let quantization = config.rewrite_quantization.artifact_name();
    let mut artifacts: Vec<_> = catalog()
        .into_iter()
        .filter(|file| {
            (config.models.custom_asr.is_none()
                && ((file.role == "asr"
                    && file.model_id.as_deref() == Some(config.models.asr.id())
                    && file.engine.as_deref() == engine)
                    // NeMo runtime packages are shared by every model they run.
                    || (file.role == "asr_runtime" && nemo))
                && file
                    .platform
                    .as_deref()
                    .is_none_or(|value| Some(value) == platform)
                && if config.uses_mlx_speech() {
                    file.backend.as_deref() == Some("metal")
                } else {
                    file.backend
                        .as_deref()
                        .is_none_or(|value| Some(value) == backend)
                })
                || (file.role == "rewriter"
                    && config.cleanup_enabled
                    && config.models.custom_rewriter.is_none()
                    && file.quantization.as_deref() == Some(quantization))
                || (file.role == "audio_frontend"
                    && file
                        .model_id
                        .as_deref()
                        .is_some_and(|id| config.audio_frontend.model_ids().contains(&id)))
        })
        .collect();
    if let Some(profile) = &config.models.custom_asr {
        artifacts.extend(profile_artifacts(profile, "asr"));
    }
    if let Some(profile) = config
        .models
        .custom_rewriter
        .as_ref()
        .filter(|_| config.cleanup_enabled)
    {
        artifacts.extend(profile_artifacts(profile, "rewriter"));
    }
    Ok(artifacts)
}

#[derive(Clone, Debug)]
struct ModelInstallSpec {
    model_id: String,
    name: String,
    role: ModelRole,
    quantization: Option<RewriteQuantization>,
    selected: bool,
    artifacts: Vec<Artifact>,
    support_error: Option<String>,
}

fn built_in_asr_spec(config: &AppConfig, model: AsrModel) -> ModelInstallSpec {
    let mut target = config.clone();
    target.models.asr = model;
    // The engine choice belongs to the model that offers it.
    if !match config.models.asr_engine {
        vesper_core::AsrEngine::Automatic | vesper_core::AsrEngine::Standard => true,
        vesper_core::AsrEngine::UnifiedGguf => vesper_core::nemo_gguf_supported(model),
        vesper_core::AsrEngine::NeuralEngine => vesper_core::neural_engine_supported(model),
    } {
        target.models.asr_engine = vesper_core::AsrEngine::Automatic;
    }
    target.models.custom_asr = None;
    target.models.custom_rewriter = None;
    target.cleanup_enabled = false;
    target.audio_frontend = Default::default();
    // Installation metadata must not inherit a recording's language restriction.
    target.language = if vesper_core::mlx_speech_only(model) {
        Language::English
    } else if vesper_core::mlx_speech_supported(model) {
        config.language
    } else if matches!(
        model,
        AsrModel::CohereTranscribe | AsrModel::Lfm25Audio15b | AsrModel::MoonshineBase
    ) {
        Language::English
    } else {
        Language::Auto
    };
    // Catalog entries must remain independently installable even when the saved
    // backend is incompatible with a different selected model. Auto still uses
    // the current host/platform resolver to choose a real packaged runtime.
    target.compute_backend = if vesper_core::mlx_speech_only(model) {
        ComputeBackend::Auto
    } else if config.compute_backend == ComputeBackend::Metal
        && vesper_core::mlx_speech_supported(model)
    {
        target.language = Language::English;
        ComputeBackend::Metal
    } else if config.compute_backend == ComputeBackend::Cpu {
        ComputeBackend::Cpu
    } else {
        ComputeBackend::Auto
    };
    let (artifacts, support_error) = match selected(&target) {
        Ok(artifacts) if artifacts.is_empty() => (
            Vec::new(),
            Some(format!(
                "no pinned artifacts are available for {}",
                model.id()
            )),
        ),
        Ok(artifacts) => (artifacts, None),
        Err(error) => (Vec::new(), Some(error.to_string())),
    };
    ModelInstallSpec {
        model_id: model.id().into(),
        name: model.name().into(),
        role: ModelRole::SpeechRecognition,
        quantization: None,
        selected: config.models.custom_asr.is_none() && config.models.asr == model,
        artifacts,
        support_error,
    }
}

fn built_in_rewriter_spec(
    config: &AppConfig,
    model: RewriteModel,
    quantization: RewriteQuantization,
) -> ModelInstallSpec {
    let quantization_name = quantization.artifact_name();
    let artifacts = catalog()
        .into_iter()
        .filter(|artifact| {
            artifact.role == "rewriter"
                && artifact.model_id.as_deref() == Some(model.id())
                && artifact.quantization.as_deref() == Some(quantization_name)
        })
        .collect::<Vec<_>>();
    let support_error = artifacts.is_empty().then(|| {
        format!(
            "no pinned {quantization_name} artifacts are available for {}",
            model.id()
        )
    });
    ModelInstallSpec {
        model_id: model.id().into(),
        name: model.name().into(),
        role: ModelRole::TextReconstruction,
        quantization: Some(quantization),
        selected: config.models.custom_rewriter.is_none()
            && config.models.rewriter == model
            && config.rewrite_quantization.resolved() == quantization,
        artifacts,
        support_error,
    }
}

fn custom_model_spec(
    profile: &HuggingFaceProfile,
    role: ModelRole,
    selected: bool,
) -> ModelInstallSpec {
    let artifact_role = match role {
        ModelRole::SpeechRecognition => "asr",
        ModelRole::TextReconstruction => "rewriter",
        ModelRole::AudioFrontend => "audio_frontend",
    };
    let (artifacts, support_error) = match profile.validate() {
        Ok(()) => (profile_artifacts(profile, artifact_role), None),
        Err(error) => (Vec::new(), Some(error.to_string())),
    };
    ModelInstallSpec {
        model_id: profile.id.clone(),
        name: profile.name.clone(),
        role,
        quantization: None,
        selected,
        artifacts,
        support_error,
    }
}

fn model_install_specs(config: &AppConfig) -> Vec<ModelInstallSpec> {
    let mut specs = AsrModel::ALL
        .into_iter()
        .map(|model| built_in_asr_spec(config, model))
        .collect::<Vec<_>>();
    specs.push(ModelInstallSpec {
        model_id: diarization::MODEL_ID.into(),
        name: "Speaker timelines · Nemotron 3".into(),
        role: ModelRole::AudioFrontend,
        quantization: None,
        selected: diarization::supports_backend(config.compute_backend),
        artifacts: vec![diarization::artifact()],
        support_error: (!diarization::supported()).then(|| {
            "Experimental speaker timelines currently require Apple silicon macOS.".into()
        }),
    });
    for (id, name) in [
        ("silero-v6.2", "Speech activity · Silero v6.2"),
        ("dpdfnet4", "Noise reduction · DPDFNet4"),
        ("dpdfnet8", "Noise reduction · DPDFNet8"),
    ] {
        specs.push(ModelInstallSpec {
            model_id: id.into(),
            name: name.into(),
            role: ModelRole::AudioFrontend,
            quantization: None,
            selected: config.audio_frontend.model_ids().contains(&id),
            artifacts: catalog()
                .into_iter()
                .filter(|a| a.role == "audio_frontend" && a.model_id.as_deref() == Some(id))
                .collect(),
            support_error: None,
        });
    }
    for model in RewriteModel::ALL {
        for quantization in [RewriteQuantization::Q4_0, RewriteQuantization::Q8_0] {
            specs.push(built_in_rewriter_spec(config, model, quantization));
        }
    }
    if let Some(profile) = &config.models.custom_asr {
        specs.push(custom_model_spec(
            profile,
            ModelRole::SpeechRecognition,
            true,
        ));
    }
    if let Some(profile) = &config.models.custom_rewriter {
        specs.push(custom_model_spec(
            profile,
            ModelRole::TextReconstruction,
            true,
        ));
    }
    // Keep shipped profile downloads visible after the user selects another model.
    // An imported declaration with the same ID remains authoritative for that ID.
    #[derive(Deserialize)]
    struct Preset {
        profile: HuggingFaceProfile,
    }
    let presets: Vec<Preset> = serde_json::from_str(include_str!("../../../models/presets.json"))
        .expect("checked-in model presets must be valid");
    for Preset { profile } in presets {
        if specs.iter().any(|spec| spec.model_id == profile.id) {
            continue;
        }
        let role = match profile.runtime {
            vesper_core::ModelRuntime::SherpaOnnxTransducer => ModelRole::SpeechRecognition,
            vesper_core::ModelRuntime::LlamaCpp => ModelRole::TextReconstruction,
        };
        specs.push(custom_model_spec(&profile, role, false));
    }
    specs
}

fn profile_artifacts(profile: &HuggingFaceProfile, role: &str) -> Vec<Artifact> {
    profile
        .files
        .iter()
        .map(|file| Artifact {
            name: file.path.clone(),
            relative_path: profile.relative_path(file),
            url: profile.download_url(file),
            revision: profile.revision.clone(),
            sha256: file.sha256.clone(),
            size_bytes: file.size_bytes,
            role: role.into(),
            model_id: None,
            platform: None,
            backend: None,
            archive: None,
            engine: None,
            quantization: None,
            license: profile.license.clone(),
        })
        .collect()
}

/// Keep descendant symlinks from redirecting model reads or installations outside
/// the selected model directory. The root itself may intentionally be a symlink.
fn artifact_path(root: &Path, relative: &str) -> Result<PathBuf> {
    let mut path = root.to_path_buf();
    for component in Path::new(relative).components() {
        if !matches!(component, std::path::Component::Normal(_)) {
            return Err(ModelError::UnsafePath(path));
        }
        path.push(component);
        match path.symlink_metadata() {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(ModelError::UnsafePath(path))
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(path)
}

fn artifact_file_installed(root: &Path, artifact: &Artifact) -> bool {
    artifact_path(root, &artifact.relative_path)
        .ok()
        .and_then(|path| path.metadata().ok())
        .is_some_and(|metadata| metadata.is_file() && metadata.len() == artifact.size_bytes)
}

fn nemo_runtime_ready(root: &Path, artifact: &Artifact) -> bool {
    nemo_speech_executable(root)
        .metadata()
        .is_ok_and(|metadata| metadata.is_file() && metadata.len() > 0)
        && nemo_speech_library(root)
            .metadata()
            .is_ok_and(|metadata| metadata.is_file() && metadata.len() > 0)
        && std::fs::read_to_string(
            root.join("nemo-speech-runtime")
                .join(NEMO_SPEECH_VERSION)
                .join("artifact-name"),
        )
        .is_ok_and(|name| name == artifact.name)
}

fn artifact_installed(root: &Path, artifact: &Artifact) -> bool {
    artifact_file_installed(root, artifact)
        && (artifact.role != "asr_runtime" || nemo_runtime_ready(root, artifact))
}

fn install_runtime_archive(root: &Path, artifact: &Artifact) -> Result<()> {
    let Some(format) = artifact.archive.as_deref() else {
        return Ok(());
    };
    let archive_path = artifact_path(root, &artifact.relative_path)?;
    let runtime_parent = root.join("nemo-speech-runtime");
    std::fs::create_dir_all(&runtime_parent)?;
    let staging = tempfile::Builder::new()
        .prefix(".nemo-speech-extract-")
        .tempdir_in(&runtime_parent)?;

    match format {
        "tar_gz" => {
            let decoder = flate2::read::GzDecoder::new(File::open(&archive_path)?);
            let mut archive = tar::Archive::new(decoder);
            archive
                .unpack(staging.path())
                .map_err(|error| ModelError::InvalidArchive(error.to_string()))?;
        }
        "zip" => {
            let mut archive = zip::ZipArchive::new(File::open(&archive_path)?)
                .map_err(|error| ModelError::InvalidArchive(error.to_string()))?;
            for index in 0..archive.len() {
                let mut entry = archive
                    .by_index(index)
                    .map_err(|error| ModelError::InvalidArchive(error.to_string()))?;
                let relative = entry
                    .enclosed_name()
                    .map(|path| path.to_owned())
                    .ok_or_else(|| {
                        ModelError::InvalidArchive(
                            entry
                                .name()
                                .map(|name| name.into_owned())
                                .unwrap_or_else(|_| "archive entry".into()),
                        )
                    })?;
                let destination = staging.path().join(relative);
                if entry.is_dir() {
                    std::fs::create_dir_all(&destination)?;
                } else {
                    if let Some(parent) = destination.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    let mut output = File::create(&destination)?;
                    std::io::copy(&mut entry, &mut output)?;
                    output.sync_all()?;
                }
            }
        }
        other => return Err(ModelError::InvalidArchive(other.into())),
    }

    let extracted = staging.path().join("nemo-speech");
    let binary = extracted.join("bin").join(if cfg!(windows) {
        "nemo-speech.exe"
    } else {
        "nemo-speech"
    });
    if !binary
        .metadata()
        .is_ok_and(|metadata| metadata.is_file() && metadata.len() > 0)
    {
        return Err(ModelError::InvalidArchive(
            "nemo-speech executable is missing".into(),
        ));
    }
    let library = nemo_speech_library_in_runtime(&extracted);
    if !library
        .metadata()
        .is_ok_and(|metadata| metadata.is_file() && metadata.len() > 0)
    {
        return Err(ModelError::InvalidArchive(
            "NeMo-Speech.cpp C ABI library is missing".into(),
        ));
    }

    let destination = runtime_parent.join(NEMO_SPEECH_VERSION);
    let replacement = tempfile::Builder::new()
        .prefix(".nemo-speech-runtime-")
        .tempdir_in(&runtime_parent)?;
    let replacement_path = replacement.keep();
    std::fs::rename(extracted, replacement_path.join("nemo-speech"))?;
    std::fs::write(replacement_path.join("artifact-name"), &artifact.name)?;
    if destination.exists() {
        let backup = tempfile::Builder::new()
            .prefix(".nemo-speech-previous-")
            .tempdir_in(&runtime_parent)?;
        std::fs::rename(&destination, backup.path().join("runtime"))?;
        std::fs::rename(&replacement_path, &destination)?;
    } else {
        std::fs::rename(&replacement_path, &destination)?;
    }
    Ok(())
}

/// A fast inventory for UI polling. The full hash is always checked before inference.
pub fn inspect(config: &AppConfig, root: &Path) -> Installation {
    let resolved = match resolve_speech(config, root) {
        Ok(selection) => selection,
        Err(error) => {
            return Installation {
                ready: false,
                supported: false,
                files: vec![],
                model_directory: root.display().to_string(),
                message: error.to_string(),
            }
        }
    };
    let config = &resolved.config;
    let selection = selected(config);
    let selection_error = selection.as_ref().err().map(ToString::to_string);
    let supported = selection.is_ok();
    #[cfg(feature = "mlx")]
    let (supported, selection_error) = if config.uses_mlx_runtime() && !mlx::runtime_ready() {
        (
            false,
            Some(
                "The packaged MLX runtime is missing. Rebuild the optional MLX application.".into(),
            ),
        )
    } else {
        (supported, selection_error)
    };
    let artifacts = selection.unwrap_or_default();
    let files: Vec<_> = artifacts
        .iter()
        .map(|file| InstalledFile {
            name: file.name.clone(),
            installed: artifact_installed(root, file),
            size_bytes: file.size_bytes,
        })
        .collect();
    let files_ready = files.iter().all(|file| file.installed);
    let ready = supported
        && artifacts.iter().zip(&files).all(|(artifact, file)| {
            matches!(artifact.role.as_str(), "audio_frontend" | "diarization") || file.installed
        });
    Installation {
        ready,
        model_directory: root.display().to_string(),
        files,
        supported,
        message: if !supported {
            selection_error.unwrap_or_else(|| {
                ModelError::Unsupported(format!(
                    "{}-{}",
                    std::env::consts::OS,
                    std::env::consts::ARCH
                ))
                .to_string()
            })
        } else if ready && !files_ready {
            "Optional audio processing files are unavailable. Transcription will use the original audio.".into()
        } else if ready && resolved.fallback_reason.is_some() {
            "Ready using the installed model. Download the updated package for this device when convenient.".into()
        } else if ready {
            "Model files are installed. Integrity is checked before use.".into()
        } else {
            "Download the selected model files to enable local transcription.".into()
        },
    }
}

fn inspect_model_spec(spec: &ModelInstallSpec, root: &Path) -> ModelInstallation {
    let files = spec
        .artifacts
        .iter()
        .map(|artifact| InstalledFile {
            name: artifact.name.clone(),
            installed: artifact_file_installed(root, artifact),
            size_bytes: artifact.size_bytes,
        })
        .collect::<Vec<_>>();
    let artifacts_installed = !files.is_empty() && files.iter().all(|file| file.installed);
    let runtime_ready = (!spec
        .artifacts
        .iter()
        .any(|a| a.relative_path.starts_with("mlx/"))
        || {
            #[cfg(feature = "mlx")]
            {
                mlx::runtime_ready()
            }
            #[cfg(not(feature = "mlx"))]
            {
                false
            }
        })
        && spec.support_error.is_none()
        && (spec.model_id != diarization::MODEL_ID || diarization::runtime_ready(root))
        && if let Some(runtime) = spec
            .artifacts
            .iter()
            .find(|artifact| artifact.role == "asr_runtime")
        {
            nemo_runtime_ready(root, runtime)
        } else {
            true
        };
    let supported = spec.support_error.is_none();
    let ready = supported && artifacts_installed && runtime_ready;
    let message = if let Some(error) = &spec.support_error {
        error.clone()
    } else if !artifacts_installed {
        "Download this model's pinned files to install it locally.".into()
    } else if !runtime_ready && spec.model_id == diarization::MODEL_ID {
        // Downloading again cannot fix this; the runtime ships inside the app.
        "Speaker model files are installed, but the bundled speaker runtime is missing or incompatible with this Mac. Reinstall Vesper to restore it.".into()
    } else if !runtime_ready {
        "Model files are installed, but the selected native runtime is not ready.".into()
    } else {
        "Model artifacts and runtime are ready. Integrity is checked before use.".into()
    };
    ModelInstallation {
        model_id: spec.model_id.clone(),
        name: spec.name.clone(),
        role: spec.role,
        quantization: spec.quantization,
        selected: spec.selected,
        ready,
        artifacts_installed,
        runtime_ready,
        model_directory: root.display().to_string(),
        files,
        supported,
        message,
        local_file_count: 0,
        downloaded_bytes: 0,
        removable_bytes: 0,
        removal_error: None,
    }
}

/// Fast status for built-ins, shipped presets, and currently configured imported
/// profiles. Cleanup quantizations are independent catalog entries.
pub fn list_model_installations(config: &AppConfig, root: &Path) -> Vec<ModelInstallation> {
    let specs = model_install_specs(config);
    specs
        .iter()
        .map(|spec| {
            let mut status = inspect_model_spec(spec, root);
            if spec.selected && spec.role == ModelRole::SpeechRecognition && !status.ready {
                let mut speech = config.clone();
                speech.cleanup_enabled = false;
                speech.audio_frontend = Default::default();
                let active = inspect(&speech, root);
                if active.ready {
                    status.ready = true;
                    status.message = active.message;
                }
            }
            storage::inspect_storage(&mut status, spec, &specs, root);
            status
        })
        .collect()
}

fn check_cancel(cancelled: &AtomicBool) -> Result<()> {
    if cancelled.load(Ordering::Relaxed) {
        Err(ModelError::Cancelled)
    } else {
        Ok(())
    }
}

fn validate_reader(
    mut reader: impl Read,
    artifact: &Artifact,
    cancelled: &AtomicBool,
) -> Result<()> {
    let mut hash = Sha256::new();
    let mut count = 0u64;
    let mut buffer = [0u8; 128 * 1024];
    loop {
        check_cancel(cancelled)?;
        let bytes = reader.read(&mut buffer)?;
        if bytes == 0 {
            break;
        }
        count += bytes as u64;
        if count > artifact.size_bytes {
            return Err(ModelError::Integrity(artifact.name.clone()));
        }
        hash.update(&buffer[..bytes]);
    }
    if count != artifact.size_bytes || format!("{:x}", hash.finalize()) != artifact.sha256 {
        return Err(ModelError::Integrity(artifact.name.clone()));
    }
    Ok(())
}

fn verify_matching(
    config: &AppConfig,
    root: &Path,
    cancelled: &AtomicBool,
    include: impl Fn(&Artifact) -> bool,
) -> Result<()> {
    for artifact in selected(config)?.into_iter().filter(include) {
        validate_reader(
            File::open(artifact_path(root, &artifact.relative_path)?)?,
            &artifact,
            cancelled,
        )?;
        if artifact.role == "asr_runtime" && !artifact_installed(root, &artifact) {
            return Err(ModelError::Integrity(artifact.name));
        }
    }
    Ok(())
}

/// Verify only the selected speech model and its native runtime.
pub fn verify_asr(config: &AppConfig, root: &Path, cancelled: &AtomicBool) -> Result<()> {
    let resolved = resolve_speech(config, root)?;
    let config = &resolved.config;
    #[cfg(feature = "mlx")]
    if config.uses_mlx_runtime() {
        mlx::verify_runtime(cancelled)?;
        mlx::verify_directory(config, root)?;
    }
    verify_matching(config, root, cancelled, |artifact| {
        matches!(artifact.role.as_str(), "asr" | "asr_runtime")
    })
}

/// Verify only the selected cleanup model. Disabled cleanup has no artifacts.
pub fn verify_rewriter(config: &AppConfig, root: &Path, cancelled: &AtomicBool) -> Result<()> {
    verify_matching(config, root, cancelled, |artifact| {
        artifact.role == "rewriter"
    })
}

pub fn verify_frontend(config: &AppConfig, root: &Path, cancelled: &AtomicBool) -> Result<()> {
    verify_matching(config, root, cancelled, |artifact| {
        artifact.role == "audio_frontend"
    })
}

pub fn verify(config: &AppConfig, root: &Path, cancelled: &AtomicBool) -> Result<()> {
    verify_asr(config, root, cancelled)?;
    verify_frontend(config, root, cancelled)?;
    verify_rewriter(config, root, cancelled)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallPhase {
    Checking,
    Downloading,
    Verifying,
    Installed,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallProgress {
    pub phase: InstallPhase,
    pub file_name: String,
    pub file_index: usize,
    pub file_count: usize,
    pub completed_bytes: u64,
    pub total_bytes: u64,
}

fn install_artifacts(
    root: &Path,
    artifacts: &[Artifact],
    cancelled: &AtomicBool,
    progress: &impl Fn(&InstallProgress),
) -> Result<()> {
    let http = hub_cache::ArtifactHttp::new()?;
    let cache = hub_cache::HubCache::from_env();
    install_artifacts_with_cache(root, artifacts, cancelled, progress, cache.as_ref(), &http)
}

fn install_artifacts_with_cache(
    root: &Path,
    artifacts: &[Artifact],
    cancelled: &AtomicBool,
    progress: &impl Fn(&InstallProgress),
    cache: Option<&hub_cache::HubCache>,
    http: &hub_cache::ArtifactHttp,
) -> Result<()> {
    let total_bytes = artifacts.iter().map(|artifact| artifact.size_bytes).sum();
    let mut completed_bytes = 0;
    for (index, artifact) in artifacts.iter().enumerate() {
        let report = |phase, current| {
            progress(&InstallProgress {
                phase,
                file_name: artifact.name.clone(),
                file_index: index + 1,
                file_count: artifacts.len(),
                completed_bytes: completed_bytes + current,
                total_bytes,
            });
        };
        check_cancel(cancelled)?;
        report(InstallPhase::Checking, 0);
        let target = artifact_path(root, &artifact.relative_path)?;
        if let Ok(existing) = File::open(&target) {
            if validate_reader(existing, artifact, cancelled).is_ok() {
                if artifact.role == "asr_runtime" && !artifact_installed(root, artifact) {
                    install_runtime_archive(root, artifact)?;
                }
                report(InstallPhase::Installed, artifact.size_bytes);
                completed_bytes += artifact.size_bytes;
                continue;
            }
        }
        check_cancel(cancelled)?;
        #[cfg(feature = "mlx")]
        if artifact.url == "builtin:qwen3-tokenizer-v1" {
            mlx::prepare_tokenizer(root, artifact, cancelled)?;
            report(InstallPhase::Installed, artifact.size_bytes);
            completed_bytes += artifact.size_bytes;
            continue;
        }
        let reused = match cache {
            Some(cache) => cache.install(artifact, &target, http, cancelled, &report)?,
            None => false,
        };
        if !reused {
            let parent = target.parent().expect("artifact has a parent");
            std::fs::create_dir_all(parent)?;
            download_artifact(parent, artifact, http, cancelled, &report)?
                .persist(&target)
                .map_err(|e| ModelError::Io(e.error))?;
        }
        if artifact.role == "asr_runtime" {
            check_cancel(cancelled)?;
            install_runtime_archive(root, artifact)?;
        }
        report(InstallPhase::Installed, artifact.size_bytes);
        completed_bytes += artifact.size_bytes;
    }
    #[cfg(feature = "mlx")]
    if artifacts
        .iter()
        .any(|artifact| artifact.engine.as_deref() == Some(NEURAL_ENGINE_ENGINE))
    {
        // Core ML's first load on a Mac takes seconds; do it while installing.
        mlx::prepare_neural_engine(root, cancelled)?;
    }
    Ok(())
}

fn download_artifact(
    parent: &Path,
    artifact: &Artifact,
    http: &hub_cache::ArtifactHttp,
    cancelled: &AtomicBool,
    report: &impl Fn(InstallPhase, u64),
) -> Result<tempfile::NamedTempFile> {
    let mut staging = tempfile::NamedTempFile::new_in(parent)?;
    report(InstallPhase::Downloading, 0);
    let mut response = http.get(artifact)?;
    let mut hash = Sha256::new();
    let mut count = 0u64;
    let mut buffer = [0u8; 128 * 1024];
    loop {
        check_cancel(cancelled)?;
        let bytes = response.read(&mut buffer)?;
        if bytes == 0 {
            break;
        }
        count += bytes as u64;
        if count > artifact.size_bytes {
            return Err(ModelError::Integrity(artifact.name.clone()));
        }
        staging.write_all(&buffer[..bytes])?;
        hash.update(&buffer[..bytes]);
        report(InstallPhase::Downloading, count);
    }
    report(InstallPhase::Verifying, count);
    if count != artifact.size_bytes || format!("{:x}", hash.finalize()) != artifact.sha256 {
        return Err(ModelError::Integrity(artifact.name.clone()));
    }
    check_cancel(cancelled)?;
    staging.as_file().sync_all()?;
    Ok(staging)
}

/// Downloads the currently selected pipeline on explicit request. Each file is
/// staged and verified before replacement.
pub fn install(
    config: &AppConfig,
    root: &Path,
    cancelled: &AtomicBool,
    progress: impl Fn(&InstallProgress),
) -> Result<Installation> {
    let artifacts = selected(config)?;
    install_artifacts(root, &artifacts, cancelled, &progress)?;
    Ok(inspect(config, root))
}

/// Speech files of the engine the selection does not use now, for a model that
/// offers both the Neural Engine and the unified GGUF (#143, #293). The app
/// fetches them in the background after the selected files, so changing the
/// language or device needs no further download. They never count toward
/// readiness, which follows `selected` alone. The Neural Engine set is offered
/// only where the MLX bridge can run it.
pub fn companion_artifacts(config: &AppConfig) -> Result<Vec<Artifact>> {
    let model = config.models.asr;
    if config.models.custom_asr.is_some()
        || !vesper_core::nemo_gguf_supported(model)
        || !vesper_core::neural_engine_supported(model)
    {
        return Ok(Vec::new());
    }
    let mut other = config.clone();
    other.cleanup_enabled = false;
    other.audio_frontend = Default::default();
    if config.uses_neural_engine() {
        other.models.asr_engine = vesper_core::AsrEngine::UnifiedGguf;
        other.language = Language::Auto;
    } else if config.uses_nemo_gguf() {
        if !neural_engine_runnable() {
            return Ok(Vec::new());
        }
        other.models.asr_engine = vesper_core::AsrEngine::NeuralEngine;
        other.language = Language::English;
        other.compute_backend = ComputeBackend::Auto;
    } else {
        return Ok(Vec::new());
    }
    let primary: std::collections::BTreeSet<_> = selected(config)?
        .into_iter()
        .map(|artifact| artifact.relative_path)
        .collect();
    Ok(selected(&other)?
        .into_iter()
        .filter(|artifact| {
            matches!(artifact.role.as_str(), "asr" | "asr_runtime")
                && !primary.contains(&artifact.relative_path)
        })
        .collect())
}

/// The MLX bridge, which runs the Neural Engine engine's decoder, loads here.
fn neural_engine_runnable() -> bool {
    #[cfg(feature = "mlx")]
    return mlx::available().unwrap_or(false);
    #[cfg(not(feature = "mlx"))]
    false
}

/// Download and verify `companion_artifacts`. Already verified files are kept.
pub fn install_companion(
    config: &AppConfig,
    root: &Path,
    cancelled: &AtomicBool,
    progress: impl Fn(&InstallProgress),
) -> Result<usize> {
    let artifacts = companion_artifacts(config)?;
    install_artifacts(root, &artifacts, cancelled, &progress)?;
    Ok(artifacts.len())
}

/// Install one catalog entry without changing the caller's saved selection.
/// Only built-ins, shipped presets, and custom profiles already present in
/// `config` can resolve, so arbitrary repository IDs are never accepted.
pub fn install_model(
    config: &AppConfig,
    root: &Path,
    model_id: &str,
    quantization: Option<RewriteQuantization>,
    cancelled: &AtomicBool,
    progress: impl Fn(&InstallProgress),
) -> Result<ModelInstallation> {
    let specs = model_install_specs(config);
    let candidates = specs
        .iter()
        .filter(|spec| spec.model_id == model_id)
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return Err(ModelError::UnknownModel(model_id.into()));
    }
    let quantization = quantization.map(RewriteQuantization::resolved);
    let matches = candidates
        .iter()
        .copied()
        .filter(|spec| spec.quantization == quantization)
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        let expected = candidates
            .iter()
            .filter_map(|spec| spec.quantization)
            .map(RewriteQuantization::artifact_name)
            .collect::<Vec<_>>();
        let requirement: String = if expected.is_empty() {
            "omit quantization for this model".into()
        } else {
            format!("choose one of these quantizations: {}", expected.join(", "))
        };
        return Err(ModelError::InvalidRequest(requirement));
    }
    let spec = matches[0];
    if let Some(error) = &spec.support_error {
        return Err(ModelError::InvalidRequest(error.clone()));
    }
    install_artifacts(root, &spec.artifacts, cancelled, &progress)?;
    let mut status = inspect_model_spec(spec, root);
    storage::inspect_storage(&mut status, spec, &specs, root);
    Ok(status)
}

pub fn provenance(config: &AppConfig) -> serde_json::Value {
    let preferred = preferred_speech_config(config);
    let effective = preferred.as_ref().unwrap_or(config);
    let asr_runtime = if effective.uses_neural_engine() {
        "Core ML (Neural Engine encoder) + MLX-Audio-Swift"
    } else if effective.uses_mlx_speech() {
        vesper_core::MLX_RUNTIME
    } else if uses_nemo_runtime(config) {
        "NeMo-Speech.cpp 0.1.0"
    } else {
        "sherpa-onnx 1.13.8"
    };
    serde_json::json!({
        "artifacts": selected(config).unwrap_or_default(),
        "customAsr": config.models.custom_asr,
        "customRewriter": if config.cleanup_enabled { config.models.custom_rewriter.as_ref() } else { None },
        "asrRuntime": asr_runtime,
        "selectionError": preferred.as_ref().err().map(ToString::to_string),
        "selectionKind": "preferred-download; actual execution is recorded per request",
        "rewriterRuntime": if config.cleanup_enabled { Some("llama-cpp-2 0.1.159") } else { None },
        "applicationVersion": env!("CARGO_PKG_VERSION")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dedicated_asr_artifacts_are_complete_and_isolated_from_cleanup_quantization() {
        for (model, folder, count) in [
            (AsrModel::Qwen3Asr06b, "qwen3-asr-0.6b/", 6),
            (AsrModel::CohereTranscribe, "cohere-transcribe/", 4),
            (AsrModel::Lfm25Audio15b, "lfm2.5-audio-1.5b/", 2),
            (AsrModel::WhisperSmall, "whisper-small/", 3),
            (AsrModel::WhisperTurbo, "whisper-turbo/", 3),
        ] {
            let mut config = AppConfig {
                compute_backend: ComputeBackend::Cpu,
                ..Default::default()
            };
            config.models.asr = model;
            config.language = Language::English;
            config.rewrite_quantization = RewriteQuantization::Q8_0;
            let all = selected(&config).unwrap();
            let speech: Vec<_> = all.iter().filter(|a| a.role == "asr").collect();
            assert_eq!(speech.len(), count);
            assert_eq!(all.iter().filter(|a| a.role == "rewriter").count(), 1);
            assert!(speech
                .iter()
                .all(|a| a.model_id.as_deref() == Some(model.id())
                    && a.relative_path.starts_with(folder)));
            let spec = built_in_asr_spec(&config, model);
            assert!(spec.support_error.is_none());
            assert_eq!(spec.artifacts.len(), count);
            if model == AsrModel::CohereTranscribe {
                assert!(speech
                    .iter()
                    .any(|a| a.relative_path == "cohere-transcribe/encoder.int8.onnx.data"));
            } else if model == AsrModel::Lfm25Audio15b {
                assert!(speech.iter().any(|a| a.name.starts_with("mmproj-")));
                assert_eq!(
                    speech.iter().map(|a| a.size_bytes).sum::<u64>(),
                    915_262_016
                );
            } else if model.is_whisper() {
                assert!(speech.iter().all(|a| a.license == "MIT"));
                assert_eq!(
                    speech
                        .iter()
                        .filter(|a| a.quantization.as_deref() == Some("int8"))
                        .count(),
                    2
                );
                assert_eq!(
                    speech.iter().map(|a| a.size_bytes).sum::<u64>(),
                    if model == AsrModel::WhisperSmall {
                        375_485_327
                    } else {
                        1_036_613_791
                    }
                );
            } else {
                assert_eq!(
                    speech
                        .iter()
                        .filter(|a| a.relative_path.contains("/tokenizer/"))
                        .count(),
                    3
                );
            }
        }
    }

    #[test]
    fn menu_presets_roundtrip_and_resolve_exact_profile_artifacts() {
        let entries: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("../../../models/presets.json")).unwrap();
        assert!(!entries.is_empty());
        let mut ids = std::collections::HashSet::new();
        for entry in entries {
            let profile: HuggingFaceProfile =
                serde_json::from_value(entry["profile"].clone()).unwrap();
            profile.validate().unwrap();
            assert!(ids.insert(profile.id.clone()));
            let mut config = AppConfig::default();
            match profile.runtime {
                vesper_core::ModelRuntime::SherpaOnnxTransducer => {
                    config.models.custom_asr = Some(profile.clone());
                }
                vesper_core::ModelRuntime::LlamaCpp => {
                    config.models.custom_rewriter = Some(profile.clone());
                }
            }
            config.validate().unwrap();
            let restored: AppConfig =
                serde_json::from_str(&serde_json::to_string(&config).unwrap()).unwrap();
            assert_eq!(restored, config);
            let specs = model_install_specs(&restored);
            let matching: Vec<_> = specs
                .iter()
                .filter(|spec| spec.model_id == profile.id)
                .collect();
            assert_eq!(matching.len(), 1);
            let spec = matching[0];
            assert!(spec.selected);
            assert!(spec.support_error.is_none());
            assert!(spec.quantization.is_none());
            assert_eq!(spec.artifacts.len(), profile.files.len());
            for (artifact, file) in spec.artifacts.iter().zip(&profile.files) {
                assert_eq!(artifact.url, profile.download_url(file));
                assert_eq!(artifact.relative_path, profile.relative_path(file));
                assert_eq!(artifact.sha256, file.sha256);
                assert_eq!(artifact.size_bytes, file.size_bytes);
                assert_eq!(artifact.revision, profile.revision);
            }
        }
    }
    use std::collections::BTreeSet;
    use vesper_core::{HuggingFaceFile, ModelFileRole, ModelRuntime};

    fn speech_profile() -> HuggingFaceProfile {
        HuggingFaceProfile {
            id: "alternate-asr".into(),
            name: "Alternate speech".into(),
            repository: "fixture/speech".into(),
            revision: "a".repeat(40),
            runtime: ModelRuntime::SherpaOnnxTransducer,
            files: [
                (ModelFileRole::Encoder, "export/acoustic.onnx"),
                (ModelFileRole::Decoder, "export/predictor.onnx"),
                (ModelFileRole::Joiner, "export/joint.onnx"),
                (ModelFileRole::Tokens, "export/vocabulary.txt"),
            ]
            .map(|(role, path)| HuggingFaceFile {
                role,
                path: path.into(),
                sha256: format!("{:x}", Sha256::digest(b"audio")),
                size_bytes: 5,
            })
            .to_vec(),
            license: "test-fixture".into(),
            model_type: Some("nemo_transducer".into()),
            languages: vec!["en".into()],
        }
    }

    #[test]
    fn custom_selection_uses_pinned_urls_actual_files_and_export_provenance() {
        let mut config = AppConfig::default();
        config.models.custom_asr = Some(speech_profile());
        config.cleanup_enabled = false;
        let artifacts = selected(&config).unwrap();
        assert_eq!(artifacts.len(), 4);
        let root = tempfile::tempdir().unwrap();
        assert!(!inspect(&config, root.path()).ready);
        for file in &artifacts {
            assert!(file
                .url
                .starts_with("https://huggingface.co/fixture/speech/resolve/"));
            assert!(file.url.contains(&"a".repeat(40)));
            assert!(file
                .relative_path
                .starts_with("huggingface/fixture/speech/"));
            let target = root.path().join(&file.relative_path);
            std::fs::create_dir_all(target.parent().unwrap()).unwrap();
            std::fs::write(target, b"audio").unwrap();
        }
        assert!(inspect(&config, root.path()).ready);
        verify(&config, root.path(), &AtomicBool::new(false)).unwrap();
        let provenance = provenance(&config);
        assert_eq!(provenance["customAsr"]["repository"], "fixture/speech");
        assert_eq!(provenance["artifacts"][0]["sha256"], artifacts[0].sha256);
        assert!(provenance["customRewriter"].is_null());
        std::fs::write(root.path().join(&artifacts[0].relative_path), b"other").unwrap();
        assert!(matches!(
            verify(&config, root.path(), &AtomicBool::new(false)),
            Err(ModelError::Integrity(_))
        ));
    }

    #[test]
    fn missing_optional_frontends_do_not_block_required_inference_readiness() {
        let mut config = AppConfig::default();
        config.models.custom_asr = Some(speech_profile());
        config.cleanup_enabled = false;
        config.audio_frontend.vad_enabled = true;
        config.audio_frontend.noise_reduction = vesper_core::NoiseReduction::Dpdfnet4;
        let root = tempfile::tempdir().unwrap();
        let artifacts = selected(&config).unwrap();
        for artifact in artifacts.iter().filter(|a| a.role == "asr") {
            let target = root.path().join(&artifact.relative_path);
            std::fs::create_dir_all(target.parent().unwrap()).unwrap();
            std::fs::write(target, b"audio").unwrap();
        }
        let status = inspect(&config, root.path());
        assert!(status.ready);
        assert_eq!(status.files.iter().filter(|f| !f.installed).count(), 2);
        assert!(!diarization::available(root.path(), config.compute_backend));
        assert!(status.message.contains("original audio"));
        verify_asr(&config, root.path(), &AtomicBool::new(false)).unwrap();
        assert!(verify_frontend(&config, root.path(), &AtomicBool::new(false)).is_err());
        let required = artifacts.iter().find(|a| a.role == "asr").unwrap();
        std::fs::remove_file(root.path().join(&required.relative_path)).unwrap();
        assert!(!inspect(&config, root.path()).ready);
    }

    #[test]
    fn custom_rewriter_replaces_only_rewriter_and_cleanup_off_omits_it() {
        let mut config = AppConfig::default();
        config.models.asr = AsrModel::Nemotron35AsrStreaming06b;
        let mut profile = speech_profile();
        profile.runtime = ModelRuntime::LlamaCpp;
        profile.model_type = None;
        profile.files.truncate(1);
        profile.files[0].role = ModelFileRole::Weights;
        profile.files[0].path = "different-instruction-model.gguf".into();
        config.models.custom_rewriter = Some(profile);
        let files = selected(&config).unwrap();
        assert_eq!(files.len(), 3);
        assert_eq!(files.iter().filter(|file| file.role == "asr").count(), 1);
        assert_eq!(
            files
                .iter()
                .filter(|file| file.role == "asr_runtime")
                .count(),
            1
        );
        assert_eq!(
            files.last().unwrap().name,
            "different-instruction-model.gguf"
        );
        config.cleanup_enabled = false;
        assert_eq!(selected(&config).unwrap().len(), 2);
    }

    #[cfg(unix)]
    #[test]
    fn descendant_symlinks_cannot_redirect_model_storage() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join("huggingface")).unwrap();
        assert!(matches!(
            artifact_path(root.path(), "huggingface/model/file.gguf"),
            Err(ModelError::UnsafePath(_))
        ));
    }
    #[test]
    fn unified_gguf_selects_one_parakeet_file_plus_the_shared_nemo_runtime() {
        let mut config = AppConfig {
            cleanup_enabled: false,
            ..Default::default()
        };
        config.models.asr = AsrModel::ParakeetTdt06bV3;
        config.models.asr_engine = vesper_core::AsrEngine::UnifiedGguf;
        config.language = Language::German;
        for backend in [ComputeBackend::Cpu, ComputeBackend::Auto] {
            config.compute_backend = backend;
            let files = selected(&config).unwrap();
            let asr: Vec<_> = files.iter().filter(|f| f.role == "asr").collect();
            assert_eq!(asr.len(), 1, "{backend:?}");
            assert_eq!(asr[0].engine.as_deref(), Some(NEMO_GGUF_ENGINE));
            assert!(asr[0].relative_path.ends_with(".q8_0.gguf"));
            let runtime: Vec<_> = files.iter().filter(|f| f.role == "asr_runtime").collect();
            assert_eq!(runtime.len(), 1, "{backend:?}");
            assert_eq!(
                runtime[0].backend.as_deref(),
                Some(nemo_package_backend(&config).unwrap())
            );
            if target_platform() == Some("macos-aarch64") {
                // One Metal package covers CPU and GPU on Apple silicon.
                assert_eq!(runtime[0].backend.as_deref(), Some("metal"));
            }
        }
        // The standard engine never downloads the GGUF, and other models ignore the setting.
        config.models.asr_engine = vesper_core::AsrEngine::Standard;
        config.compute_backend = ComputeBackend::Cpu;
        assert!(selected(&config)
            .unwrap()
            .iter()
            .all(|f| f.engine.is_none() && f.role != "asr_runtime"));
        config.models.asr_engine = vesper_core::AsrEngine::UnifiedGguf;
        let spec = built_in_asr_spec(&config, AsrModel::CohereTranscribe);
        assert!(spec.support_error.is_none(), "{:?}", spec.support_error);
        assert!(spec.artifacts.iter().all(|f| f.engine.is_none()));
    }

    #[test]
    fn companion_files_are_the_other_parakeet_engine_and_never_required() {
        let mut config = AppConfig {
            cleanup_enabled: false,
            ..Default::default()
        };
        config.models.asr = AsrModel::ParakeetTdt06bV3;
        config.models.asr_engine = vesper_core::AsrEngine::UnifiedGguf;
        config.language = Language::German;
        let gguf = selected(&config).unwrap();
        let companion = companion_artifacts(&config).unwrap();
        // Only builds and devices that can run the Neural Engine set get it.
        if vesper_core::neural_engine_supported(config.models.asr) && neural_engine_runnable() {
            assert_eq!(companion.len(), 6);
            assert!(companion
                .iter()
                .all(|a| a.engine.as_deref() == Some(NEURAL_ENGINE_ENGINE)));
            config.models.asr_engine = vesper_core::AsrEngine::NeuralEngine;
            config.language = Language::English;
            config.compute_backend = ComputeBackend::Auto;
            let companion = companion_artifacts(&config).unwrap();
            let paths: Vec<_> = companion.iter().map(|a| a.relative_path.clone()).collect();
            assert!(gguf
                .iter()
                .filter(|a| matches!(a.role.as_str(), "asr" | "asr_runtime"))
                .all(|a| paths.contains(&a.relative_path)));
        } else {
            assert!(companion.is_empty());
        }
        // Other models and the standard engine have no companion.
        config.models.asr_engine = vesper_core::AsrEngine::Standard;
        assert!(companion_artifacts(&config).unwrap().is_empty());
        config.models.asr = AsrModel::Qwen3Asr06b;
        assert!(companion_artifacts(&config).unwrap().is_empty());
    }

    #[test]
    fn pinned_inventory_selects_only_needed_files() {
        let mut config = AppConfig::default();
        assert_eq!(config.models.asr, AsrModel::CohereTranscribe);
        assert_eq!(selected(&config).unwrap().len(), 5);
        config.models.asr = AsrModel::Nemotron35AsrStreaming06b;
        assert_eq!(selected(&config).unwrap().len(), 3);
        config.cleanup_enabled = false;
        assert_eq!(selected(&config).unwrap().len(), 2);
        for artifact in catalog() {
            assert_eq!(artifact.sha256.len(), 64);
            assert_eq!(artifact.revision.len(), 40);
            if artifact.url.contains("huggingface.co") {
                assert!(artifact.url.contains(&artifact.revision));
            }
            assert!(!artifact.relative_path.contains(".."));
        }
        config.models.asr = AsrModel::ParakeetTdt06bV3;
        config.compute_backend = ComputeBackend::Cpu;
        // Automatic: the unified GGUF plus one NeMo runtime package.
        assert_eq!(selected(&config).unwrap().len(), 2);
        assert!(inspect(&config, Path::new("/nonexistent")).supported);
        config.models.asr_engine = vesper_core::AsrEngine::Standard;
        assert_eq!(selected(&config).unwrap().len(), 4);
    }

    #[test]
    fn frontend_installation_is_selected_independently_of_asr() {
        let mut config = AppConfig::default();
        config.audio_frontend.vad_enabled = true;
        config.audio_frontend.noise_reduction = vesper_core::NoiseReduction::Dpdfnet4;
        let selected_frontends: Vec<_> = selected(&config)
            .unwrap()
            .into_iter()
            .filter(|a| a.role == "audio_frontend")
            .collect();
        assert_eq!(selected_frontends.len(), 2);
        assert!(selected_frontends
            .iter()
            .all(|a| a.size_bytes < 200_000_000));
        for spec in model_install_specs(&config) {
            if spec.role == ModelRole::SpeechRecognition {
                assert!(spec.artifacts.iter().all(|a| a.role != "audio_frontend"));
            } else if spec.role == ModelRole::AudioFrontend {
                assert_eq!(spec.artifacts.len(), 1);
                assert_eq!(
                    spec.selected,
                    matches!(spec.model_id.as_str(), "silero-v6.2" | "dpdfnet4")
                        || (spec.model_id == diarization::MODEL_ID
                            && diarization::supports_backend(config.compute_backend))
                );
            }
        }
    }

    #[test]
    fn install_catalog_lists_all_built_ins_and_separates_cleanup_quantizations() {
        let config = AppConfig::default();
        let root = tempfile::tempdir().unwrap();
        let installations = list_model_installations(&config, root.path());
        assert!(
            installations
                .iter()
                .filter(|entry| entry.role == ModelRole::SpeechRecognition)
                .count()
                >= AsrModel::ALL.len()
        );
        assert!(
            installations
                .iter()
                .filter(|entry| entry.role == ModelRole::TextReconstruction)
                .count()
                >= 2
        );
        let cleanup = installations
            .iter()
            .filter(|entry| entry.model_id == RewriteModel::Qwen35_08b.id())
            .collect::<Vec<_>>();
        assert_eq!(cleanup.len(), 2);
        let quantizations = cleanup
            .iter()
            .filter_map(|entry| entry.quantization)
            .collect::<Vec<_>>();
        assert!(quantizations.contains(&RewriteQuantization::Q4_0));
        assert!(quantizations.contains(&RewriteQuantization::Q8_0));
        assert_eq!(
            installations
                .iter()
                .filter(|entry| entry.selected && entry.role == ModelRole::SpeechRecognition)
                .count(),
            1
        );
        assert_eq!(
            installations
                .iter()
                .filter(|entry| entry.selected && entry.role == ModelRole::TextReconstruction)
                .count(),
            1
        );
        let serialized = serde_json::to_value(&installations[0]).unwrap();
        for field in [
            "modelId",
            "name",
            "role",
            "quantization",
            "selected",
            "ready",
            "artifactsInstalled",
            "runtimeReady",
            "modelDirectory",
            "files",
            "supported",
            "message",
            "localFileCount",
            "downloadedBytes",
            "removableBytes",
            "removalError",
        ] {
            assert!(serialized.get(field).is_some(), "missing {field}");
        }
    }

    #[test]
    fn automatic_writer_precision_selects_the_q4_file() {
        let config = AppConfig::default();
        assert_eq!(config.rewrite_quantization, RewriteQuantization::Automatic);
        let writer: Vec<_> = selected(&config)
            .unwrap()
            .into_iter()
            .filter(|a| a.role == "rewriter")
            .collect();
        assert_eq!(writer.len(), 1);
        assert_eq!(writer[0].quantization.as_deref(), Some("q4_0"));
        let selected_specs: Vec<_> = model_install_specs(&config)
            .into_iter()
            .filter(|spec| spec.selected && spec.role == ModelRole::TextReconstruction)
            .map(|spec| spec.quantization)
            .collect();
        assert_eq!(selected_specs, [Some(RewriteQuantization::Q4_0)]);
    }

    #[test]
    fn asr_catalog_status_is_independent_of_the_saved_models_backend_preference() {
        let config = AppConfig {
            compute_backend: ComputeBackend::Vulkan,
            ..AppConfig::default()
        };
        let root = tempfile::tempdir().unwrap();
        let installations = list_model_installations(&config, root.path());
        for model in AsrModel::ALL {
            let status = installations
                .iter()
                .find(|entry| entry.model_id == model.id())
                .unwrap();
            // GPU-only models are installable only where their runtime is compiled.
            assert_eq!(
                status.supported,
                !vesper_core::mlx_speech_only(model) || vesper_core::mlx_speech_supported(model),
                "{}: {}",
                model.id(),
                status.message
            );
        }
    }

    #[test]
    fn zip_runtime_archive_extracts_files_and_rejects_traversal() {
        use std::io::Write;
        let write_zip = |path: &Path, entries: &[(&str, &[u8])]| {
            let mut zip = zip::ZipWriter::new(File::create(path).unwrap());
            for (name, bytes) in entries {
                zip.start_file(*name, zip::write::SimpleFileOptions::default())
                    .unwrap();
                zip.write_all(bytes).unwrap();
            }
            zip.finish().unwrap();
        };
        let artifact = |name: &str| Artifact {
            name: name.into(),
            relative_path: name.into(),
            url: format!("https://example.invalid/{name}"),
            revision: "a".repeat(40),
            sha256: String::new(),
            size_bytes: 0,
            role: "asr_runtime".into(),
            model_id: Some(AsrModel::Nemotron35AsrStreaming06b.id().into()),
            platform: target_platform().map(str::to_owned),
            backend: Some("cpu".into()),
            archive: Some("zip".into()),
            engine: None,
            quantization: None,
            license: "fixture".into(),
        };
        let root = tempfile::tempdir().unwrap();
        let library = nemo_speech_library_in_runtime(Path::new("nemo-speech"));
        let library = library.to_str().unwrap().replace('\\', "/");
        let binary = if cfg!(windows) {
            "nemo-speech/bin/nemo-speech.exe"
        } else {
            "nemo-speech/bin/nemo-speech"
        };
        write_zip(
            &root.path().join("good.zip"),
            &[(binary, b"exe"), (library.as_str(), b"lib")],
        );
        install_runtime_archive(root.path(), &artifact("good.zip")).unwrap();
        let installed = root
            .path()
            .join("nemo-speech-runtime")
            .join(NEMO_SPEECH_VERSION)
            .join("nemo-speech");
        assert_eq!(
            std::fs::read(installed.join(&binary[12..])).unwrap(),
            b"exe"
        );
        write_zip(
            &root.path().join("evil.zip"),
            &[("../escaped", b"x"), (binary, b"exe")],
        );
        assert!(matches!(
            install_runtime_archive(root.path(), &artifact("evil.zip")),
            Err(ModelError::InvalidArchive(_))
        ));
        assert!(!root.path().join("nemo-speech-runtime/escaped").exists());
        assert!(!root.path().join("escaped").exists());
    }

    #[test]
    fn artifact_and_runtime_readiness_are_reported_independently() {
        let root = tempfile::tempdir().unwrap();
        let artifact = Artifact {
            name: "runtime.tar.gz".into(),
            relative_path: "runtime.tar.gz".into(),
            url: "https://example.invalid/runtime.tar.gz".into(),
            revision: "a".repeat(40),
            sha256: format!("{:x}", Sha256::digest(b"archive")),
            size_bytes: 7,
            role: "asr_runtime".into(),
            model_id: Some(AsrModel::Nemotron35AsrStreaming06b.id().into()),
            platform: target_platform().map(str::to_owned),
            backend: Some("cpu".into()),
            archive: Some("tar_gz".into()),
            engine: None,
            quantization: None,
            license: "fixture".into(),
        };
        std::fs::write(root.path().join(&artifact.relative_path), b"archive").unwrap();
        let status = inspect_model_spec(
            &ModelInstallSpec {
                model_id: AsrModel::Nemotron35AsrStreaming06b.id().into(),
                name: "Fixture".into(),
                role: ModelRole::SpeechRecognition,
                quantization: None,
                selected: false,
                artifacts: vec![artifact],
                support_error: None,
            },
            root.path(),
        );
        assert!(status.artifacts_installed);
        assert!(!status.runtime_ready);
        assert!(!status.ready);
    }

    #[test]
    fn one_model_install_accepts_only_catalog_ids_and_preserves_selection() {
        let mut config = AppConfig::default();
        config.models.custom_asr = Some(speech_profile());
        config.cleanup_enabled = false;
        let original = config.clone();
        let root = tempfile::tempdir().unwrap();
        for artifact in profile_artifacts(config.models.custom_asr.as_ref().unwrap(), "asr") {
            let target = root.path().join(artifact.relative_path);
            std::fs::create_dir_all(target.parent().unwrap()).unwrap();
            std::fs::write(target, b"audio").unwrap();
        }
        let status = install_model(
            &config,
            root.path(),
            "alternate-asr",
            None,
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
        assert!(status.ready);
        assert_eq!(config, original);
        assert!(matches!(
            install_model(
                &config,
                root.path(),
                "untrusted/arbitrary-model",
                None,
                &AtomicBool::new(false),
                |_| {},
            ),
            Err(ModelError::UnknownModel(_))
        ));
        assert!(matches!(
            install_model(
                &config,
                root.path(),
                RewriteModel::Qwen35_08b.id(),
                None,
                &AtomicBool::new(false),
                |_| {},
            ),
            Err(ModelError::InvalidRequest(_))
        ));

        let mut colliding = speech_profile();
        colliding.runtime = ModelRuntime::LlamaCpp;
        colliding.model_type = None;
        colliding.files.truncate(1);
        colliding.files[0].role = ModelFileRole::Weights;
        colliding.files[0].path = "cleanup.gguf".into();
        config.models.custom_rewriter = Some(colliding);
        assert!(matches!(
            install_model(
                &config,
                root.path(),
                "alternate-asr",
                None,
                &AtomicBool::new(false),
                |_| {},
            ),
            Err(ModelError::InvalidRequest(_))
        ));
    }

    #[test]
    fn nemotron_runtime_manifest_covers_every_published_desktop_backend() {
        let actual = catalog()
            .into_iter()
            .filter(|artifact| artifact.role == "asr_runtime")
            .map(|artifact| {
                (
                    artifact.platform.expect("runtime platform"),
                    artifact.backend.expect("runtime backend"),
                )
            })
            .collect::<BTreeSet<_>>();
        let expected = [
            ("macos-aarch64", "cpu"),
            ("macos-aarch64", "metal"),
            ("macos-x86_64", "cpu"),
            ("linux-aarch64", "cpu"),
            ("linux-aarch64", "cuda"),
            ("linux-aarch64", "vulkan"),
            ("linux-x86_64", "cpu"),
            ("linux-x86_64", "cuda"),
            ("linux-x86_64", "vulkan"),
            ("windows-x86_64", "cpu"),
            ("windows-x86_64", "cuda"),
            ("windows-x86_64", "vulkan"),
        ]
        .into_iter()
        .map(|(platform, backend)| (platform.to_owned(), backend.to_owned()))
        .collect();
        assert_eq!(actual, expected);
    }

    #[test]
    fn cuda_visibility_disables_automatic_gpu_selection_explicitly() {
        assert!(cuda_is_visible(None));
        assert!(cuda_is_visible(Some(OsStr::new("0"))));
        assert!(cuda_is_visible(Some(OsStr::new("0,2"))));
        assert!(!cuda_is_visible(Some(OsStr::new(""))));
        assert!(!cuda_is_visible(Some(OsStr::new("-1"))));
        assert!(!cuda_is_visible(Some(OsStr::new("none"))));
    }
    #[test]
    fn download_progress_covers_cached_files_verification_corruption_and_cancellation() {
        use std::{
            cell::RefCell,
            io::{BufRead, BufReader},
            net::TcpListener,
            thread,
        };
        for mode in ["success", "corrupt", "cancel"] {
            let root = tempfile::tempdir().unwrap();
            let payload = vec![7_u8; 384 * 1024];
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let served = payload.clone();
            let server = thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut reader = BufReader::new(&mut stream);
                let mut request = String::new();
                while !request.ends_with("\r\n\r\n") {
                    assert!(reader.read_line(&mut request).unwrap() > 0);
                    assert!(request.len() <= 8192);
                }
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    served.len()
                )
                .unwrap();
                // Cancellation can close the connection before all fixture bytes are sent.
                let _ = stream.write_all(&served);
            });
            let make_artifact = |name: &str, bytes: &[u8]| Artifact {
                name: name.into(),
                relative_path: name.into(),
                url: format!("http://{address}/{name}"),
                revision: "synthetic".into(),
                sha256: format!("{:x}", Sha256::digest(bytes)),
                size_bytes: bytes.len() as u64,
                role: "asr".into(),
                model_id: None,
                platform: None,
                backend: None,
                archive: None,
                engine: None,
                quantization: None,
                license: String::new(),
            };
            std::fs::write(root.path().join("cached"), b"audio").unwrap();
            let cached = make_artifact("cached", b"audio");
            let mut next = make_artifact("new-file", &payload);
            if mode == "corrupt" {
                next.sha256 = "0".repeat(64);
            }
            let cancelled = AtomicBool::new(false);
            let samples = RefCell::new(Vec::new());
            let result = install_artifacts(root.path(), &[cached, next], &cancelled, &|progress| {
                samples.borrow_mut().push(progress.clone());
                if mode == "cancel"
                    && progress.phase == InstallPhase::Downloading
                    && progress.completed_bytes > 5
                {
                    cancelled.store(true, Ordering::Relaxed);
                }
            });
            server.join().unwrap();
            let samples = samples.into_inner();
            assert_eq!(samples[0].phase, InstallPhase::Checking);
            assert_eq!(samples[0].completed_bytes, 0);
            assert!(samples
                .iter()
                .all(|sample| sample.total_bytes == payload.len() as u64 + 5
                    && sample.file_count == 2));
            assert!(samples
                .windows(2)
                .all(|pair| pair[0].completed_bytes <= pair[1].completed_bytes));
            assert!(samples
                .iter()
                .any(|sample| sample.phase == InstallPhase::Downloading
                    && sample.completed_bytes == 5
                    && sample.file_index == 2));
            if mode == "success" {
                result.unwrap();
                assert_eq!(
                    std::fs::read(root.path().join("new-file")).unwrap(),
                    payload
                );
                assert_eq!(samples.last().unwrap().phase, InstallPhase::Installed);
                assert!(samples
                    .iter()
                    .any(|sample| sample.phase == InstallPhase::Verifying));
            } else {
                assert!(matches!(
                    (mode, result),
                    ("corrupt", Err(ModelError::Integrity(_)))
                        | ("cancel", Err(ModelError::Cancelled))
                ));
                assert!(!root.path().join("new-file").exists());
                assert!(!samples.iter().any(
                    |sample| sample.file_index == 2 && sample.phase == InstallPhase::Installed
                ));
                assert_eq!(
                    std::fs::read_dir(root.path()).unwrap().count(),
                    1,
                    "temporary download file leaked"
                );
            }
        }
    }

    #[test]
    fn corruption_truncation_and_cancellation_are_rejected() {
        let artifact = Artifact {
            name: "fixture".into(),
            relative_path: "fixture".into(),
            url: String::new(),
            revision: String::new(),
            sha256: format!("{:x}", Sha256::digest(b"audio")),
            size_bytes: 5,
            role: "asr".into(),
            model_id: None,
            platform: None,
            backend: None,
            archive: None,
            engine: None,
            quantization: None,
            license: String::new(),
        };
        let cancelled = AtomicBool::new(false);
        assert!(validate_reader(&b"audio"[..], &artifact, &cancelled).is_ok());
        assert!(validate_reader(&b"other"[..], &artifact, &cancelled).is_err());
        assert!(validate_reader(&b"aud"[..], &artifact, &cancelled).is_err());
        assert!(validate_reader(&b"audio!"[..], &artifact, &cancelled).is_err());
        cancelled.store(true, Ordering::Relaxed);
        assert!(matches!(
            validate_reader(&b"audio"[..], &artifact, &cancelled),
            Err(ModelError::Cancelled)
        ));
    }

    #[test]
    fn artifact_reader_matches_sha256_known_answers_across_read_boundaries() {
        // Published SHA-256 known answers, independent of this build's backend.
        // The million-byte vector crosses seven complete 128 KiB reader buffers.
        for (data, expected) in [
            (
                Vec::new(),
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            ),
            (
                b"abc".to_vec(),
                "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
            ),
            (
                vec![b'a'; 1_000_000],
                "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0",
            ),
        ] {
            let artifact = Artifact {
                name: "known-answer".into(),
                relative_path: "fixture".into(),
                url: String::new(),
                revision: String::new(),
                sha256: expected.into(),
                size_bytes: data.len() as u64,
                role: "asr".into(),
                model_id: None,
                platform: None,
                backend: None,
                archive: None,
                engine: None,
                quantization: None,
                license: String::new(),
            };
            assert!(validate_reader(&data[..], &artifact, &AtomicBool::new(false)).is_ok());
        }
    }
}
