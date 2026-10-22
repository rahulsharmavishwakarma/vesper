//! One-time first-run state.
//!
//! The state lives in its own `onboarding.json` beside `settings.json`, so an
//! app update (which only replaces the bundle) or **Restore defaults** never
//! shows the flow again. It is decided once at startup, before anything else
//! writes into the profile: a profile that already has settings, models or
//! recordings is an existing install and is marked completed without ever
//! seeing the flow. An unreadable file is treated as completed so a damaged
//! file can never trap anyone in setup.
use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

const VERSION: u32 = 1;
/// Index of the last step (Ready). Saved steps are clamped to it.
pub const LAST_STEP: u8 = 7;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Pending,
    Completed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    Finished,
    ExistingInstall,
    Unreadable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OnboardingState {
    pub version: u32,
    pub status: Status,
    /// The step to resume at while pending (0 is Welcome).
    pub step: u8,
    pub reason: Option<Reason>,
    /// Seconds since the Unix epoch.
    pub completed_at: Option<u64>,
}

impl OnboardingState {
    fn pending() -> Self {
        Self {
            version: VERSION,
            status: Status::Pending,
            step: 0,
            reason: None,
            completed_at: None,
        }
    }
    fn completed(reason: Reason) -> Self {
        Self {
            version: VERSION,
            status: Status::Completed,
            step: LAST_STEP,
            reason: Some(reason),
            completed_at: Some(now()),
        }
    }
}

pub struct OnboardingStore {
    path: PathBuf,
    state: Mutex<OnboardingState>,
}

impl OnboardingStore {
    /// Read the saved state, or decide it for this profile on first launch.
    /// Call before anything writes into the model or data folders.
    pub fn open(path: PathBuf, existing_profile: bool) -> Self {
        let state = match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .unwrap_or_else(|_| OnboardingState::completed(Reason::Unreadable)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let state = if existing_profile {
                    OnboardingState::completed(Reason::ExistingInstall)
                } else {
                    OnboardingState::pending()
                };
                if let Err(error) = write(&path, &state) {
                    eprintln!("could not save first-run state: {error}");
                }
                state
            }
            Err(_) => OnboardingState::completed(Reason::Unreadable),
        };
        Self {
            path,
            state: Mutex::new(state),
        }
    }

    pub fn get(&self) -> Result<OnboardingState, String> {
        Ok(self.state.lock().map_err(|e| e.to_string())?.clone())
    }

    /// Remember where to resume. A completed flow is never reopened.
    pub fn save_step(&self, step: u8) -> Result<OnboardingState, String> {
        let mut state = self.state.lock().map_err(|e| e.to_string())?;
        if state.status == Status::Completed {
            return Ok(state.clone());
        }
        let mut next = state.clone();
        next.step = step.min(LAST_STEP);
        write(&self.path, &next).map_err(|e| format!("Could not save setup progress: {e}"))?;
        *state = next;
        Ok(state.clone())
    }

    pub fn complete(&self) -> Result<OnboardingState, String> {
        let mut state = self.state.lock().map_err(|e| e.to_string())?;
        if state.status == Status::Completed {
            return Ok(state.clone());
        }
        let next = OnboardingState::completed(Reason::Finished);
        write(&self.path, &next).map_err(|e| format!("Could not finish setup: {e}"))?;
        *state = next;
        Ok(state.clone())
    }
}

/// A profile someone has already used: saved settings, downloaded models or
/// saved recordings. Hidden entries such as `.DS_Store` do not count.
pub fn existing_profile(settings: &Path, model_dir: &Path, dataset_dir: &Path) -> bool {
    settings.exists() || has_entries(model_dir) || has_entries(dataset_dir)
}

fn has_entries(dir: &Path) -> bool {
    fs::read_dir(dir).is_ok_and(|entries| {
        entries
            .flatten()
            .any(|entry| !entry.file_name().to_string_lossy().starts_with('.'))
    })
}

