//! One policy for preferred downloads and available, same-model execution.
use super::*;

#[derive(Debug, Clone)]
pub struct SpeechSelection {
    /// Effective speech-only configuration. Never save it as the user's preference
    /// or apply its backend override to the independently selected writer.
    pub config: AppConfig,
    pub requested_backend: ComputeBackend,
    pub preferred_backend: ComputeBackend,
    pub fallback_reason: Option<String>,
}

impl SpeechSelection {
    pub fn evidence(&self) -> serde_json::Value {
        serde_json::json!({
            "requestedBackend": self.requested_backend,
            "preferredBackend": self.preferred_backend,
            "resolvedBackend": self.config.compute_backend,
            "runtime": self.config.speech_adapter().runtime,
            "fallbackReason": self.fallback_reason,
            "artifacts": selected_exact(&self.config).unwrap_or_default()
                .into_iter().filter(|a| matches!(a.role.as_str(), "asr" | "asr_runtime"))
                .collect::<Vec<_>>()
        })
    }

    /// Only callers handling a GPU initialization failure may use this retry.
    /// Cancellation, verification and decoding failures are not retry signals.
    pub fn cpu_fallback(&mut self, root: &Path, reason: String) -> Result<()> {
        if vesper_core::mlx_speech_only(self.config.models.asr) {
            return Err(ModelError::RuntimeUnavailable(format!(
                "Automatic GPU initialization failed ({reason}). This model runs only on the Apple GPU and has no CPU version."
            )));
        }
        if self.requested_backend != ComputeBackend::Auto || !self.config.uses_mlx_speech() {
            return Err(ModelError::InvalidRequest(
                "CPU fallback is unavailable for this selection".into(),
            ));
        }
        let mut cpu = self.config.clone();
        cpu.compute_backend = ComputeBackend::Cpu;
        if !speech_files_present(&cpu, root)? {
            return Err(ModelError::RuntimeUnavailable(format!(
                "Automatic GPU initialization failed ({reason}). CPU model files are not installed; choose CPU and download this model to prepare the fallback."
            )));
        }
        self.config = cpu;
        self.fallback_reason = Some(reason);
        Ok(())
    }
}

fn automatic_candidate(config: &AppConfig) -> bool {
    config.compute_backend == ComputeBackend::Auto
        && config.models.custom_asr.is_none()
        && config.language == Language::English
        && vesper_core::mlx_speech_supported(config.models.asr)
        // The unified GGUF picks its own device through NeMo-Speech.cpp, and the
        // Neural Engine engine has one execution path.
        && !config.uses_nemo_gguf()
        && !config.uses_neural_engine()
}

fn preferred_with_availability(config: &AppConfig, available: bool) -> AppConfig {
    let mut preferred = config.clone();
    // A model without a CPU export has one execution path: Automatic means GPU,
    // and an unusable GPU surfaces as a load error rather than another model.
    if (automatic_candidate(config) && available)
        || (config.compute_backend == ComputeBackend::Auto && config.uses_mlx_speech())
    {
        preferred.compute_backend = ComputeBackend::Metal;
    }
    preferred
}

/// Select pinned files for an explicit install/repair, independent of which
/// export is already installed. Hardware discovery never loads model weights.
pub fn preferred_speech_config(config: &AppConfig) -> Result<AppConfig> {
    #[cfg(feature = "mlx")]
    let available = automatic_candidate(config) && mlx::available()?;
    #[cfg(not(feature = "mlx"))]
    let available = false;
    Ok(preferred_with_availability(config, available))
}

