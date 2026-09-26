use std::{fs, io::Write, path::PathBuf, sync::Mutex};

use vesper_core::{AppConfig, AudioFrontendConfig, NoiseReduction};

/// Desktop-owned audio selection; evaluator profiles remain independently configurable.
pub(crate) fn managed_audio(mut settings: AppConfig) -> AppConfig {
    settings.audio_frontend = AudioFrontendConfig {
        vad_enabled: true,
        auto_level_enabled: false,
        noise_reduction: NoiseReduction::Dpdfnet8,
    };
    settings
}

/// Serialize reads and atomic replacements of the local preferences file.
pub struct SettingsStore {
    path: PathBuf,
    access: Mutex<()>,
}

impl SettingsStore {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            access: Mutex::new(()),
        }
    }

    pub fn load(&self) -> Result<AppConfig, String> {
        let _guard = self.access.lock().map_err(|e| e.to_string())?;
        let bytes = match fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(managed_audio(AppConfig::default()));
            }
            Err(error) => return Err(format!("Could not read settings: {error}")),
        };
        let settings: AppConfig = serde_json::from_slice(&bytes)
            .map_err(|e| format!("Saved settings could not be decoded: {e}"))?;
        settings.validate().map_err(|e| e.to_string())?;
        Ok(managed_audio(settings))
    }

    /// Keep startup available for explicit recovery. Normal settings reads stay
    /// strict, and this fallback never writes preferences or configures inference.
    pub fn startup_config(&self) -> AppConfig {
        self.load().unwrap_or_default()
    }

    pub fn save(&self, settings: &AppConfig) -> Result<AppConfig, String> {
        settings.validate().map_err(|e| e.to_string())?;
        settings
            .validate_writer_language()
            .map_err(|e| e.to_string())?;
        let settings = managed_audio(settings.clone());
        let _guard = self.access.lock().map_err(|e| e.to_string())?;
        let parent = self.path.parent().ok_or("Settings path has no parent")?;
        fs::create_dir_all(parent)
            .map_err(|e| format!("Could not create settings directory: {e}"))?;
        let bytes = serde_json::to_vec_pretty(&settings).map_err(|e| e.to_string())?;
        let mut staged = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
        staged.write_all(&bytes).map_err(|e| e.to_string())?;
        staged.as_file().sync_all().map_err(|e| e.to_string())?;
        staged
            .persist(&self.path)
            .map_err(|e| format!("Could not save settings: {e}"))?;
        Ok(settings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vesper_core::{AsrModel, Language};

    #[test]
    fn preferences_survive_reopening_and_invalid_changes_preserve_saved_data() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("config/settings.json");
        let store = SettingsStore::new(path.clone());
        let mut settings = store.load().unwrap();
        settings.models.asr = AsrModel::Nemotron35AsrStreaming06b;
        settings.language = Language::Hindi;
        store.save(&settings).unwrap();
        assert_eq!(SettingsStore::new(path).load().unwrap(), settings);
        let before = fs::read(&store.path).unwrap();
        settings.models.asr = AsrModel::ParakeetTdt06bV3;
        assert!(store.save(&settings).is_err());
        assert_eq!(fs::read(&store.path).unwrap(), before);
    }

    #[test]
    fn legacy_writer_languages_are_readable_but_invalid_saves_preserve_the_file() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("settings.json");
        let store = SettingsStore::new(path.clone());
        let presets: serde_json::Value =
            serde_json::from_str(include_str!("../../../../models/presets.json")).unwrap();
        let preset = presets
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["profile"]["runtime"] == "llama_cpp")
            .unwrap();
        let profile = serde_json::from_value(preset["profile"].clone()).unwrap();
        let mut config = AppConfig::default();
        config.models.custom_rewriter = Some(profile);
        for languages in [vec![], vec!["hi".into()]] {
            config.models.custom_rewriter.as_mut().unwrap().languages = languages;
            let bytes = serde_json::to_vec(&config).unwrap();
            fs::write(&path, &bytes).unwrap();
            assert_eq!(store.load().unwrap().models, config.models);
            assert!(store.save(&config).is_err());
            assert_eq!(fs::read(&path).unwrap(), bytes);
        }
        config.cleanup_enabled = false;
        store.save(&config).unwrap();
        assert!(!store.load().unwrap().cleanup_enabled);
        config.cleanup_enabled = true;
        config.models.custom_rewriter.as_mut().unwrap().languages = vec!["en-US".into()];
        store.save(&config).unwrap();
        assert_eq!(store.load().unwrap().models, config.models);
    }

    #[test]
    fn corrupted_preferences_are_reported_instead_of_silently_replaced() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("settings.json");
        fs::write(&path, "broken JSON").unwrap();
        let store = SettingsStore::new(path);
        assert!(store.load().is_err());
        assert_eq!(fs::read_to_string(&store.path).unwrap(), "broken JSON");
        store.save(&AppConfig::default()).unwrap();
        assert_eq!(store.load().unwrap(), managed_audio(AppConfig::default()));
    }

    #[test]
    fn startup_preserves_corrupt_and_invalid_settings_until_explicit_recovery() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("settings.json");
        let store = SettingsStore::new(path.clone());
        let invalid = AppConfig {
            schema_version: 0,
            ..AppConfig::default()
        };
        for bytes in [
            b"broken JSON".to_vec(),
            serde_json::to_vec(&invalid).unwrap(),
        ] {
            fs::write(&path, &bytes).unwrap();
            assert_eq!(
                store.startup_config().global_shortcut,
                AppConfig::default().global_shortcut
            );
            assert!(store.load().is_err());
            assert_eq!(fs::read(&path).unwrap(), bytes);
        }
    }

    #[test]
    fn startup_uses_valid_custom_shortcut() {
        let temp = tempfile::tempdir().unwrap();
        let store = SettingsStore::new(temp.path().join("settings.json"));
        let settings = AppConfig {
            global_shortcut: "CommandOrControl+Shift+D".into(),
            ..AppConfig::default()
        };
        store.save(&settings).unwrap();
        assert_eq!(
            store.startup_config().global_shortcut,
            settings.global_shortcut
        );
    }

    #[test]
    fn managed_audio_migrates_old_preferences_without_rewriting_on_read() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("settings.json");
        let store = SettingsStore::new(path.clone());
        for noise_reduction in [
            NoiseReduction::Off,
            NoiseReduction::Dpdfnet4,
            NoiseReduction::Dpdfnet8,
        ] {
            let old = AppConfig {
                cleanup_enabled: false,
                audio_frontend: AudioFrontendConfig {
                    vad_enabled: false,
                    auto_level_enabled: true,
                    noise_reduction,
                },
                ..AppConfig::default()
            };
            let bytes = serde_json::to_vec(&old).unwrap();
            fs::write(&path, &bytes).unwrap();
            let expected = managed_audio(old.clone());
            assert_eq!(store.load().unwrap(), expected);
            assert_eq!(store.startup_config(), expected);
            assert_eq!(fs::read(&path).unwrap(), bytes);
            assert_eq!(store.save(&old).unwrap(), expected);
            assert_eq!(store.load().unwrap(), expected);
        }
        // Changing desktop policy must not contaminate raw evaluator baselines.
        assert_eq!(
            AppConfig::default().audio_frontend,
            AudioFrontendConfig::default()
        );
    }

    #[test]
    #[ignore = "downloads pinned audio models; requires VESPER_SMOKE_AUDIO public fixture"]
    fn managed_audio_downloads_verify_and_reuse_pinned_packages() {
        use std::{cell::Cell, sync::atomic::AtomicBool};
        use vesper_models::{install_model, list_model_installations, InstallPhase};
        let temp = tempfile::tempdir().unwrap();
        let store = SettingsStore::new(temp.path().join("settings.json"));
        let config = store.load().unwrap();
        let root = temp.path().join("models");
        let cancelled = AtomicBool::new(false);
        assert_eq!(
            config.audio_frontend.model_ids(),
            ["silero-v6.2", "dpdfnet8"]
        );
        for id in ["dpdfnet8", "silero-v6.2"] {
            let downloaded = Cell::new(false);
            let installed = install_model(&config, &root, id, None, &cancelled, |progress| {
                if progress.phase == InstallPhase::Downloading {
                    downloaded.set(true);
                }
            })
            .unwrap();
            assert!(
                downloaded.get()
                    && installed.selected
                    && installed.ready
                    && installed.artifacts_installed
            );
            let reused = install_model(&config, &root, id, None, &cancelled, |progress| {
                assert_ne!(
                    progress.phase,
                    InstallPhase::Downloading,
                    "verified files must be reused"
                );
            })
            .unwrap();
            assert!(reused.ready);
        }
        // Size-only inventory still considers this file installed. Automatic
        // verification must repair it before it can be used by the frontend.
        let weights = root.join("audio-frontends/dpdfnet8.onnx");
        let mut damaged = fs::read(&weights).unwrap();
        damaged[128] ^= 0xff;
        fs::write(&weights, damaged).unwrap();
        let repaired = Cell::new(false);
        let result = install_model(&config, &root, "dpdfnet8", None, &cancelled, |progress| {
            if progress.phase == InstallPhase::Downloading {
                repaired.set(true);
            }
        })
        .unwrap();
        assert!(repaired.get() && result.ready);
        assert!(list_model_installations(&config, &root)
            .iter()
            .filter(|entry| entry.artifacts_installed)
            .all(|entry| matches!(entry.model_id.as_str(), "dpdfnet8" | "silero-v6.2")));
        assert!(!root.join("audio-frontends/dpdfnet4.onnx").exists());
        assert!(
            !store.path.exists(),
            "automatic setup must not write settings"
        );
        let fixture =
            vesper_core::environment::var_os("VESPER_SMOKE_AUDIO").expect("public audio fixture");
        let audio = vesper_audio::decode_file(std::path::Path::new(&fixture), 60).unwrap();
        let mut cache = vesper_inference::audio_frontend::FrontendCache::default();
        let (output, metrics) = vesper_inference::audio_frontend::process_completed(
            &mut cache, &audio, &config, &root, &cancelled,
        )
        .unwrap();
        assert_eq!(metrics.settings, config.audio_frontend);
        assert!(metrics.finalized && metrics.automatic_level.is_none());
        assert_eq!(output.samples().len(), metrics.input_samples_16k);
        assert!(
            !output.samples().is_empty()
                && output.samples().iter().all(|sample| sample.is_finite())
        );
    }
}