fn write(path: &Path, state: &OnboardingState) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, serde_json::to_vec_pretty(state)?)?;
    fs::rename(temporary, path)
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}

#[tauri::command]
pub fn get_onboarding(
    storage: tauri::State<'_, crate::AppStorage>,
) -> Result<OnboardingState, String> {
    storage.onboarding.get()
}

#[tauri::command]
pub fn save_onboarding_step(
    step: u8,
    storage: tauri::State<'_, crate::AppStorage>,
) -> Result<OnboardingState, String> {
    storage.onboarding.save_step(step)
}

#[tauri::command]
pub fn complete_onboarding(
    storage: tauri::State<'_, crate::AppStorage>,
) -> Result<OnboardingState, String> {
    storage.onboarding.complete()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(dir: &Path, existing: bool) -> OnboardingStore {
        OnboardingStore::open(dir.join("onboarding.json"), existing)
    }

    #[test]
    fn a_fresh_profile_starts_the_flow_once_and_never_again() {
        let temp = tempfile::tempdir().unwrap();
        let first = store(temp.path(), false);
        assert_eq!(first.get().unwrap().status, Status::Pending);
        first.save_step(3).unwrap();
        // Quitting mid-flow resumes at the saved step.
        let resumed = store(temp.path(), true);
        assert_eq!(resumed.get().unwrap().status, Status::Pending);
        assert_eq!(resumed.get().unwrap().step, 3);
        resumed.complete().unwrap();
        // Later launches, including after an update, stay completed.
        let later = store(temp.path(), true);
        let state = later.get().unwrap();
        assert_eq!(state.status, Status::Completed);
        assert_eq!(state.reason, Some(Reason::Finished));
        assert!(state.completed_at.is_some());
        // A completed flow is never reopened by a stale step save.
        assert_eq!(later.save_step(2).unwrap().status, Status::Completed);
    }

    #[test]
    fn existing_installs_are_marked_completed_without_the_flow() {
        let temp = tempfile::tempdir().unwrap();
        let state = store(temp.path(), true).get().unwrap();
        assert_eq!(state.status, Status::Completed);
        assert_eq!(state.reason, Some(Reason::ExistingInstall));
        // The decision is written down, so it holds even if settings are reset.
        assert_eq!(
            store(temp.path(), false).get().unwrap().reason,
            Some(Reason::ExistingInstall)
        );
    }

    #[test]
    fn an_unreadable_state_file_never_traps_anyone_in_setup() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("onboarding.json"), b"{ not json").unwrap();
        let state = store(temp.path(), false).get().unwrap();
        assert_eq!(state.status, Status::Completed);
        assert_eq!(state.reason, Some(Reason::Unreadable));
    }

    #[test]
    fn steps_are_clamped_to_the_last_step() {
        let temp = tempfile::tempdir().unwrap();
        assert_eq!(
            store(temp.path(), false).save_step(42).unwrap().step,
            LAST_STEP
        );
    }

    #[test]
    fn a_used_profile_is_recognised_by_settings_models_or_recordings() {
        let temp = tempfile::tempdir().unwrap();
        let settings = temp.path().join("settings.json");
        let models = temp.path().join("models");
        let dataset = temp.path().join("dataset");
        assert!(!existing_profile(&settings, &models, &dataset));
        fs::create_dir_all(&models).unwrap();
        fs::write(models.join(".DS_Store"), b"").unwrap();
        assert!(
            !existing_profile(&settings, &models, &dataset),
            "hidden files do not count"
        );
        fs::write(models.join("model.gguf"), b"weights").unwrap();
        assert!(existing_profile(&settings, &models, &dataset));
        fs::remove_file(models.join("model.gguf")).unwrap();
        fs::create_dir_all(dataset.join("sessions")).unwrap();
        assert!(existing_profile(&settings, &models, &dataset));
        fs::remove_dir_all(&dataset).unwrap();
        fs::write(&settings, b"{}").unwrap();
        assert!(existing_profile(&settings, &models, &dataset));
    }
}
