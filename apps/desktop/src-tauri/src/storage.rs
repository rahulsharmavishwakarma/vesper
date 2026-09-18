//! Preserve access to data from earlier product names without moving user files.
use std::{
    ffi::OsString,
    fs, io,
    path::{Path, PathBuf},
};

const TRANSCRIBE_IDENTIFIER: &str = "com.transcribe.desktop";
const UKTI_IDENTIFIER: &str = "com.ukti.desktop";
const STT_IDENTIFIER: &str = "com.stt.desktop";

/// An existing transcribe store wins. Otherwise keep using the newest sibling
/// store with app state. Config and data share a root on macOS.
pub fn compatible_app_root(current: PathBuf) -> io::Result<PathBuf> {
    if current
        .file_name()
        .is_none_or(|name| name != TRANSCRIBE_IDENTIFIER)
        || contains_app_state(&current)?
    {
        return Ok(current);
    }
    for identifier in [UKTI_IDENTIFIER, STT_IDENTIFIER] {
        let legacy = current.with_file_name(identifier);
        if contains_app_state(&legacy)? {
            return Ok(legacy);
        }
    }
    Ok(current)
}

fn contains_app_state(root: &Path) -> io::Result<bool> {
    // Even a malformed or dangling settings entry is explicit state: report its
    // normal load error instead of quietly loading another installation.
    match root.join("settings.json").symlink_metadata() {
        Ok(_) => return Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    for directory in ["dataset", "models"] {
        if contains_stored_entry(&root.join(directory))? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Empty scaffolding is not evidence of a new store. Never follow symlinks,
/// which could leave the app root or form cycles; their presence is state.
fn contains_stored_entry(root: &Path) -> io::Result<bool> {
    let metadata = match root.symlink_metadata() {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    };
    if !metadata.is_dir() {
        return Ok(true);
    }
    let mut directories = vec![root.to_path_buf()];
    while let Some(directory) = directories.pop() {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            if entry.file_name() == ".DS_Store" {
                continue;
            }
            if entry.file_type()?.is_dir() {
                directories.push(entry.path());
            } else {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

pub fn model_root(
    app_data_root: &Path,
    preferred_override: Option<OsString>,
    prior_override: Option<OsString>,
    legacy_override: Option<OsString>,
) -> PathBuf {
    preferred_override
        .filter(|value| !value.is_empty())
        .or_else(|| prior_override.filter(|value| !value.is_empty()))
        .or_else(|| legacy_override.filter(|value| !value.is_empty()))
        .map(PathBuf::from)
        .unwrap_or_else(|| app_data_root.join("models"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_install_uses_transcribe_without_creating_directories() {
        let temp = tempfile::tempdir().unwrap();
        let current = temp.path().join(TRANSCRIBE_IDENTIFIER);
        assert_eq!(compatible_app_root(current.clone()).unwrap(), current);
        assert!(!current.exists());
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 0);
    }

    #[test]
    fn legacy_settings_dataset_and_models_remain_in_place() {
        let temp = tempfile::tempdir().unwrap();
        let current = temp.path().join(TRANSCRIBE_IDENTIFIER);
        let legacy = temp.path().join(UKTI_IDENTIFIER);
        let settings = crate::settings::SettingsStore::new(legacy.join("settings.json"));
        let config = transcribe_core::AppConfig {
            max_recording_seconds: 42,
            ..Default::default()
        };
        settings.save(&config).unwrap();
        fs::create_dir_all(legacy.join("dataset/audio")).unwrap();
        fs::create_dir_all(legacy.join("models/example")).unwrap();
        fs::write(legacy.join("dataset/audio/original.wav"), b"original audio").unwrap();
        fs::write(legacy.join("models/example/weights.gguf"), b"weights").unwrap();
        let selected = compatible_app_root(current.clone()).unwrap();
        assert_eq!(selected, legacy);
        assert_eq!(
            crate::settings::SettingsStore::new(selected.join("settings.json"))
                .load()
                .unwrap(),
            config
        );
        assert_eq!(
            fs::read(selected.join("dataset/audio/original.wav")).unwrap(),
            b"original audio"
        );
        assert_eq!(
            fs::read(model_root(&selected, None, None, None).join("example/weights.gguf")).unwrap(),
            b"weights"
        );
        assert!(!current.exists());
    }

    #[test]
    fn empty_new_scaffolding_does_not_hide_legacy_data() {
        let temp = tempfile::tempdir().unwrap();
        let current = temp.path().join(TRANSCRIBE_IDENTIFIER);
        let legacy = temp.path().join(UKTI_IDENTIFIER);
        fs::create_dir_all(&legacy).unwrap();
        fs::write(legacy.join("settings.json"), b"legacy settings").unwrap();
        fs::create_dir_all(current.join("dataset/audio")).unwrap();
        fs::create_dir_all(current.join("models/huggingface/example")).unwrap();
        fs::write(current.join("dataset/.DS_Store"), b"Finder metadata").unwrap();
        assert_eq!(compatible_app_root(current.clone()).unwrap(), legacy);
        fs::write(
            current.join("models/huggingface/example/weights.gguf"),
            b"new weights",
        )
        .unwrap();
        assert_eq!(compatible_app_root(current.clone()).unwrap(), current);
        assert_eq!(
            fs::read(legacy.join("settings.json")).unwrap(),
            b"legacy settings"
        );
    }

    #[test]
    fn explicit_new_settings_or_database_win_without_merging_or_overwriting() {
        for entry in ["settings.json", "dataset/sessions.sqlite3"] {
            let temp = tempfile::tempdir().unwrap();
            let current = temp.path().join(TRANSCRIBE_IDENTIFIER);
            let legacy = temp.path().join(UKTI_IDENTIFIER);
            fs::create_dir_all(current.join("dataset")).unwrap();
            fs::create_dir_all(&legacy).unwrap();
            fs::write(legacy.join("settings.json"), b"old").unwrap();
            // A zero-byte new file still counts: corruption must be reported,
            // not hidden by falling back to an older copy.
            fs::write(current.join(entry), []).unwrap();
            assert_eq!(compatible_app_root(current.clone()).unwrap(), current);
            assert_eq!(fs::read(legacy.join("settings.json")).unwrap(), b"old");
            assert_eq!(fs::metadata(current.join(entry)).unwrap().len(), 0);
        }
    }

    #[cfg(unix)]
    #[test]
    fn new_symlink_state_is_not_followed_or_replaced_by_legacy() {
        let temp = tempfile::tempdir().unwrap();
        let current = temp.path().join(TRANSCRIBE_IDENTIFIER);
        fs::create_dir_all(current.join("models")).unwrap();
        fs::create_dir_all(temp.path().join(UKTI_IDENTIFIER)).unwrap();
        std::os::unix::fs::symlink(current.join("models"), current.join("models/cycle")).unwrap();
        assert_eq!(compatible_app_root(current.clone()).unwrap(), current);
    }

    #[test]
    fn model_override_precedence_preserves_earlier_names() {
        let root = Path::new("selected-app-data");
        assert_eq!(
            model_root(
                root,
                Some("new-models".into()),
                Some("ukti-models".into()),
                Some("old-models".into())
            ),
            PathBuf::from("new-models")
        );
        assert_eq!(
            model_root(
                root,
                None,
                Some("ukti-models".into()),
                Some("old-models".into())
            ),
            PathBuf::from("ukti-models")
        );
        assert_eq!(
            model_root(root, None, None, Some("old-models".into())),
            PathBuf::from("old-models")
        );
        assert_eq!(
            model_root(root, Some("".into()), None, Some("old-models".into())),
            PathBuf::from("old-models")
        );
        assert_eq!(
            model_root(root, None, None, Some("".into())),
            root.join("models")
        );
        assert_eq!(model_root(root, None, None, None), root.join("models"));
    }

    #[test]
    fn ukti_state_wins_over_the_older_stt_store() {
        let temp = tempfile::tempdir().unwrap();
        let current = temp.path().join(TRANSCRIBE_IDENTIFIER);
        let ukti = temp.path().join(UKTI_IDENTIFIER);
        let stt = temp.path().join(STT_IDENTIFIER);
        fs::create_dir_all(&ukti).unwrap();
        fs::create_dir_all(&stt).unwrap();
        fs::write(ukti.join("settings.json"), b"ukti").unwrap();
        fs::write(stt.join("settings.json"), b"stt").unwrap();
        assert_eq!(compatible_app_root(current).unwrap(), ukti);
    }
}
