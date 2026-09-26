//! Owned text only crosses the native boundary; AX objects, offsets and guards do not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalEdit {
    pub before: String,
    pub after: String,
}
#[cfg(target_os = "macos")]
type Callback = Box<dyn FnOnce(ExternalEdit) + Send>;
#[cfg(target_os = "macos")]
static CALLBACK: std::sync::Mutex<Option<(u64, Callback)>> = std::sync::Mutex::new(None);
#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn td_observation_arm(
        token: u64,
        callback: extern "C" fn(u64, *const std::ffi::c_char, *const std::ffi::c_char),
    );
    fn td_observation_cancel();
    #[cfg(test)]
    fn td_observation_decide(
        ack: bool,
        before: *const std::ffi::c_char,
        window: *const std::ffi::c_char,
        left: *const std::ffi::c_char,
        right: *const std::ffi::c_char,
        offset: i64,
        selection_start: i64,
        selection_length: i64,
        elapsed: f64,
        focus: bool,
        stable: bool,
    ) -> i32;
    #[cfg(test)]
    fn td_observation_validate(
        window: *const std::ffi::c_char,
        left: *const std::ffi::c_char,
        right: *const std::ffi::c_char,
        expected: *const std::ffi::c_char,
    ) -> bool;
}
#[cfg(target_os = "macos")]
extern "C" fn receive(token: u64, before: *const std::ffi::c_char, after: *const std::ffi::c_char) {
    // No Rust panic may cross the native callback boundary.
    let _ = std::panic::catch_unwind(|| {
        let callback = CALLBACK.lock().ok().and_then(|mut c| {
            if c.as_ref().is_some_and(|(id, _)| *id == token) {
                c.take().map(|(_, c)| c)
            } else {
                None
            }
        });
        if before.is_null() || after.is_null() {
            return;
        }
        // SAFETY: Native supplies NUL-terminated NSString UTF8String buffers valid
        // for this synchronous callback. No pointer is retained after copying.
        let (before, after) = unsafe {
            (
                std::ffi::CStr::from_ptr(before),
                std::ffi::CStr::from_ptr(after),
            )
        };
        let (Ok(before), Ok(after)) = (before.to_str(), after.to_str()) else {
            return;
        };
        if before.encode_utf16().count() > 8192
            || after.encode_utf16().count() > 8192
            || before == after
            || before.is_empty()
            || after.is_empty()
        {
            return;
        }
        if let Some(callback) = callback {
            callback(ExternalEdit {
                before: before.into(),
                after: after.into(),
            });
        }
    });
}
/// Main-thread only. Arm one optional observation for the immediately following paste.
pub fn observe_next_paste(token: u64, callback: impl FnOnce(ExternalEdit) + Send + 'static) {
    #[cfg(target_os = "macos")]
    {
        cancel_observation();
        if let Ok(mut slot) = CALLBACK.lock() {
            *slot = Some((token, Box::new(callback)));
        }
        // SAFETY: Static callback entry; native owns all AX state and checks main thread.
        unsafe {
            td_observation_arm(token, receive);
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (token, callback);
}
pub fn cancel_observation() {
    #[cfg(target_os = "macos")]
    {
        // SAFETY: Native cancellation checks thread affinity, releases its receipt
        // and synchronously releases the matching callback with null text pointers.
        unsafe {
            td_observation_cancel();
        }
        if let Ok(mut slot) = CALLBACK.lock() {
            slot.take();
        }
    }
}
#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    fn valid(window: &str, left: &str, right: &str, expected: &str) -> bool {
        let args = [window, left, right, expected].map(|s| std::ffi::CString::new(s).unwrap());
        // SAFETY: Pure Foundation function, no AX/GUI access. All strings live through call.
        unsafe {
            td_observation_validate(
                args[0].as_ptr(),
                args[1].as_ptr(),
                args[2].as_ptr(),
                args[3].as_ptr(),
            )
        }
    }
    #[test]
    fn native_transition_gates_acknowledgement_settling_focus_timeout_and_selection() {
        let before = std::ffi::CString::new("read me").unwrap();
        let left = std::ffi::CString::new("[ ").unwrap();
        let right = std::ffi::CString::new(" ]").unwrap();
        // ack, window, selection start/length, elapsed, focused, stable, expected
        let cases = [
            (false, "[ read me ]", 9, 0, 0.1, true, false, 1),
            (false, "[ other ]", 7, 0, 0.1, true, false, 0),
            (false, "[ read me ]", 9, 0, 2.1, true, false, -1),
            (true, "[ read me ]", 9, 0, 3.0, true, false, 0),
            (true, "[ README ]", 8, 0, 3.0, true, false, 2),
            (true, "[ README ]", 8, 0, 3.3, true, true, 3),
            (true, "[ README ]", 8, 0, 60.1, true, true, -1),
            (true, "[ README ]", 8, 0, 3.3, false, true, -1),
            (true, "[ README ]", 1, 0, 3.3, true, true, -1),
            (true, "[ README ]", 9, 0, 3.3, true, true, -1),
            (true, "[ README ]", 3, 1, 3.3, true, true, -1),
            (true, "Z README ]", 8, 0, 3.3, true, true, -1),
            (true, "[ README [  ]", 8, 0, 3.3, true, true, -1),
            (true, "[  ]", 2, 0, 3.3, true, true, -1),
        ];
        for (ack, window, start, length, elapsed, focus, stable, expected) in cases {
            let value = std::ffi::CString::new(window).unwrap();
            // SAFETY: Pure bounded transition function. All pointers are live CStrings;
            // scalar offsets are checked natively and no AX objects are accessed.
            let actual = unsafe {
                td_observation_decide(
                    ack,
                    before.as_ptr(),
                    value.as_ptr(),
                    left.as_ptr(),
                    right.as_ptr(),
                    2,
                    start,
                    length,
                    elapsed,
                    focus,
                    stable,
                )
            };
            assert_eq!(actual, expected, "{window}, ack={ack}, focus={focus}");
        }
    }
    #[test]
    fn bounded_native_matcher_preserves_utf16_unicode_and_rejects_lost_or_repeated_guards() {
        for text in ["README", "🧑🏽‍💻 café", "नमस्ते दुनिया", "e\u{301}", "👩‍👩‍👧‍👦", "中文"]
        {
            assert!(valid(
                &format!("LEFT {text} RIGHT"),
                "LEFT ",
                " RIGHT",
                text
            ));
            assert!(valid(text, "", "", text));
        }
        for (window, left, right) in [
            ("other text RIGHT", "LEFT ", " RIGHT"),
            ("aa TEXT aa", "aa", "aa"),
            ("aaaTEXT!", "aa", "!"),
            ("L text R lost", "L ", " R"),
        ] {
            assert!(!valid(window, left, right, "text"));
        }
        assert!(!valid(&"x".repeat(8193), "", "", &"x".repeat(8193)));
        assert!(!valid(&"😀".repeat(4097), "", "", &"😀".repeat(4097)));
        assert!(valid(&"😀".repeat(4096), "", "", &"😀".repeat(4096)));
    }
}