fn speech_files_present(config: &AppConfig, root: &Path) -> Result<bool> {
    let files: Vec<_> = selected_exact(config)?
        .into_iter()
        .filter(|a| matches!(a.role.as_str(), "asr" | "asr_runtime"))
        .collect();
    let mut complete = !files.is_empty();
    for artifact in files {
        let path = artifact_path(root, &artifact.relative_path)?;
        match path.metadata() {
            Ok(metadata) if metadata.is_file() && metadata.len() == artifact.size_bytes => {}
            Ok(_) => {
                return Err(ModelError::Integrity(format!(
                    "invalid size for {}",
                    artifact.name
                )))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => complete = false,
            Err(error) => return Err(error.into()),
        }
    }
    Ok(complete)
}

fn resolve_with_preferred(
    config: &AppConfig,
    preferred: AppConfig,
    root: &Path,
) -> Result<SpeechSelection> {
    config.validate()?;
    preferred.validate()?;
    let mut selection = SpeechSelection {
        requested_backend: config.compute_backend,
        preferred_backend: preferred.compute_backend,
        config: preferred,
        fallback_reason: None,
    };
    if config.compute_backend == ComputeBackend::Auto
        && selection.config.uses_mlx_speech()
        && !vesper_core::mlx_speech_only(config.models.asr)
        && !speech_files_present(&selection.config, root)?
    {
        let mut cpu = config.clone();
        cpu.compute_backend = ComputeBackend::Cpu;
        if speech_files_present(&cpu, root)? {
            selection.config = cpu;
            selection.fallback_reason = Some(
                "Optimized model files are not installed; using the existing CPU model.".into(),
            );
        }
    }
    // Automatic prefers the unified GGUF; until it is installed, keep using the
    // model's existing standard files rather than failing.
    if config.models.asr_engine == vesper_core::AsrEngine::Automatic
        && selection.config.uses_nemo_gguf()
        && !speech_files_present(&selection.config, root)?
    {
        let mut standard = config.clone();
        standard.models.asr_engine = vesper_core::AsrEngine::Standard;
        let standard = preferred_speech_config(&standard)?;
        let mut cpu = standard.clone();
        cpu.compute_backend = ComputeBackend::Cpu;
        let installed = [standard, cpu]
            .into_iter()
            .filter(|candidate| candidate.validate().is_ok())
            .find_map(|candidate| match speech_files_present(&candidate, root) {
                Ok(true) => Some(Ok(candidate)),
                Ok(false) => None,
                Err(error) => Some(Err(error)),
            })
            .transpose()?;
        if let Some(standard) = installed {
            selection.config = standard;
            selection.fallback_reason = Some(
                "The optimized model file is not installed yet; using the existing model files."
                    .into(),
            );
        }
    }
    #[cfg(feature = "mlx")]
    if selection.fallback_reason.is_none()
        && automatic_candidate(config)
        && selection.config.compute_backend == ComputeBackend::Cpu
    {
        if let Some(reason) = mlx::load_failure() {
            selection.fallback_reason = Some(format!(
                "The GPU runtime could not be loaded ({reason}); using the CPU model."
            ));
        }
    }
    Ok(selection)
}

/// Fast file readiness determines the available path; integrity remains mandatory
/// before native use. This must not download, hash weights, or change preferences.
pub fn resolve_speech(config: &AppConfig, root: &Path) -> Result<SpeechSelection> {
    resolve_with_preferred(config, preferred_speech_config(config)?, root)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apple_gpu_only_automatic_is_metal_without_a_cpu_fallback() {
        let mut config = AppConfig::default();
        config.models.asr = AsrModel::GraniteSpeech5TurboCtc;
        config.compute_backend = ComputeBackend::Auto;
        for language in [Language::English, Language::German, Language::Japanese] {
            config.language = language;
            for available in [true, false] {
                let expected = if vesper_core::mlx_speech_supported(config.models.asr) {
                    ComputeBackend::Metal
                } else {
                    ComputeBackend::Auto
                };
                assert_eq!(
                    preferred_with_availability(&config, available).compute_backend,
                    expected
                );
            }
        }
        if vesper_core::mlx_speech_supported(config.models.asr) {
            let root = tempfile::tempdir().unwrap();
            let mut selection = resolve_with_preferred(
                &config,
                preferred_with_availability(&config, false),
                root.path(),
            )
            .unwrap();
            assert_eq!(selection.config.compute_backend, ComputeBackend::Metal);
            assert!(selection.fallback_reason.is_none());
            let error = selection
                .cpu_fallback(root.path(), "fixture".into())
                .unwrap_err();
            assert!(error.to_string().contains("no CPU version"), "{error}");
        }
    }

    #[test]
    fn unified_gguf_automatic_is_left_to_the_nemo_runtime() {
        let mut config = AppConfig::default();
        config.models.asr = AsrModel::ParakeetTdt06bV3;
        config.models.asr_engine = vesper_core::AsrEngine::UnifiedGguf;
        config.compute_backend = ComputeBackend::Auto;
        config.language = Language::English;
        for available in [false, true] {
            let preferred = preferred_with_availability(&config, available);
            assert_eq!(preferred.compute_backend, ComputeBackend::Auto);
            assert!(!preferred.uses_mlx_speech());
        }
    }

    #[test]
    fn automatic_preserves_language_model_and_manual_overrides() {
        let mut config = AppConfig::default();
        config.models.asr = AsrModel::Qwen3Asr06b;
        for backend in [
            ComputeBackend::Auto,
            ComputeBackend::Cpu,
            ComputeBackend::Metal,
            ComputeBackend::Cuda,
        ] {
            config.compute_backend = backend;
            for language in [
                Language::English,
                Language::Auto,
                Language::French,
                Language::Hindi,
            ] {
                config.language = language;
                for available in [true, false] {
                    let got = preferred_with_availability(&config, available);
                    let expected = if backend == ComputeBackend::Auto
                        && language == Language::English
                        && available
                        && vesper_core::mlx_speech_supported(config.models.asr)
                    {
                        ComputeBackend::Metal
                    } else {
                        backend
                    };
                    assert_eq!(got.compute_backend, expected);
                    let mut restored = got;
                    restored.compute_backend = config.compute_backend;
                    assert_eq!(restored, config);
                }
            }
        }
        for model in [
            AsrModel::CohereTranscribe,
            AsrModel::WhisperTurbo,
            AsrModel::Lfm25Audio15b,
            AsrModel::Nemotron35AsrStreaming06b,
        ] {
            config.models.asr = model;
            config.compute_backend = ComputeBackend::Auto;
            config.language = Language::English;
            assert_eq!(preferred_with_availability(&config, true), config);
        }
        let presets: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("../../../models/presets.json")).unwrap();
        config.models.asr = AsrModel::Qwen3Asr06b;
        config.models.custom_asr = presets
            .into_iter()
            .map(|p| serde_json::from_value::<HuggingFaceProfile>(p["profile"].clone()).unwrap())
            .find(|p| p.runtime == vesper_core::ModelRuntime::SherpaOnnxTransducer);
        assert!(config.models.custom_asr.is_some());
        assert_eq!(preferred_with_availability(&config, true), config);
    }

    #[test]
    fn automatic_keeps_installed_standard_files_until_the_gguf_is_installed() {
        let root = tempfile::tempdir().unwrap();
        let mut config = AppConfig::default();
        config.models.asr = AsrModel::ParakeetTdt06bV3;
        config.compute_backend = ComputeBackend::Cpu;
        config.language = Language::German;
        let preferred = preferred_with_availability(&config, false);
        let install = |config: &AppConfig| {
            for artifact in selected_exact(config)
                .unwrap()
                .into_iter()
                .filter(|a| matches!(a.role.as_str(), "asr" | "asr_runtime"))
            {
                let path = root.path().join(&artifact.relative_path);
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::File::create(path)
                    .unwrap()
                    .set_len(artifact.size_bytes)
                    .unwrap();
            }
        };
        // Nothing installed: the GGUF stays the download target.
        let chosen = resolve_with_preferred(&config, preferred.clone(), root.path()).unwrap();
        assert!(chosen.config.uses_nemo_gguf());
        assert!(chosen.fallback_reason.is_none());
        // Only the standard files are installed: keep using them.
        let mut standard = config.clone();
        standard.models.asr_engine = vesper_core::AsrEngine::Standard;
        install(&standard);
        let chosen = resolve_with_preferred(&config, preferred.clone(), root.path()).unwrap();
        assert!(!chosen.config.uses_nemo_gguf());
        assert_eq!(
            chosen.config.models.asr_engine,
            vesper_core::AsrEngine::Standard
        );
        assert!(chosen.fallback_reason.is_some());
        // Once the GGUF and its runtime package are installed, Automatic uses them.
        install(&preferred);
        let chosen = resolve_with_preferred(&config, preferred, root.path()).unwrap();
        assert!(chosen.config.uses_nemo_gguf());
        assert!(chosen.fallback_reason.is_none());
    }

    #[test]
    #[cfg(feature = "mlx")]
    fn installed_export_selection_and_download_preference_stay_distinct() {
        let root = tempfile::tempdir().unwrap();
        let mut config = AppConfig::default();
        config.models.asr = AsrModel::Qwen3Asr06b;
        let preferred = preferred_with_availability(&config, true);
        assert!(preferred.uses_mlx_speech());
        assert!(
            resolve_with_preferred(&config, preferred.clone(), root.path())
                .unwrap()
                .config
                .uses_mlx_speech()
        );
        let mut cpu = config.clone();
        cpu.compute_backend = ComputeBackend::Cpu;
        let create_files = |config: &AppConfig| {
            for artifact in selected_exact(config)
                .unwrap()
                .into_iter()
                .filter(|a| a.role == "asr")
            {
                let path = root.path().join(&artifact.relative_path);
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                File::create(path)
                    .unwrap()
                    .set_len(artifact.size_bytes)
                    .unwrap();
            }
        };
        create_files(&cpu);
        let chosen = resolve_with_preferred(&config, preferred.clone(), root.path()).unwrap();
        assert_eq!(chosen.config.compute_backend, ComputeBackend::Cpu);
        assert_eq!(chosen.preferred_backend, ComputeBackend::Metal);
        assert!(chosen.fallback_reason.is_some());
        create_files(&preferred);
        let mut chosen = resolve_with_preferred(&config, preferred.clone(), root.path()).unwrap();
        assert!(chosen.config.uses_mlx_speech());
        chosen
            .cpu_fallback(root.path(), "fixture initialization failure".into())
            .unwrap();
        assert_eq!(chosen.config.compute_backend, ComputeBackend::Cpu);
        // Readiness never substitutes for hash verification (these are sparse placeholders).
        assert!(verify_asr(&chosen.config, root.path(), &AtomicBool::new(false)).is_err());
        for a in selected_exact(&cpu)
            .unwrap()
            .into_iter()
            .filter(|a| a.role == "asr")
        {
            std::fs::remove_file(root.path().join(a.relative_path)).unwrap();
        }
        assert!(
            resolve_with_preferred(&config, preferred.clone(), root.path())
                .unwrap()
                .config
                .uses_mlx_speech()
        );
        let mut explicit = config.clone();
        explicit.compute_backend = ComputeBackend::Metal;
        assert!(resolve_with_preferred(&explicit, preferred, root.path())
            .unwrap()
            .cpu_fallback(root.path(), "fixture".into())
            .is_err());
    }
}
