//! Operation-scoped shortcut orchestration. Audio and inference remain owned by
//! the existing runtime; the WebView only renders state and requests actions.
use crate::{commands, runtime::PipelineResult, AppStorage};
use serde::Serialize;
use std::sync::{atomic::Ordering, Mutex};
use std::time::{Duration, Instant};
use tauri::{Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
use vesper_core::{DictationOutput, RecordingMode};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    #[default]
    Idle,
    Starting,
    Recording,
    Finishing,
    Pasting,
    Ready,
    Sent,
    Cancelled,
    Error,
}
impl Phase {
    fn active(self) -> bool {
        matches!(
            self,
            Self::Starting | Self::Recording | Self::Finishing | Self::Pasting
        )
    }
}
#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    id: u64,
    revision: u64,
    phase: Phase,
    mode: RecordingMode,
    operation_id: Option<String>,
    message: String,
    text: Option<String>,
    started_at_unix_ms: Option<u64>,
    capture_ready_ms: Option<u64>,
    stop_to_final_ms: Option<u64>,
    final_to_dispatch_ms: Option<u64>,
}
#[derive(Default)]
pub struct Dictation {
    state: Mutex<State>,
}
#[derive(Default)]
struct State {
    status: Status,
    key_down: [bool; 2],
    held_session: bool,
    stop_requested: bool,
    cancelled: bool,
    output: DictationOutput,
    target_ready: bool,
    began: Option<Instant>,
    released: Option<Instant>,
}
#[derive(Debug, PartialEq, Eq)]
enum Action {
    None,
    Start(u64),
    Stop(u64),
    Cancel,
}
impl State {
    fn may_start_microphone(&self, id: u64) -> bool {
        self.status.id == id
            && self.status.phase == Phase::Starting
            && !self.cancelled
            && !self.stop_requested
    }
    fn adopt_main_recording(&mut self, operation: Option<String>) {
        self.output = DictationOutput::App;
        self.target_ready = false;
        self.held_session = false;
        self.status.operation_id = operation;
        self.status.phase = Phase::Finishing;
        self.released = Some(Instant::now());
    }
    fn toggle(&mut self, mode: RecordingMode, output: DictationOutput, from_key: bool) -> Action {
        match self.status.phase {
            Phase::Starting => {
                self.stop_requested = true;
                self.released.get_or_insert_with(Instant::now);
                Action::None
            }
            Phase::Recording => {
                self.stop_requested = true;
                self.released.get_or_insert_with(Instant::now);
                self.status.phase = Phase::Finishing;
                Action::Stop(self.status.id)
            }
            phase if phase.active() => Action::None,
            _ => {
                self.status = Status {
                    id: self.status.id.wrapping_add(1).max(1),
                    phase: Phase::Starting,
                    mode,
                    message: "Preparing microphone…".into(),
                    ..Status::default()
                };
                self.held_session = from_key && mode == RecordingMode::Hold;
                self.stop_requested = false;
                self.cancelled = false;
                self.output = output;
                self.target_ready = false;
                self.began = Some(Instant::now());
                self.released = None;
                Action::Start(self.status.id)
            }
        }
    }
    fn key(&mut self, pressed: bool, mode: RecordingMode, output: DictationOutput) -> Action {
        let index = if mode == RecordingMode::Hold { 0 } else { 1 };
        if pressed {
            if self.key_down[index] {
                return Action::None;
            }
            self.key_down[index] = true;
            if self.status.phase.active() && mode == RecordingMode::Hold {
                return Action::None;
            }
            self.toggle(mode, output, true)
        } else {
            let was_down = self.key_down[index];
            self.key_down[index] = false;
            if was_down
                && mode == RecordingMode::Hold
                && self.held_session
                && matches!(self.status.phase, Phase::Starting | Phase::Recording)
            {
                self.toggle(mode, output, true)
            } else {
                Action::None
            }
        }
    }
    fn ready(&mut self, id: u64) -> Action {
        if self.status.id != id || self.status.phase != Phase::Starting {
            return Action::None;
        }
        if self.cancelled {
            return Action::Cancel;
        }
        self.status.capture_ready_ms = self.began.map(|t| t.elapsed().as_millis() as u64);
        self.status.started_at_unix_ms = Some(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        );
        if self.stop_requested {
            self.status.phase = Phase::Finishing;
            Action::Stop(self.status.id)
        } else {
            self.status.phase = Phase::Recording;
            Action::None
        }
    }
}
fn emit(app: &tauri::AppHandle) {
    if let Ok(mut state) = app.state::<AppStorage>().dictation.state.lock() {
        state.status.revision = state.status.revision.wrapping_add(1);
        let _ = app.emit("dictation-state", state.status.clone());
    }
}
/// All AppKit/AX/clipboard calls are executed on the UI thread. No foreign
/// pointers cross this boundary, and a stalled UI cannot block the worker forever.
pub(crate) async fn on_main<T: Send + 'static>(
    app: &tauri::AppHandle,
    f: impl FnOnce() -> T + Send + 'static,
) -> Result<T, String> {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        app.run_on_main_thread(move || {
            let _ = tx.send(f());
        })
        .map_err(|e| e.to_string())?;
        rx.recv_timeout(Duration::from_secs(5))
            .map_err(|_| "The desktop integration did not respond.".into())
    })
    .await
    .map_err(|e| e.to_string())?
}
/// Tauri's original macOS window is hidden; move the owned native panel.
#[tauri::command]
pub async fn start_capture_drag(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
) -> Result<(), String> {
    if window.label() != "quick-capture" {
        return Err("Only the floating capture panel can be moved here.".into());
    }
    #[cfg(target_os = "macos")]
    {
        if !on_main(&app, vesper_desktop_platform::start_drag).await? {
            return Ok(()); // A quick click may have released before IPC arrived.
        }
        // AppKit hands off to Window Server immediately; mouse-up may never
        // reach the WebView. Wait for release without blocking UI or audio.
        while on_main(&app, vesper_desktop_platform::is_dragging).await? {
            tauri::async_runtime::spawn_blocking(|| {
                std::thread::sleep(Duration::from_millis(50));
            })
            .await
            .map_err(|error| error.to_string())?;
        }
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        window.start_dragging().map_err(|error| error.to_string())
    }
}

