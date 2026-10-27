//! Narrow native desktop boundary. Every native entry checks the main thread;
//! borrowed window handles never outlive their owner during initialization.
use raw_window_handle::HasWindowHandle;
mod background;
mod projects;
pub use background::{
    enter_background_thread, system_conditions, thread_cpu_time, SystemConditions, ThermalState,
};
pub use projects::{
    project_application, project_context, project_document, DocumentContext, DocumentKind,
    ProjectApplication,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PasteResult {
    Sent,
    ModifiersHeld,
    Unavailable,
    TargetChanged,
    ClipboardUnavailable,
}

#[cfg(target_os = "macos")]
mod ffi {
    use std::ffi::{c_char, c_void};
    unsafe extern "C" {
        pub fn td_panel_init(view: *mut c_void) -> bool;
        pub fn td_panel_show();
        pub fn td_panel_start_drag() -> bool;
        pub fn td_panel_is_dragging() -> bool;
        pub fn td_panel_resize(width: f64, height: f64);
        pub fn td_panel_hide();
        pub fn td_panel_destroy();
        pub fn td_permissions(request: bool) -> u32;
        pub fn td_microphone(request: bool) -> i32;
        pub fn td_device(chip: *mut c_char, capacity: usize) -> u64;
        pub fn td_free_space(path: *const c_char) -> i64;
        pub fn td_secure_input() -> bool;
        pub fn td_paste_access_enabled(enabled: bool);
        pub fn td_target_capture(token: u64) -> i32;
        pub fn td_target_clear(token: u64);
        pub fn td_paste(token: u64, text: *const c_char) -> i32;
        pub fn td_copy(text: *const c_char) -> bool;
        pub fn td_target_context(token: u64, limit: usize, output: *mut c_char, size: usize)
            -> i32;
        pub fn td_paste_receipts_enabled(enabled: bool);
        pub fn td_capture_failure(output: *mut c_char, size: usize) -> i32;
        pub fn td_receipt_read(
            token: u64,
            pasted: *mut u64,
            output: *mut c_char,
            size: usize,
        ) -> i32;
    }
}

/// Prepare supported external accessibility providers only for automatic paste.
/// Call on the main thread; disabling removes the activation listener without
/// changing any destination application's accessibility setting.
pub fn set_automatic_paste_enabled(enabled: bool) {
    #[cfg(target_os = "macos")]
    // SAFETY: Scalar preference only; native code checks thread affinity and
    // owns the observer and bounded process cache through shutdown.
    unsafe {
        ffi::td_paste_access_enabled(enabled)
    }
    #[cfg(not(target_os = "macos"))]
    let _ = enabled;
}

pub fn copy(text: &str) -> bool {
    #[cfg(target_os = "macos")]
    {
        let Ok(text) = std::ffi::CString::new(text) else {
            return false;
        };
        // SAFETY: The CString is valid for this call and native code checks the
        // main thread. This is an explicit Copy action, not automatic insertion.
        unsafe { ffi::td_copy(text.as_ptr()) }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = text;
        false
    }
}

pub fn initialize(window: &impl HasWindowHandle) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let handle = window.window_handle().map_err(|e| e.to_string())?;
        let raw_window_handle::RawWindowHandle::AppKit(handle) = handle.as_raw() else {
            return Err("The floating recorder needs a native Mac window.".into());
        };
        // SAFETY: The borrow keeps the NSView owner alive for this call. Native
        // code checks the main thread and retains the original window/content.
        if unsafe { ffi::td_panel_init(handle.ns_view.as_ptr()) } {
            Ok(())
        } else {
            Err("Could not prepare the floating recorder.".into())
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = window;
        Ok(())
    }
}

