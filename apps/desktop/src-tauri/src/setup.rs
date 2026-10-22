//! Device facts and permission checks used by first-run setup (issue #270).
//! Reads never prompt; the microphone prompt and the System Settings panes
//! open only from an explicit action.
use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceProfile {
    pub os: String,
    pub arch: String,
    pub chip: Option<String>,
    pub memory_bytes: Option<u64>,
    /// Free space on the volume that holds the model folder.
    pub free_bytes: Option<u64>,
}

#[tauri::command]
pub fn get_device_profile(storage: tauri::State<'_, crate::AppStorage>) -> DeviceProfile {
    let (chip, memory_bytes) = vesper_desktop_platform::device();
    DeviceProfile {
        os: std::env::consts::OS.into(),
        arch: std::env::consts::ARCH.into(),
        chip,
        memory_bytes,
        free_bytes: vesper_desktop_platform::free_space(&storage.model_dir),
    }
}

pub fn microphone_label(code: i32) -> &'static str {
    match code {
        1 => "restricted",
        2 => "denied",
        3 => "allowed",
        4 => "asking",
        _ => "not_asked",
    }
}

/// `request` shows the system prompt only when the user has not been asked.
#[tauri::command]
pub fn microphone_access(request: bool) -> &'static str {
    microphone_label(vesper_desktop_platform::microphone(request))
}

pub fn privacy_pane_url(pane: &str) -> Option<&'static str> {
    match pane {
        "microphone" => {
            Some("x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone")
        }
        "accessibility" => {
            Some("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
        }
        _ => None,
    }
}

#[tauri::command]
pub fn open_privacy_settings(pane: String) -> Result<(), String> {
    let url = privacy_pane_url(&pane).ok_or("Unknown System Settings pane.")?;
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("/usr/bin/open")
            .arg(url)
            .status()
            .map_err(|error| format!("Could not open System Settings: {error}"))
            .and_then(|status| {
                status
                    .success()
                    .then_some(())
                    .ok_or_else(|| "Could not open System Settings.".into())
            })
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = url;
        Err("System Settings is only available on macOS.".into())
    }
}

/// Start listening so setup can show a live level. It holds the operation slot
/// like a recording, saves nothing and stops on its own after 30 seconds.
#[tauri::command]
pub fn start_microphone_check(
    device_id: Option<String>,
    storage: tauri::State<'_, crate::AppStorage>,
) -> Result<(), String> {
    storage
        .runtime
        .begin("checking-microphone", "Checking the microphone…")?;
    let result = storage
        .recorder
        .start(device_id, 30)
        .map_err(|error| error.to_string());
    if result.is_err() {
        storage.runtime.finish(&result);
    }
    result
}

#[tauri::command]
pub fn stop_microphone_check(storage: tauri::State<'_, crate::AppStorage>) -> Result<(), String> {
    let result = storage.recorder.cancel().map_err(|error| error.to_string());
    storage.runtime.finish(&Ok::<(), String>(()));
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn microphone_codes_map_to_stable_labels() {
        assert_eq!(microphone_label(0), "not_asked");
        assert_eq!(microphone_label(1), "restricted");
        assert_eq!(microphone_label(2), "denied");
        assert_eq!(microphone_label(3), "allowed");
        assert_eq!(microphone_label(4), "asking");
        assert_eq!(microphone_label(-7), "not_asked");
    }

    #[test]
    fn only_known_privacy_panes_open() {
        assert!(
            privacy_pane_url("microphone").is_some_and(|url| url.ends_with("Privacy_Microphone"))
        );
        assert!(privacy_pane_url("accessibility")
            .is_some_and(|url| url.ends_with("Privacy_Accessibility")));
        assert_eq!(privacy_pane_url("x-apple.systempreferences:anything"), None);
        assert_eq!(privacy_pane_url(""), None);
    }
}