/// Apply the persisted output preference to the native activation listener.
pub(crate) fn apply_output(app: &tauri::AppHandle) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        // Read the latest persisted value here so delayed settings callbacks
        // cannot re-enable preparation after a newer save disabled it.
        let enabled = handle
            .state::<AppStorage>()
            .settings
            .load()
            .is_ok_and(|config| config.dictation_output == DictationOutput::Paste);
        vesper_desktop_platform::set_automatic_paste_enabled(enabled);
    });
}

/// Layout geometry only; recording and native focus ownership stay unchanged.
pub(crate) fn apply_capture_design(app: &tauri::AppHandle, design: vesper_core::CaptureDesign) {
    use vesper_core::CaptureDesign;
    let (width, height) = match design {
        CaptureDesign::Split => (396.0, 192.0),
        CaptureDesign::Ribbon => (388.0, 148.0),
        CaptureDesign::Underline => (454.0, 224.0),
        CaptureDesign::Studio => (412.0, 232.0),
        CaptureDesign::Margin => (272.0, 280.0),
        CaptureDesign::Counter => (200.0, 304.0),
    };
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        #[cfg(target_os = "macos")]
        vesper_desktop_platform::resize(width, height);
        #[cfg(not(target_os = "macos"))]
        if let Some(window) = handle.get_webview_window("quick-capture") {
            let _ = window.set_size(tauri::LogicalSize::new(width, height));
        }
        let _ = handle.emit_to("quick-capture", "capture-design-changed", design);
    });
}

