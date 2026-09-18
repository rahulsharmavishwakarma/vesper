use std::{fs, io::Write, path::PathBuf, sync::Mutex};

use transcribe_core::AppConfig;

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
                return Ok(AppConfig::default());
            }
            Err(error) => return Err(format!("Could not read settings: {error}")),
        };
        let settings: AppConfig = serde_json::from_slice(&bytes)
            .map_err(|e| format!("Saved settings could not be decoded: {e}"))?;
        settings.validate().map_err(|e| e.to_string())?;
        Ok(settings)
    }

    /// Keep startup available for explicit recovery. Normal settings reads stay
    /// strict, and this fallback never writes preferences or configures inference.
    pub fn startup_config(&self) -> AppConfig {
        self.load().unwrap_or_default()
    }

    pub fn save(&self, settings: &AppConfig) -> Result<AppConfig, String> {
        settings.validate().map_err(|e| e.to_string())?;
        let _guard = self.access.lock().map_err(|e| e.to_string())?;
        let parent = self.path.parent().ok_or("Settings path has no parent")?;
        fs::create_dir_all(parent)
            .map_err(|e| format!("Could not create settings directory: {e}"))?;
        let bytes = serde_json::to_vec_pretty(settings).map_err(|e| e.to_string())?;
        let mut staged = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
        staged.write_all(&bytes).map_err(|e| e.to_string())?;
        staged.as_file().sync_all().map_err(|e| e.to_string())?;
        staged
            .persist(&self.path)
            .map_err(|e| format!("Could not save settings: {e}"))?;
        Ok(settings.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use transcribe_core::{AsrModel, Language};

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
    fn corrupted_preferences_are_reported_instead_of_silently_replaced() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("settings.json");
        fs::write(&path, "broken JSON").unwrap();
        let store = SettingsStore::new(path);
        assert!(store.load().is_err());
        assert_eq!(fs::read_to_string(&store.path).unwrap(), "broken JSON");
        store.save(&AppConfig::default()).unwrap();
        assert_eq!(store.load().unwrap(), AppConfig::default());
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
}
