//! Explicit project pickers and transient receiving-app metadata. Main thread only.
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
        fn td_project_document(application: *const c_char, path: *mut c_char, size: usize) -> bool;
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
    pub fn document(application: &str) -> Option<String> {
        let application = CString::new(application).ok()?;
        let mut path = [0; 4097];
        // SAFETY: CString and output array remain valid for this synchronous
        // call; native code validates process identity, thread and output size.
        unsafe { td_project_document(application.as_ptr(), path.as_mut_ptr(), path.len()) }
            .then(|| string(&path))
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
pub fn project_document(application: &str) -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        native::document(application)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = application;
        None
    }
}