fn show(app: &tauri::AppHandle) {
    #[cfg(target_os = "macos")]
    {
        let _ = app.run_on_main_thread(vesper_desktop_platform::show);
    }
    #[cfg(not(target_os = "macos"))]
    if let Some(window) = app.get_webview_window("quick-capture") {
        let _ = window.show();
    }
}
#[tauri::command]
pub fn hide_dictation(app: tauri::AppHandle) {
    #[cfg(target_os = "macos")]
    {
        let _ = app.run_on_main_thread(vesper_desktop_platform::hide);
    }
    #[cfg(not(target_os = "macos"))]
    if let Some(window) = app.get_webview_window("quick-capture") {
        let _ = window.hide();
    }
}
#[tauri::command]
pub fn get_dictation_status(storage: tauri::State<'_, AppStorage>) -> Result<Status, String> {
    Ok(storage
        .dictation
        .state
        .lock()
        .map_err(|e| e.to_string())?
        .status
        .clone())
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Permissions {
    supported: bool,
    accessibility: bool,
    event_posting: bool,
    clipboard: bool,
}
#[tauri::command]
pub async fn dictation_permissions(
    request: bool,
    app: tauri::AppHandle,
) -> Result<Permissions, String> {
    let bits = on_main(&app, move || vesper_desktop_platform::permissions(request)).await?;
    Ok(Permissions {
        supported: bits & 1 != 0,
        accessibility: bits & 2 != 0,
        event_posting: bits & 4 != 0,
        clipboard: bits & 8 != 0,
    })
}
pub fn shortcut(app: &tauri::AppHandle, shortcut: &Shortcut, event: ShortcutState) {
    let storage = app.state::<AppStorage>();
    let Ok(config) = storage.settings.load() else {
        return;
    };
    let keys = config.recording_shortcuts();
    let Some(index) = keys.iter().position(|key| {
        key.parse::<Shortcut>()
            .is_ok_and(|key| key.id() == shortcut.id())
    }) else {
        return;
    };
    let mode = if index == 0 {
        RecordingMode::Hold
    } else {
        RecordingMode::Toggle
    };
    let Ok(mut state) = storage.dictation.state.lock() else {
        return;
    };
    let action = state.key(
        event == ShortcutState::Pressed,
        mode,
        config.dictation_output,
    );
    drop(state);
    dispatch(app, action);
}
pub fn reset_shortcut_keys(app: &tauri::AppHandle) {
    if let Ok(mut state) = app.state::<AppStorage>().dictation.state.lock() {
        if !state.status.phase.active() {
            state.key_down = [false; 2];
        }
    }
}
pub fn require_shortcut_edit_idle(storage: &AppStorage) -> Result<(), String> {
    let state = storage.dictation.state.lock().map_err(|e| e.to_string())?;
    if state.status.phase.active() {
        return Err("Finish the current recording before changing shortcuts.".into());
    }
    Ok(())
}
#[tauri::command]
pub fn toggle_dictation(app: tauri::AppHandle) {
    toggle(&app);
}
pub fn toggle(app: &tauri::AppHandle) {
    let storage = app.state::<AppStorage>();
    let Ok(config) = storage.settings.load() else {
        return;
    };
    let Ok(mut state) = storage.dictation.state.lock() else {
        return;
    };
    let action = state.toggle(RecordingMode::Toggle, config.dictation_output, false);
    drop(state);
    dispatch(app, action);
}
fn dispatch(app: &tauri::AppHandle, action: Action) {
    match action {
        Action::Start(id) => {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                start(app, id).await;
            });
        }
        Action::Stop(id) => {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                stop(app, id).await;
            });
        }
        Action::Cancel => cancel(app),
        Action::None => {}
    }
    emit(app);
}
async fn stop(app: tauri::AppHandle, id: u64) {
    if let Err(error) = commands::stop_recording_inner(app.clone(), app.state(), Some(id)).await {
        // A watcher may already own finalization. Let its operation-scoped
        // completion finish; only recover immediately if there is no work left.
        if app.state::<AppStorage>().runtime.require_idle().is_ok() {
            let id = app
                .state::<AppStorage>()
                .dictation
                .state
                .lock()
                .ok()
                .filter(|state| state.status.id == id && state.status.phase.active())
                .map(|state| state.status.id);
            if let Some(id) = id {
                fail(&app, id, &error);
            }
        }
    }
}
async fn start(app: tauri::AppHandle, id: u64) {
    let storage = app.state::<AppStorage>();
    // A main-window recording can be stopped by Toggle without adopting its
    // destination or turning it into an automatic paste operation.
    if storage.runtime.require_idle().is_err() {
        let recording = storage
            .runtime
            .activity()
            .ok()
            .is_some_and(|s| s["stage"] == "recording");
        let may_stop_existing = storage
            .dictation
            .state
            .lock()
            .is_ok_and(|s| !s.held_session);
        if recording && may_stop_existing {
            if let Ok(mut state) = storage.dictation.state.lock() {
                if state.status.id != id || state.cancelled {
                    return;
                }
                state.adopt_main_recording(storage.runtime.operation_id());
            }
            emit(&app);
            show(&app);
            stop(app.clone(), id).await;
        } else {
            fail(&app, id, "Another operation is running.");
        }
        return;
    }
    let root = storage.dataset_dir.clone();
    let snapshot = tauri::async_runtime::spawn_blocking(move || {
        vesper_data::DatasetStore::open(root)
            .and_then(|store| store.dictation_personalization_snapshot())
            .map_err(|error| error.to_string())
    })
    .await;
    let mut snapshot = match snapshot {
        Ok(Ok(snapshot)) => snapshot,
        error => {
            fail(
                &app,
                id,
                &format!("Could not load project preferences: {error:?}"),
            );
            return;
        }
    };
    let desktop = app.clone();
    let setup = on_main(&app, move || {
        let storage = desktop.state::<AppStorage>();
        let Ok(mut state) = storage.dictation.state.lock() else {
            return None;
        };
        if state.status.id != id || state.status.phase != Phase::Starting || state.cancelled {
            return None;
        }
        if snapshot.profile.project_mode == vesper_core::ProjectMode::Automatic
            && snapshot.profile.active_project.trim().is_empty()
        {
            let application = vesper_desktop_platform::project_application(None);
            let app_id = application
                .as_ref()
                .map(|application| application.id.as_str());
            let needs_document = snapshot.profile.project_rules.iter().any(|rule| {
                rule.enabled
                    && Some(rule.application_id.as_str()) == app_id
                    && rule.folder.is_some()
            });
            let document = app_id
                .filter(|_| needs_document)
                .and_then(vesper_desktop_platform::project_document);
            snapshot.resolve_project(app_id, document.as_deref());
        }
        if state.output == DictationOutput::Paste {
            match vesper_desktop_platform::capture_target(id) {
                Ok(()) => state.target_ready = true,
                Err(message) => state.status.message = message,
            }
        }
        drop(state);
        if storage
            .settings
            .load()
            .is_ok_and(|s| s.floating_bar_enabled)
        {
            show(&desktop);
        }
        Some(snapshot)
    })
    .await;
    let snapshot = match setup {
        Ok(Some(snapshot)) => snapshot,
        Ok(None) => return,
        Err(error) => {
            fail(&app, id, &error);
            return;
        }
    };
    if !start_allowed(&storage, id) {
        fail(&app, id, "Recording cancelled.");
        return;
    }
    // Escape is scoped to a dictation session, not a permanent global binding.
    let _ = app.global_shortcut().register("Escape");
    let result =
        commands::start_recording_inner(None, app.clone(), app.state(), Some(id), Some(snapshot))
            .await;
    match result {
        Ok(_) => {
            let action = storage
                .dictation
                .state
                .lock()
                .map(|mut s| s.ready(id))
                .unwrap_or(Action::None);
            dispatch(&app, action);
        }
        Err(error) => fail(&app, id, &error),
    }
}
pub fn start_allowed(storage: &AppStorage, id: u64) -> bool {
    storage
        .dictation
        .state
        .lock()
        .is_ok_and(|s| s.may_start_microphone(id))
}
pub fn bind(storage: &AppStorage, id: u64, operation: &str) -> bool {
    let Ok(mut state) = storage.dictation.state.lock() else {
        return false;
    };
    if state.status.id != id || state.cancelled {
        return false;
    }
    state.status.operation_id = Some(operation.into());
    true
}
fn fail(app: &tauri::AppHandle, id: u64, error: &str) {
    let storage = app.state::<AppStorage>();
    if let Ok(mut state) = storage.dictation.state.lock() {
        if state.status.id != id {
            return;
        }
        state.status.phase = if state.cancelled {
            Phase::Cancelled
        } else {
            Phase::Error
        };
        state.status.message = error.into();
    }
    let _ = app.global_shortcut().unregister("Escape");
    let _ = app.run_on_main_thread(move || vesper_desktop_platform::clear_target(id));
    show(app);
    emit(app);
}
/// Called synchronously before native cancellation; this suppresses insertion
/// even when recognition/writing cannot return immediately.
pub fn suppress(storage: &AppStorage) {
    if let Ok(mut state) = storage.dictation.state.lock() {
        if state.status.phase.active() {
            state.cancelled = true;
        }
    }
}
#[tauri::command]
pub fn cancel_dictation(app: tauri::AppHandle) {
    cancel(&app);
}
pub fn cancel(app: &tauri::AppHandle) {
    let storage = app.state::<AppStorage>();
    let id = {
        let Ok(mut state) = storage.dictation.state.lock() else {
            return;
        };
        if !state.status.phase.active() {
            return;
        }
        state.cancelled = true;
        state.status.id
    };
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let _ = commands::cancel_operation_inner(app.clone(), app.state(), Some(id)).await;
        fail(&app, id, "Recording cancelled.");
    });
}
pub fn matches_operation(storage: &AppStorage, id: u64) -> bool {
    storage.dictation.state.lock().is_ok_and(|state| {
        state.status.id == id
            && state.status.phase.active()
            && state.status.operation_id.is_some()
            && state.status.operation_id == storage.runtime.operation_id()
    })
}
/// Runs before runtime.finish, so another operation cannot begin while a paste
/// is pending. Every native completion path calls this with its own operation ID.
pub async fn completed(
    app: &tauri::AppHandle,
    operation: &str,
    result: &Result<PipelineResult, String>,
) {
    let storage = app.state::<AppStorage>();
    let (id, paste, text) = {
        let Ok(mut state) = storage.dictation.state.lock() else {
            return;
        };
        if state.status.operation_id.as_deref() != Some(operation) || !state.status.phase.active() {
            return;
        }
        if state.cancelled || storage.runtime.cancelled.load(Ordering::Relaxed) {
            state.status.phase = Phase::Cancelled;
            state.status.message = "Recording cancelled.".into();
            (state.status.id, false, None)
        } else {
            match result {
                Ok(result) => {
                    let text = result
                        .personalized_transcript
                        .as_deref()
                        .or(result.cleaned_transcript.as_deref())
                        .unwrap_or(&result.raw_transcript)
                        .trim()
                        .to_owned();
                    let paste = state.output == DictationOutput::Paste
                        && state.target_ready
                        && !text.is_empty();
                    state.status.text = Some(text.clone());
                    state.status.stop_to_final_ms =
                        state.released.map(|t| t.elapsed().as_millis() as u64);
                    state.status.phase = if paste { Phase::Pasting } else { Phase::Ready };
                    if !paste && state.output == DictationOutput::App {
                        state.status.message =
                            "Kept in app. Check Quick capture settings for automatic paste.".into();
                    }
                    (state.status.id, paste, Some(text))
                }
                Err(error) => {
                    state.status.phase = Phase::Error;
                    state.status.message = error.clone();
                    (state.status.id, false, None)
                }
            }
        }
    };
    emit(app);
    if paste {
        let began = Instant::now();
        let text = text.unwrap_or_default();
        let external = match result {
            Ok(result) => crate::external_observation::reserve(app, result, &text).await,
            Err(_) => None,
        };
        let outcome = loop {
            let desktop = app.clone();
            let text = text.clone();
            let external = external.clone();
            let outcome = on_main(app, move || {
                let storage = desktop.state::<AppStorage>();
                // Serialize cancellation vs the actual dispatch, not just a
                // preflight in another thread. The lock is held only for this call.
                let Ok(state) = storage.dictation.state.lock() else {
                    return vesper_desktop_platform::PasteResult::Unavailable;
                };
                if state.cancelled
                    || state.status.id != id
                    || state.status.phase != Phase::Pasting
                    || storage.runtime.cancelled.load(Ordering::Relaxed)
                {
                    return vesper_desktop_platform::PasteResult::Unavailable;
                }
                if let Some(receipt) = external {
                    crate::external_observation::arm(&desktop, id, receipt);
                }
                vesper_desktop_platform::paste(id, &text)
            })
            .await
            .unwrap_or(vesper_desktop_platform::PasteResult::Unavailable);
            if outcome != vesper_desktop_platform::PasteResult::ModifiersHeld
                || began.elapsed() > Duration::from_secs(3)
            {
                break outcome;
            }
            tauri::async_runtime::spawn_blocking(|| std::thread::sleep(Duration::from_millis(25)))
                .await
                .ok();
        };
        if let Ok(mut state) = storage.dictation.state.lock() {
            if state.status.id == id {
                use vesper_desktop_platform::PasteResult;
                state.status.phase = if state.cancelled {
                    Phase::Cancelled
                } else if outcome == PasteResult::Sent {
                    Phase::Sent
                } else {
                    Phase::Ready
                };
                state.status.final_to_dispatch_ms =
                    (outcome == PasteResult::Sent).then(|| began.elapsed().as_millis() as u64);
                state.status.message = match outcome {
                    PasteResult::Sent => "Paste sent. Your transcript is also available to copy.",
                    PasteResult::TargetChanged => {
                        "The text field changed. Copy your transcript when ready."
                    }
                    PasteResult::ClipboardUnavailable => {
                        "Clipboard unavailable. Your transcript is ready to copy."
                    }
                    PasteResult::ModifiersHeld => {
                        "Release the shortcut keys, then copy your transcript."
                    }
                    PasteResult::Unavailable => {
                        "Automatic paste unavailable. Your transcript is ready to copy."
                    }
                }
                .into();
            }
        }
    }
    let _ = app.global_shortcut().unregister("Escape");
    let _ = on_main(app, move || vesper_desktop_platform::clear_target(id)).await;
    emit(app);
}

