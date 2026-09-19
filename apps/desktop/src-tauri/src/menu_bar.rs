//! Native status menu, sharing the native dictation controller with shortcuts.
use std::sync::{atomic::Ordering, Arc, Mutex};
use std::time::Duration;

use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    Emitter, Manager,
};

use crate::AppStorage;

const TRAY_ID: &str = "transcribe-menu-bar";

#[derive(Clone, Debug, PartialEq, Eq)]
struct MenuStatus {
    label: &'static str,
    capture_label: &'static str,
    capture_enabled: bool,
    quit_enabled: bool,
}

impl MenuStatus {
    fn from_activity(busy: bool, stage: &str, closing: bool) -> Self {
        let recording = busy && stage == "recording";
        Self {
            label: if closing {
                "Quitting…"
            } else if busy {
                match stage {
                    "recording" => "● Recording",
                    "transcribing" => "Transcribing…",
                    "rewriting" => "Polishing text…",
                    "installing" => "Downloading / verifying models…",
                    "removing" => "Removing model files…",
                    "saving" => "Saving…",
                    "cancelling" => "Cancelling…",
                    _ => "Working…",
                }
            } else if stage == "error" {
                "Needs attention — open transcribe"
            } else {
                "Idle · on-device"
            },
            capture_label: if recording {
                "Stop recording"
            } else {
                "Start recording"
            },
            capture_enabled: !closing && (!busy || recording),
            quit_enabled: !closing && !busy,
        }
    }

    fn read(app: &tauri::AppHandle) -> Self {
        let storage = app.state::<AppStorage>();
        match storage.runtime.activity() {
            Ok(activity) => Self::from_activity(
                activity["busy"].as_bool().unwrap_or(true),
                activity["stage"].as_str().unwrap_or("unknown"),
                storage.runtime.closing.load(Ordering::Relaxed),
            ),
            Err(_) => Self {
                label: "Status unavailable — open transcribe",
                capture_label: "Start recording",
                capture_enabled: false,
                quit_enabled: false,
            },
        }
    }
}

pub struct MenuBar {
    // Retained until the main WebView acknowledges it, including clicks during startup.
    navigation: Mutex<Option<String>>,
    stopped: Arc<std::sync::atomic::AtomicBool>,
}

fn destination(id: &str) -> Option<&'static str> {
    match id {
        "tray-history" => Some("history"),
        "tray-models" => Some("models"),
        "tray-settings" => Some("settings"),
        "tray-about" => Some("about"),
        _ => None,
    }
}

pub fn show_main(app: &tauri::AppHandle, view: Option<&str>) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or("The main window is unavailable.")?;
    if let Some(view) = view {
        *app.state::<MenuBar>()
            .navigation
            .lock()
            .map_err(|error| error.to_string())? = Some(view.into());
        window
            .emit("menu-navigate", ())
            .map_err(|error| error.to_string())?;
    }
    window.unminimize().map_err(|error| error.to_string())?;
    window.show().map_err(|error| error.to_string())?;
    window.set_focus().map_err(|error| error.to_string())
}

#[tauri::command]
pub fn take_menu_navigation(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
) -> Result<Option<String>, String> {
    if window.label() != "main" {
        return Ok(None);
    }
    Ok(app
        .state::<MenuBar>()
        .navigation
        .lock()
        .map_err(|error| error.to_string())?
        .take())
}

/// Shared by the global shortcut and menu, retaining the floating-bar preference.
pub fn toggle_capture(app: &tauri::AppHandle, from_menu: bool) {
    if !MenuStatus::read(app).capture_enabled {
        return;
    }
    let _ = from_menu;
    crate::dictation::toggle(app);
}