/// Resize the owned nonactivating panel. Must be called on the main thread.
pub fn resize(width: f64, height: f64) {
    #[cfg(target_os = "macos")]
    // SAFETY: Scalar sizes only; native code validates bounds and thread affinity.
    unsafe {
        ffi::td_panel_resize(width, height)
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (width, height);
}

/// Start a Window Server drag on the owned panel using its retained mouse-down.
/// Returns immediately. Never activates the application or its original window.
pub fn start_drag() -> bool {
    #[cfg(target_os = "macos")]
    // SAFETY: No foreign arguments; native code checks the UI thread and gesture.
    unsafe {
        ffi::td_panel_start_drag()
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

/// Whether the native drag is still held. Must be called on the main thread.
pub fn is_dragging() -> bool {
    #[cfg(target_os = "macos")]
    // SAFETY: No foreign arguments; native code checks the thread and button state.
    unsafe {
        ffi::td_panel_is_dragging()
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

pub fn show() {
    #[cfg(target_os = "macos")]
    // SAFETY: No pointer arguments; native implementation guards main-thread use.
    unsafe {
        ffi::td_panel_show()
    }
}
pub fn hide() {
    #[cfg(target_os = "macos")]
    // SAFETY: No pointer arguments; native implementation guards main-thread use.
    unsafe {
        ffi::td_panel_hide()
    }
}
pub fn shutdown() {
    #[cfg(target_os = "macos")]
    // SAFETY: Restores the retained view before releasing its native panel.
    unsafe {
        ffi::td_panel_destroy()
    }
}
/// Bits: supported, accessibility, event posting, clipboard preservation access.
/// No permission prompts on reads.
pub fn permissions(request: bool) -> u32 {
    #[cfg(target_os = "macos")]
    // SAFETY: Scalar argument; native code checks main-thread use.
    unsafe {
        ffi::td_permissions(request)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = request;
        0
    }
}
/// Microphone access: 0 not asked, 1 restricted, 2 denied, 3 allowed,
/// 4 just asked. `request` shows the system prompt only when not yet asked.
pub fn microphone(request: bool) -> i32 {
    #[cfg(target_os = "macos")]
    // SAFETY: Scalar argument and result.
    unsafe {
        ffi::td_microphone(request)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = request;
        3
    }
}
/// The chip name and physical memory in bytes, when known.
pub fn device() -> (Option<String>, Option<u64>) {
    #[cfg(target_os = "macos")]
    {
        let mut chip = [0 as std::ffi::c_char; 128];
        // SAFETY: The buffer and its capacity are valid for the call; the
        // native side always terminates the string.
        let memory = unsafe { ffi::td_device(chip.as_mut_ptr(), chip.len()) };
        // SAFETY: Terminated above.
        let name = unsafe { std::ffi::CStr::from_ptr(chip.as_ptr()) }
            .to_string_lossy()
            .trim()
            .to_string();
        (
            (!name.is_empty()).then_some(name),
            (memory > 0).then_some(memory),
        )
    }
    #[cfg(not(target_os = "macos"))]
    {
        (None, None)
    }
}
/// Bytes available for new files on the volume holding `path`.
pub fn free_space(path: &std::path::Path) -> Option<u64> {
    #[cfg(target_os = "macos")]
    {
        use std::os::unix::ffi::OsStrExt;
        let path = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
        // SAFETY: A valid, terminated path string for the duration of the call.
        let free = unsafe { ffi::td_free_space(path.as_ptr()) };
        u64::try_from(free).ok()
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = path;
        None
    }
}
pub fn secure_input() -> bool {
    #[cfg(target_os = "macos")]
    // SAFETY: Scalar result; no external handles cross this boundary.
    unsafe {
        ffi::td_secure_input()
    }
    #[cfg(not(target_os = "macos"))]
    false
}
fn capture_message(code: i32) -> Result<(), String> {
    match code {
        0 => Ok(()),
        -1 => Err("Enable Accessibility access in Settings to paste automatically.".into()),
        -4 => Err("A secure field (such as a password prompt) is blocking automatic paste. Dismiss it and try again — your transcript will be available to copy.".into()),
        _ => Err(
            "No supported text field selected. Your transcript will be available to copy.".into(),
        ),
    }
}
/// Why the last `capture_target` found no field: a reason plus the focused
/// element's AX role, subrole and editability flags. Never any text.
pub fn capture_failure() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        // SAFETY: The buffer outlives the synchronous call; native code bounds writes.
        read_native(|out, size| unsafe { ffi::td_capture_failure(out, size) })
    }
    #[cfg(not(target_os = "macos"))]
    None
}

/// Field text the opt-in field-words feature may read: at most this many UTF-16
/// units before the caret of the captured paste target.
pub const FIELD_CONTEXT_CHARS: usize = 4000;

#[cfg(target_os = "macos")]
fn read_native(read: impl FnOnce(*mut std::ffi::c_char, usize) -> i32) -> Option<String> {
    let mut buffer = vec![0 as std::ffi::c_char; 131_073];
    let written = read(buffer.as_mut_ptr(), buffer.len());
    if written < 0 {
        return None;
    }
    // SAFETY: Native code wrote `written` bytes plus a terminator into `buffer`.
    let bytes =
        unsafe { std::slice::from_raw_parts(buffer.as_ptr().cast::<u8>(), written as usize) };
    Some(String::from_utf8_lossy(bytes).into_owned())
}

/// Text before the caret of the field captured for `token` (opt-in field words).
/// `None` for secure fields, cells, a different target or an unreadable field.
pub fn target_context(token: u64) -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        // SAFETY: The buffer outlives the synchronous call; native code bounds writes.
        read_native(|out, size| unsafe {
            ffi::td_target_context(token, FIELD_CONTEXT_CHARS, out, size)
        })
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = token;
        None
    }
}

/// Keep one receipt per paste so its span can be read back once (opt-in).
/// Disabling drops any pending receipt. Call on the main thread.
pub fn set_paste_receipts_enabled(enabled: bool) {
    #[cfg(target_os = "macos")]
    // SAFETY: Scalar preference; native code checks thread affinity.
    unsafe {
        ffi::td_paste_receipts_enabled(enabled)
    }
    #[cfg(not(target_os = "macos"))]
    let _ = enabled;
}

/// Read back the last paste once: (paste token, text from where it began).
/// `token` 0 takes any pending receipt. The receipt is consumed either way.
pub fn take_paste_receipt(token: u64) -> Option<(u64, String)> {
    #[cfg(target_os = "macos")]
    {
        let mut pasted = 0;
        // SAFETY: `pasted` and the buffer outlive the synchronous call.
        let text =
            read_native(|out, size| unsafe { ffi::td_receipt_read(token, &mut pasted, out, size) });
        text.map(|text| (pasted, text))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = token;
        None
    }
}

pub fn capture_target(token: u64) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    // SAFETY: Native code owns all AX references; Rust receives no external handles.
    let code = unsafe { ffi::td_target_capture(token) };
    #[cfg(not(target_os = "macos"))]
    let code = {
        let _ = token;
        -1
    };
    capture_message(code)
}
pub fn clear_target(token: u64) {
    #[cfg(target_os = "macos")]
    // SAFETY: Scalar token; only its matching native target can be released.
    unsafe {
        ffi::td_target_clear(token)
    }
    #[cfg(not(target_os = "macos"))]
    let _ = token;
}
pub fn paste(token: u64, text: &str) -> PasteResult {
    #[cfg(target_os = "macos")]
    {
        let Ok(text) = std::ffi::CString::new(text) else {
            return PasteResult::Unavailable;
        };
        // SAFETY: CString is valid for this call. Native code copies the text and
        // dispatches at most once, only after revalidating its retained target.
        match unsafe { ffi::td_paste(token, text.as_ptr()) } {
            0 => PasteResult::Sent,
            1 => PasteResult::ModifiersHeld,
            -2 => PasteResult::TargetChanged,
            -3 => PasteResult::ClipboardUnavailable,
            _ => PasteResult::Unavailable,
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (token, text);
        PasteResult::Unavailable
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capture_codes_name_the_true_blocker() {
        assert!(capture_message(0).is_ok());
        assert_eq!(
            capture_message(-1).unwrap_err(),
            "Enable Accessibility access in Settings to paste automatically."
        );
        let secure = capture_message(-4).unwrap_err();
        assert!(
            secure.contains("secure field") && !secure.contains("Accessibility"),
            "secure input must not point at Settings: {secure}"
        );
        for code in [-2, -3, -99] {
            assert_eq!(
                capture_message(code).unwrap_err(),
                "No supported text field selected. Your transcript will be available to copy.",
                "{code}"
            );
        }
    }
}