#[tauri::command]
pub async fn copy_dictation(id: u64, app: tauri::AppHandle) -> Result<(), String> {
    let desktop = app.clone();
    on_main(&app, move || {
        let storage = desktop.state::<AppStorage>();
        let state = storage.dictation.state.lock().map_err(|e| e.to_string())?;
        if state.status.id != id || !matches!(state.status.phase, Phase::Ready | Phase::Sent) {
            return Err("This transcript is no longer selected. Open History to copy it.".into());
        }
        let text = state
            .status
            .text
            .as_deref()
            .ok_or("No transcript is available.")?;
        if vesper_desktop_platform::copy(text) {
            Ok(())
        } else {
            Err("Clipboard access is unavailable. Open the app to copy your transcript.".into())
        }
    })
    .await?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hold_repeats_and_release_during_startup_finish_only_that_request() {
        let mut state = State::default();
        let Action::Start(id) = state.key(true, RecordingMode::Hold, DictationOutput::Paste) else {
            panic!("start");
        };
        assert!(state.may_start_microphone(id));
        for _ in 0..10 {
            assert_eq!(
                state.key(true, RecordingMode::Hold, DictationOutput::Paste),
                Action::None
            );
        }
        assert_eq!(
            state.key(false, RecordingMode::Hold, DictationOutput::Paste),
            Action::None
        );
        assert!(
            !state.may_start_microphone(id),
            "a released hold cannot open the microphone after a cold load"
        );
        assert_eq!(state.ready(id), Action::Stop(id));
        assert_eq!(
            state.key(false, RecordingMode::Hold, DictationOutput::Paste),
            Action::None
        );
        assert_eq!(state.ready(id), Action::None);
        assert_eq!(state.status.phase, Phase::Finishing);
    }
    #[test]
    fn stopping_a_main_window_recording_never_enables_automatic_paste() {
        let mut state = State {
            output: DictationOutput::Paste,
            target_ready: true,
            ..State::default()
        };
        state.adopt_main_recording(Some("main-operation".into()));
        assert_eq!(state.output, DictationOutput::App);
        assert!(!state.target_ready);
        assert_eq!(state.status.operation_id.as_deref(), Some("main-operation"));
        assert_eq!(state.status.phase, Phase::Finishing);
    }
    #[test]
    fn toggle_ignores_release_and_key_repeat_and_stops_on_next_press() {
        let mut state = State::default();
        let Action::Start(id) = state.key(true, RecordingMode::Toggle, DictationOutput::App) else {
            panic!("start");
        };
        assert_eq!(state.ready(id), Action::None);
        assert_eq!(
            state.key(true, RecordingMode::Toggle, DictationOutput::App),
            Action::None
        );
        assert_eq!(
            state.key(false, RecordingMode::Toggle, DictationOutput::App),
            Action::None
        );
        assert_eq!(state.status.phase, Phase::Recording);
        assert_eq!(
            state.key(true, RecordingMode::Toggle, DictationOutput::App),
            Action::Stop(id)
        );
        assert_eq!(
            state.key(false, RecordingMode::Toggle, DictationOutput::App),
            Action::None
        );
    }
    #[test]
    fn separate_shortcuts_keep_release_and_repeat_state_independent() {
        let mut state = State::default();
        let Action::Start(id) = state.key(true, RecordingMode::Hold, DictationOutput::Paste) else {
            panic!("start");
        };
        state.ready(id);
        // An unrelated or duplicate Toggle release cannot finish Hold.
        assert_eq!(
            state.key(false, RecordingMode::Toggle, DictationOutput::Paste),
            Action::None
        );
        assert_eq!(state.status.phase, Phase::Recording);
        assert_eq!(
            state.key(false, RecordingMode::Hold, DictationOutput::Paste),
            Action::Stop(id)
        );
        assert_eq!(
            state.key(false, RecordingMode::Hold, DictationOutput::Paste),
            Action::None
        );
        state.status.phase = Phase::Ready;
        let Action::Start(next) = state.key(true, RecordingMode::Toggle, DictationOutput::App)
        else {
            panic!("toggle");
        };
        state.ready(next);
        assert_eq!(
            state.key(true, RecordingMode::Hold, DictationOutput::App),
            Action::None
        );
        assert_eq!(
            state.key(false, RecordingMode::Hold, DictationOutput::App),
            Action::None
        );
        assert_eq!(
            state.key(false, RecordingMode::Toggle, DictationOutput::App),
            Action::None
        );
        assert_eq!(
            state.key(true, RecordingMode::Toggle, DictationOutput::App),
            Action::Stop(next)
        );
    }
    #[test]
    fn menu_capture_is_not_stopped_by_releasing_an_unrelated_hold_shortcut() {
        let mut state = State::default();
        let Action::Start(id) = state.toggle(RecordingMode::Hold, DictationOutput::Paste, false)
        else {
            panic!("start");
        };
        state.ready(id);
        assert_eq!(
            state.key(true, RecordingMode::Hold, DictationOutput::Paste),
            Action::None
        );
        assert_eq!(
            state.key(false, RecordingMode::Hold, DictationOutput::Paste),
            Action::None
        );
        assert_eq!(state.status.phase, Phase::Recording);
        assert_eq!(
            state.toggle(RecordingMode::Hold, DictationOutput::Paste, false),
            Action::Stop(id)
        );
    }
    #[test]
    fn cancel_before_microphone_ready_cannot_turn_into_recording_or_stop_to_paste() {
        let mut state = State::default();
        let Action::Start(id) = state.key(true, RecordingMode::Hold, DictationOutput::Paste) else {
            panic!("start");
        };
        state.cancelled = true;
        state.key(false, RecordingMode::Hold, DictationOutput::Paste);
        assert_eq!(state.ready(id), Action::Cancel);
        state.status.phase = Phase::Cancelled;
        let Action::Start(next) = state.key(true, RecordingMode::Hold, DictationOutput::Paste)
        else {
            panic!("restart");
        };
        assert_ne!(id, next);
        assert_eq!(state.ready(id), Action::None);
        assert_eq!(state.status.phase, Phase::Starting);
        assert!(!state.cancelled);
    }
    #[test]
    fn completion_keeps_last_text_until_next_recording_and_pending_delivery_rejects_restarts() {
        let mut state = State::default();
        state.status.phase = Phase::Pasting;
        for _ in 0..3 {
            assert_eq!(
                state.toggle(RecordingMode::Toggle, DictationOutput::App, false),
                Action::None
            );
        }
        state.status.phase = Phase::Sent;
        state.status.text = Some("previous".into());
        assert!(matches!(
            state.toggle(RecordingMode::Toggle, DictationOutput::App, false),
            Action::Start(_)
        ));
        assert!(state.status.text.is_none());
    }
}
