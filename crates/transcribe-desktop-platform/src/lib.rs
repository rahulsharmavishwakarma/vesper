//! Narrow native desktop boundary. Every native entry checks the main thread;
//! borrowed window handles never outlive their owner during initialization.
use raw_window_handle::HasWindowHandle;

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
        pub fn td_target_capture(token: u64) -> i32;
        pub fn td_target_clear(token: u64);
        pub fn td_paste(token: u64, text: *const c_char) -> i32;
        pub fn td_copy(text: *const c_char) -> bool;
    }
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
pub fn capture_target(token: u64) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    // SAFETY: Native code owns all AX references; Rust receives no external handles.
    let code = unsafe { ffi::td_target_capture(token) };
    #[cfg(not(target_os = "macos"))]
    let code = {
        let _ = token;
        -1
    };
    match code {
        0 => Ok(()),
        -1 => Err("Enable Accessibility access in Settings to paste automatically.".into()),
        _ => Err(
            "No supported text field selected. Your transcript will be available to copy.".into(),
        ),
    }
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
