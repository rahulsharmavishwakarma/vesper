//! Explicit project pickers and transient receiving-app metadata. Main thread only.

/// What the focused window exposes through Accessibility. Transient: callers
/// hash it and drop it; it is never stored or logged.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DocumentKind {
    #[default]
    None,
    /// `value` is an absolute file path (the window's `AXDocument`).
    File,
    /// `value` is an absolute folder path (for example Terminal's working directory).
    Folder,
    /// `value` is a lowercase web host only, never a path or query.
    Web,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DocumentContext {
    pub kind: DocumentKind,
    pub value: String,
}
#[derive(Debug, Clone)]
pub struct ProjectApplication {
    pub id: String,
    pub name: String,
}

#[cfg(target_os = "macos")]
mod native {
    use std::ffi::{c_char, CStr, CString};
    unsafe extern "C" {
        fn td_project_application(
            id: *mut c_char,
            id_size: usize,
            name: *mut c_char,
            name_size: usize,
            path: *const c_char,
        ) -> bool;
        fn td_project_context(
            application: *const c_char,
            web: bool,
            output: *mut c_char,
            size: usize,
        ) -> i32;
    }
    fn string(buffer: &[c_char]) -> String {
        // SAFETY: All callers use zero-initialized buffers; native writes are
        // bounded and include a terminator or leave the buffer untouched.
        unsafe { CStr::from_ptr(buffer.as_ptr()) }
            .to_string_lossy()
            .into_owned()
    }
    pub fn application(path: Option<&str>) -> Option<super::ProjectApplication> {
        let selected = path.map(CString::new).transpose().ok()?;
        let path = selected.as_ref().map_or(std::ptr::null(), |p| p.as_ptr());
        let mut id = [0; 257];
        let mut name = [0; 257];
        // SAFETY: Writable arrays live throughout the synchronous call. Native
        // code checks thread affinity and rejects truncated output.
        unsafe {
            td_project_application(
                id.as_mut_ptr(),
                id.len(),
                name.as_mut_ptr(),
                name.len(),
                path,
            )
        }
        .then(|| super::ProjectApplication {
            id: string(&id),
            name: string(&name),
        })
    }
    pub fn context(application: &str, web: bool) -> super::DocumentContext {
        let none = super::DocumentContext::default();
        let Ok(application) = CString::new(application) else {
            return none;
        };
        let mut value = [0; 4097];
        // SAFETY: CString and output array remain valid for this synchronous
        // call; native code validates process identity, thread and output size.
        let kind = unsafe {
            td_project_context(application.as_ptr(), web, value.as_mut_ptr(), value.len())
        };
        let kind = match kind {
            1 => super::DocumentKind::File,
            2 => super::DocumentKind::Folder,
            3 => super::DocumentKind::Web,
            _ => return none,
        };
        super::DocumentContext {
            kind,
            value: string(&value),
        }
    }
}

pub fn project_application(path: Option<&str>) -> Option<ProjectApplication> {
    #[cfg(target_os = "macos")]
    {
        native::application(path)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = path;
        None
    }
}
/// Read the focused window's document for any frontmost app with bounded
/// Accessibility calls (150 ms app-level, 30 ms per element, 200 ms in all),
/// abstaining on focus changes and secure fields.
/// `web` also reads the focused web area's host (browsers only).
pub fn project_context(application: &str, web: bool) -> DocumentContext {
    #[cfg(target_os = "macos")]
    {
        native::context(application, web)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (application, web);
        DocumentContext::default()
    }
}

/// File-path view of [`project_context`] for folder rules.
pub fn project_document(application: &str) -> Option<String> {
    let context = project_context(application, false);
    matches!(context.kind, DocumentKind::File | DocumentKind::Folder).then_some(context.value)
}
