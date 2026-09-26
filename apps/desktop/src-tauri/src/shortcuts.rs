use tauri::{Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

#[derive(Default)]
pub struct Recorder {
    session: Option<String>,
    held: std::collections::HashSet<u32>,
}

#[derive(Debug, PartialEq, Eq)]
enum RecorderEvent {
    Pass,
    Suppress,
    Capture(String),
}

impl Recorder {
    fn begin(&mut self, session: String) {
        self.session = Some(session);
    }
    fn end(&mut self, session: &str) {
        if self.session.as_deref() == Some(session) {
            self.cancel();
        }
    }
    pub fn cancel(&mut self) {
        self.session = None;
    }
    fn route(&mut self, id: u32, pressed: bool, focused: bool) -> RecorderEvent {
        if !focused {
            self.cancel();
        }
        if !pressed {
            return if self.held.remove(&id) {
                RecorderEvent::Suppress
            } else {
                RecorderEvent::Pass
            };
        }
        if self.held.contains(&id) {
            return RecorderEvent::Suppress;
        }
        if let Some(session) = &self.session {
            self.held.insert(id);
            return RecorderEvent::Capture(session.clone());
        }
        RecorderEvent::Pass
    }
}

#[tauri::command]
pub fn begin_shortcut_recording(
    session: String,
    window: tauri::WebviewWindow,
    storage: tauri::State<'_, crate::AppStorage>,
) -> Result<(), String> {
    if window.label() != "main" || !window.is_focused().map_err(|e| e.to_string())? {
        return Err("Focus the shortcut field to record a binding.".into());
    }
    if session.is_empty()
        || session.len() > 64
        || !session
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-')
    {
        return Err("Invalid shortcut editing session.".into());
    }
    let _gate = storage.operation_gate.lock().map_err(|e| e.to_string())?;
    storage.runtime.require_idle()?;
    crate::dictation::require_shortcut_edit_idle(&storage)?;
    storage
        .shortcut_recorder
        .lock()
        .map_err(|e| e.to_string())?
        .begin(session);
    Ok(())
}

#[tauri::command]
pub fn end_shortcut_recording(
    session: String,
    window: tauri::WebviewWindow,
    storage: tauri::State<'_, crate::AppStorage>,
) -> Result<(), String> {
    if window.label() != "main" {
        return Err("Only Settings can edit recording shortcuts.".into());
    }
    storage
        .shortcut_recorder
        .lock()
        .map_err(|e| e.to_string())?
        .end(&session);
    Ok(())
}

// Existing registered hotkeys may be consumed before a WebView keydown. Feed
// them to the focused editor through the existing callback, without a new hook.
pub fn capture_event(app: &tauri::AppHandle, shortcut: &Shortcut, event: ShortcutState) -> bool {
    let focused = app
        .get_webview_window("main")
        .is_some_and(|window| window.is_focused().unwrap_or(false));
    let storage = app.state::<crate::AppStorage>();
    let focused = focused
        && storage.runtime.require_idle().is_ok()
        && crate::dictation::require_shortcut_edit_idle(&storage).is_ok();
    let action = match storage.shortcut_recorder.lock() {
        Ok(mut recorder) => recorder.route(shortcut.id(), event == ShortcutState::Pressed, focused),
        Err(_) => return true,
    };
    match action {
        RecorderEvent::Pass => false,
        RecorderEvent::Suppress => true,
        RecorderEvent::Capture(session) => {
            let mut parts = Vec::new();
            for (flag, name) in [
                (Modifiers::CONTROL, "Control"),
                (Modifiers::SUPER, "Super"),
                (Modifiers::ALT, "Alt"),
                (Modifiers::SHIFT, "Shift"),
            ] {
                if shortcut.mods.contains(flag) {
                    parts.push(name);
                }
            }
            let code = shortcut.key.to_string();
            let key = code
                .strip_prefix("Key")
                .or_else(|| code.strip_prefix("Digit"))
                .unwrap_or(&code);
            parts.push(key);
            let _ = app.emit_to(
                "main",
                "shortcut-recorded",
                serde_json::json!({ "session": session, "shortcut": parts.join("+") }),
            );
            true
        }
    }
}

// Compare/register native identities, not spelling or modifier order. Otherwise
// changing CommandOrControl to Super on macOS can unregister the retained key.
pub fn canonical_bindings(bindings: [&str; 2]) -> Result<[String; 2], String> {
    let parse = |value: &str| {
        value
            .parse::<tauri_plugin_global_shortcut::Shortcut>()
            .map(|shortcut| shortcut.to_string())
            .map_err(|error| error.to_string())
    };
    let next = [parse(bindings[0])?, parse(bindings[1])?];
    if next[0] == next[1] {
        return Err("Hold and Toggle must use different shortcuts.".into());
    }
    Ok(next)
}

pub trait ShortcutBackend {
    fn is_registered(&self, shortcut: &str) -> bool;
    fn register(&self, shortcut: &str) -> Result<(), String>;
    fn unregister(&self, shortcut: &str) -> Result<(), String>;
}

impl ShortcutBackend for tauri::AppHandle {
    fn is_registered(&self, shortcut: &str) -> bool {
        self.global_shortcut().is_registered(shortcut)
    }
    fn register(&self, shortcut: &str) -> Result<(), String> {
        self.global_shortcut()
            .register(shortcut)
            .map_err(|e| e.to_string())
    }
    fn unregister(&self, shortcut: &str) -> Result<(), String> {
        self.global_shortcut()
            .unregister(shortcut)
            .map_err(|e| e.to_string())
    }
}

#[derive(Default)]
pub struct ShortcutStatus {
    pub error: Option<String>,
}

impl ShortcutStatus {
    pub fn replace(
        &mut self,
        backend: &impl ShortcutBackend,
        previous: &[&str],
        next: &[&str],
    ) -> Result<(), String> {
        // Keep all existing bindings until every replacement is registered.
        // In particular, swapping Hold and Toggle requires no OS registration.
        let mut added = Vec::new();
        let mut removed = Vec::new();
        let result: Result<(), String> = (|| {
            for shortcut in next {
                if !backend.is_registered(shortcut) {
                    backend.register(shortcut).map_err(|error| format!(
                        "Could not register {shortcut}: {error}. Choose another shortcut in Settings or retry."
                    ))?;
                    added.push(*shortcut);
                }
            }
            for shortcut in previous {
                if !next.contains(shortcut) && backend.is_registered(shortcut) {
                    backend
                        .unregister(shortcut)
                        .map_err(|error| format!("Could not release {shortcut}: {error}"))?;
                    removed.push(*shortcut);
                }
            }
            Ok(())
        })();
        let result = result.map_err(|mut error| {
            for shortcut in removed {
                if let Err(rollback) = backend.register(shortcut) {
                    error.push_str(&format!("; could not restore {shortcut}: {rollback}"));
                }
            }
            for shortcut in added {
                if let Err(rollback) = backend.unregister(shortcut) {
                    error.push_str(&format!(
                        "; could not release replacement {shortcut}: {rollback}"
                    ));
                }
            }
            error
        });
        self.error = result.as_ref().err().cloned();
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::HashSet;

    #[test]
    fn recorder_routes_registered_keys_without_starting_dictation_or_leaking_releases() {
        let mut recorder = Recorder::default();
        assert_eq!(recorder.route(7, true, true), RecorderEvent::Pass);
        recorder.begin("hold-editor".into());
        assert_eq!(
            recorder.route(7, true, true),
            RecorderEvent::Capture("hold-editor".into())
        );
        assert_eq!(recorder.route(7, true, true), RecorderEvent::Suppress);
        recorder.end("hold-editor");
        assert_eq!(recorder.route(7, true, true), RecorderEvent::Suppress);
        assert_eq!(recorder.route(7, false, true), RecorderEvent::Suppress);
        assert_eq!(recorder.route(7, true, true), RecorderEvent::Pass);
    }

    #[test]
    fn recorder_focus_loss_and_late_cleanup_do_not_affect_a_new_editor() {
        let mut recorder = Recorder::default();
        recorder.begin("old-editor".into());
        recorder.begin("new-editor".into());
        recorder.end("old-editor");
        assert_eq!(
            recorder.route(8, true, true),
            RecorderEvent::Capture("new-editor".into())
        );
        recorder.cancel();
        assert_eq!(recorder.route(8, false, false), RecorderEvent::Suppress);
        recorder.begin("another-editor".into());
        assert_eq!(recorder.route(9, true, false), RecorderEvent::Pass);
        assert_eq!(recorder.route(9, true, true), RecorderEvent::Pass);
    }

    #[test]
    fn native_parser_accepts_custom_key_contract_and_canonicalizes_aliases() {
        let mut keys = ('A'..='Z')
            .chain('0'..='9')
            .map(|key| key.to_string())
            .collect::<Vec<_>>();
        keys.extend((1..=24).map(|key| format!("F{key}")));
        keys.extend(
            [
                "Space",
                "ArrowUp",
                "ArrowDown",
                "ArrowLeft",
                "ArrowRight",
                "Home",
                "End",
                "PageUp",
                "PageDown",
                "Backquote",
                "Minus",
                "Equal",
                "BracketLeft",
                "BracketRight",
                "Backslash",
                "Semicolon",
                "Quote",
                "Comma",
                "Period",
                "Slash",
            ]
            .map(String::from),
        );
        for key in keys {
            let chord = format!("Control+Alt+{key}");
            assert!(
                vesper_core::recording_shortcut_identity(&chord, cfg!(target_os = "macos"))
                    .is_some()
            );
            chord
                .parse::<tauri_plugin_global_shortcut::Shortcut>()
                .unwrap();
        }
        assert_eq!(
            canonical_bindings(["Alt+Control+K", "Alt+F8"]).unwrap(),
            canonical_bindings(["Control+Alt+K", "Alt+F8"]).unwrap()
        );
        let primary = if cfg!(target_os = "macos") {
            "Super+Shift+D"
        } else {
            "Control+Shift+D"
        };
        assert!(canonical_bindings(["CommandOrControl+Shift+D", primary]).is_err());
    }

    #[test]
    fn equivalent_custom_spelling_does_not_unregister_the_retained_binding() {
        let backend = FakeBackend::default();
        let mut status = ShortcutStatus::default();
        let previous = canonical_bindings(["Control+Alt+K", "Alt+F8"]).unwrap();
        let next = canonical_bindings(["Alt+Control+K", "Alt+F9"]).unwrap();
        let previous = previous.each_ref().map(String::as_str);
        let next = next.each_ref().map(String::as_str);
        status.replace(&backend, &[], &previous).unwrap();
        status.replace(&backend, &previous, &next).unwrap();
        assert!(backend.is_registered(next[0]));
        assert!(backend.is_registered(next[1]));
        assert_eq!(backend.active.borrow().len(), 2);
    }

    #[derive(Default)]
    struct FakeBackend {
        active: RefCell<HashSet<String>>,
        unavailable: RefCell<HashSet<String>>,
        cannot_release: RefCell<HashSet<String>>,
    }
    impl ShortcutBackend for FakeBackend {
        fn is_registered(&self, shortcut: &str) -> bool {
            self.active.borrow().contains(shortcut)
        }
        fn register(&self, shortcut: &str) -> Result<(), String> {
            if self.unavailable.borrow().contains(shortcut) {
                return Err("shortcut in use".into());
            }
            self.active.borrow_mut().insert(shortcut.into());
            Ok(())
        }
        fn unregister(&self, shortcut: &str) -> Result<(), String> {
            if self.cannot_release.borrow().contains(shortcut) {
                return Err("release failed".into());
            }
            self.active.borrow_mut().remove(shortcut);
            Ok(())
        }
    }

    #[test]
    fn startup_failure_is_retained_and_retry_clears_it() {
        let backend = FakeBackend::default();
        backend.unavailable.borrow_mut().insert("saved".into());
        let mut status = ShortcutStatus::default();
        assert!(status.replace(&backend, &[], &["saved"]).is_err());
        assert!(status.error.as_deref().unwrap().contains("shortcut in use"));
        backend.unavailable.borrow_mut().clear();
        status.replace(&backend, &["saved"], &["saved"]).unwrap();
        assert!(status.error.is_none());
        assert!(backend.is_registered("saved"));
    }

    #[test]
    fn conflict_preserves_working_shortcut_and_a_new_selection_recovers() {
        let backend = FakeBackend::default();
        let mut status = ShortcutStatus::default();
        status.replace(&backend, &[], &["old"]).unwrap();
        backend.unavailable.borrow_mut().insert("busy".into());
        assert!(status.replace(&backend, &["old"], &["busy"]).is_err());
        assert!(backend.is_registered("old"));
        status.replace(&backend, &["old"], &["new"]).unwrap();
        assert!(!backend.is_registered("old"));
        assert!(backend.is_registered("new"));
        assert!(status.error.is_none());
        status.replace(&backend, &["new"], &["new"]).unwrap();
    }
    #[test]
    fn two_bindings_rollback_a_partial_registration_and_allow_role_swaps() {
        let backend = FakeBackend::default();
        let mut status = ShortcutStatus::default();
        status.replace(&backend, &[], &["hold", "toggle"]).unwrap();
        backend.unavailable.borrow_mut().insert("busy".into());
        assert!(status
            .replace(&backend, &["hold", "toggle"], &["new-hold", "busy"])
            .is_err());
        assert!(backend.is_registered("hold") && backend.is_registered("toggle"));
        assert!(!backend.is_registered("new-hold"));
        status
            .replace(&backend, &["hold", "toggle"], &["toggle", "hold"])
            .unwrap();
        assert!(status.error.is_none());
        backend.cannot_release.borrow_mut().insert("toggle".into());
        assert!(status
            .replace(&backend, &["hold", "toggle"], &["new-hold", "new-toggle"])
            .is_err());
        assert!(backend.is_registered("hold") && backend.is_registered("toggle"));
        assert!(!backend.is_registered("new-hold") && !backend.is_registered("new-toggle"));
    }
}