pub fn setup(app: &tauri::AppHandle) -> tauri::Result<()> {
    let initial = MenuStatus::read(app);
    let status = MenuItem::with_id(app, "tray-status", initial.label, false, None::<&str>)?;
    let capture = MenuItem::with_id(
        app,
        "tray-capture",
        initial.capture_label,
        initial.capture_enabled,
        None::<&str>,
    )?;
    let open = MenuItem::with_id(app, "tray-open", "Open transcribe", true, None::<&str>)?;
    let history = MenuItem::with_id(app, "tray-history", "History", true, None::<&str>)?;
    let models = MenuItem::with_id(app, "tray-models", "Models…", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "tray-settings", "Settings…", true, Some("CmdOrCtrl+,"))?;
    let about = MenuItem::with_id(app, "tray-about", "About transcribe…", true, None::<&str>)?;
    let quit = MenuItem::with_id(
        app,
        "tray-quit",
        "Quit transcribe",
        initial.quit_enabled,
        Some("CmdOrCtrl+Q"),
    )?;
    let menu = Menu::with_items(
        app,
        &[
            &status,
            &PredefinedMenuItem::separator(app)?,
            &capture,
            &open,
            &PredefinedMenuItem::separator(app)?,
            &history,
            &models,
            &settings,
            &about,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;
    let builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip(format!("transcribe — {}", initial.label))
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| {
            let id = event.id.as_ref();
            if let Some(view) = destination(id) {
                let _ = show_main(app, Some(view));
            } else {
                match id {
                    "tray-open" => {
                        let _ = show_main(app, None);
                    }
                    "tray-capture" => toggle_capture(app, true),
                    // Recheck because the native menu may have been opened before work began.
                    "tray-quit" if MenuStatus::read(app).quit_enabled => app.exit(0),
                    _ => {}
                }
            }
        });
    // The word is the brand on macOS. Other system trays require a bitmap icon.
    #[cfg(target_os = "macos")]
    let builder = builder.title("transcribe");
    #[cfg(not(target_os = "macos"))]
    let builder = if let Some(icon) = app.default_window_icon() {
        builder.icon(icon.clone())
    } else {
        builder
    };
    let tray = builder.build(app)?;
    let stopped = Arc::new(std::sync::atomic::AtomicBool::new(false));
    app.manage(MenuBar {
        navigation: Mutex::new(None),
        stopped: Arc::clone(&stopped),
    });
    // Retain the platform's Edit/Window menus and route About to the shared sheet.
    #[cfg(target_os = "macos")]
    {
        let native_menu = Menu::default(app)?;
        let items = native_menu.items()?;
        if let Some(app_menu) = items.first().and_then(|item| item.as_submenu()) {
            app_menu.remove_at(0)?;
            app_menu.prepend(&MenuItem::with_id(
                app,
                "app-about",
                "About transcribe…",
                true,
                None::<&str>,
            )?)?;
        }
        app.set_menu(native_menu)?;
        app.on_menu_event(|app, event| {
            if event.id.as_ref() == "app-about" {
                let _ = show_main(app, Some("about"));
            }
        });
    }
    let app = app.clone();
    std::thread::Builder::new()
        .name("transcribe-menu-status".into())
        .spawn(move || {
            let mut previous = initial;
            while !stopped.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(500));
                if stopped.load(Ordering::Relaxed) {
                    break;
                }
                let next = MenuStatus::read(&app);
                if next == previous {
                    continue;
                }
                // Tauri dispatches these native menu mutations onto the main thread.
                let update = (|| -> tauri::Result<()> {
                    status.set_text(next.label)?;
                    capture.set_text(next.capture_label)?;
                    capture.set_enabled(next.capture_enabled)?;
                    quit.set_enabled(next.quit_enabled)?;
                    tray.set_tooltip(Some(format!("transcribe — {}", next.label)))?;
                    Ok(())
                })();
                if update.is_ok() {
                    previous = next;
                }
            }
        })?;
    Ok(())
}

pub fn stop(app: &tauri::AppHandle) {
    if let Some(menu) = app.try_state::<MenuBar>() {
        menu.stopped.store(true, Ordering::Relaxed);
    }
}

/// A close button hides the workspace; only an explicit, idle quit exits the app.
/// This gate also covers Cmd-Q / the Dock menu, not just our own menu item.
pub fn prevent_exit(app: &tauri::AppHandle) -> bool {
    let storage = app.state::<AppStorage>();
    let Ok(_gate) = storage.operation_gate.lock() else {
        return true;
    };
    if storage.runtime.prepare_exit().is_err() {
        let _ = show_main(app, Some("dictate"));
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quitting_never_cancels_active_work_and_blocks_new_work_after_idle_exit() {
        let runtime = crate::runtime::Runtime::default();
        for stage in [
            "recording",
            "transcribing",
            "rewriting",
            "installing",
            "removing",
        ] {
            runtime.begin(stage, "Working").unwrap();
            assert!(runtime.prepare_exit().is_err());
            assert!(runtime.is_busy());
            assert!(!runtime.closing.load(Ordering::Relaxed));
            assert!(!runtime.cancelled.load(Ordering::Relaxed));
            assert_eq!(runtime.activity().unwrap()["stage"], stage);
            runtime.finish(&Ok::<(), String>(()));
        }
        runtime.prepare_exit().unwrap();
        assert!(runtime.begin("recording", "Starting").is_err());
        assert!(runtime.begin("installing", "Downloading").is_err());
    }

    #[test]
    fn recording_can_stop_but_processing_cannot_toggle_or_quit() {
        let recording = MenuStatus::from_activity(true, "recording", false);
        assert_eq!(recording.capture_label, "Stop recording");
        assert!(recording.capture_enabled);
        assert!(!recording.quit_enabled);
        for stage in [
            "transcribing",
            "rewriting",
            "installing",
            "removing",
            "saving",
            "cancelling",
            "future-stage",
        ] {
            let status = MenuStatus::from_activity(true, stage, false);
            assert!(!status.capture_enabled, "{stage}");
            assert!(!status.quit_enabled, "{stage}");
            assert_ne!(status.label, "Idle · on-device");
        }
    }

    #[test]
    fn completion_errors_and_shutdown_have_safe_actions() {
        for stage in ["idle", "cancelled", "error", "recording"] {
            let idle = MenuStatus::from_activity(false, stage, false);
            assert_eq!(idle.capture_label, "Start recording");
            assert!(idle.capture_enabled && idle.quit_enabled);
        }
        assert!(MenuStatus::from_activity(false, "error", false)
            .label
            .contains("Needs attention"));
        let closing = MenuStatus::from_activity(false, "idle", true);
        assert!(!closing.capture_enabled && !closing.quit_enabled);
    }

    #[test]
    fn only_our_navigation_items_change_the_current_view() {
        assert_eq!(destination("tray-history"), Some("history"));
        assert_eq!(destination("tray-models"), Some("models"));
        assert_eq!(destination("tray-settings"), Some("settings"));
        assert_eq!(destination("tray-about"), Some("about"));
        for id in [
            "tray-open",
            "tray-capture",
            "tray-status",
            "quit",
            "settings",
            "unknown",
        ] {
            assert_eq!(destination(id), None);
        }
    }
}
