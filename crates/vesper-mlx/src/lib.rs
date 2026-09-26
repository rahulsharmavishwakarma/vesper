//! Safe, worker-owned binding to the pinned MLX speech bridge.
//! Models are neither Send nor Sync. A call borrows input and cancellation until
//! Swift has joined its task; no callback or audio pointer escapes the call.
use libloading::Library;
use serde::{Deserialize, Serialize};
use std::{
    ffi::{c_char, c_void, CStr, CString},
    marker::PhantomData,
    path::Path,
    ptr::NonNull,
    rc::Rc,
    sync::atomic::{AtomicBool, Ordering},
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("MLX model initialization: {0}")]
    Initialization(String),
    #[error("MLX runtime: {0}")]
    Runtime(String),
    #[error("invalid MLX request: {0}")]
    Input(&'static str),
    #[error("operation cancelled")]
    Cancelled,
    #[error("incomplete MLX transcription was discarded: {0}")]
    Incomplete(String),
}
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Transcript {
    pub schema_version: u32,
    pub text: String,
    pub stop_reason: String,
    pub generation_tokens: usize,
    pub prompt_tokens: usize,
    pub native_seconds: f64,
    #[serde(rename = "peakMemoryGB")]
    pub peak_memory_gb: f64,
    pub provider: String,
    pub runtime_revision: String,
    pub whisper_scores: Option<WhisperScores>,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WhisperScores {
    pub no_speech_probability: f64,
    pub average_log_probability: f64,
}
pub const RUNTIME_REVISION: &str = "d302a5c6080d2bb97bae38c7418f82abb76013b6";

/// Probe only the verified application library, never downloaded model code.
pub fn device_available(library: &Path) -> Result<bool> {
    // SAFETY: caller verifies the application-built image; the symbol is the
    // synchronous, pointer-free Metal availability probe in Bridge.swift.
    unsafe {
        let image = Library::new(library).map_err(runtime)?;
        let probe = image
            .get::<unsafe extern "C" fn() -> bool>(b"transcribe_mlx_device_available\0")
            .map_err(runtime)?;
        Ok(probe())
    }
}

type Load = unsafe extern "C" fn(*const c_char, *const c_char, *mut *mut c_char) -> *mut c_void;
type Unload = unsafe extern "C" fn(*mut c_void);
type Free = unsafe extern "C" fn(*mut c_char);
type Cancel = unsafe extern "C" fn(*mut c_void) -> bool;
type Generate = unsafe extern "C" fn(
    *mut c_void,
    *const f32,
    i64,
    *const c_char,
    Option<Cancel>,
    *mut c_void,
) -> *mut c_char;

pub struct Model {
    handle: NonNull<c_void>,
    unload: Unload,
    free: Free,
    generate: Generate,
    whisper: bool,
    // Function pointers and the opaque model cannot outlive the image.
    _library: Library,
    _worker_owned: PhantomData<Rc<()>>,
}

impl Model {
    /// `library` must point to the trusted application-built bridge, never a
    /// downloaded model repository. The caller verifies model artifacts first.
    pub fn load(library: &Path, directory: &Path, architecture: &str) -> Result<Self> {
        if !library
            .parent()
            .is_some_and(|parent| parent.join("mlx.metallib").is_file())
        {
            return Err(Error::Input(
                "missing colocated mlx.metallib; rebuild the native runtime",
            ));
        }
        if !["cohere", "whisper", "qwen3_asr", "parakeet"].contains(&architecture) {
            return Err(Error::Input("unsupported architecture"));
        }
        let path = CString::new(directory.to_str().ok_or(Error::Input("non-UTF-8 path"))?)
            .map_err(|_| Error::Input("null in path"))?;
        let whisper = architecture == "whisper";
        let architecture = CString::new(architecture).expect("compiled architecture");
        // SAFETY: the caller supplies the application-built library. Symbols
        // below are its versioned ABI, and Library lives through model teardown.
        let library = unsafe { Library::new(library) }.map_err(runtime)?;
        // SAFETY: these signatures exactly match native/mlx/Bridge.swift ABI v1.
        let (load, unload, free, generate) = unsafe {
            let version = library
                .get::<unsafe extern "C" fn() -> u32>(b"transcribe_mlx_abi_version\0")
                .map_err(runtime)?;
            if version() != 1 {
                return Err(Error::Runtime("unsupported bridge ABI".into()));
            }
            (
                *library
                    .get::<Load>(b"transcribe_mlx_load\0")
                    .map_err(runtime)?,
                *library
                    .get::<Unload>(b"transcribe_mlx_unload\0")
                    .map_err(runtime)?,
                *library
                    .get::<Free>(b"transcribe_mlx_free_string\0")
                    .map_err(runtime)?,
                *library
                    .get::<Generate>(b"transcribe_mlx_transcribe\0")
                    .map_err(runtime)?,
            )
        };
        let mut error = std::ptr::null_mut();
        // SAFETY: NUL-terminated strings and error out-pointer live for the call.
        let handle = unsafe { load(path.as_ptr(), architecture.as_ptr(), &mut error) };
        let message = take_string(error, free);
        let handle = NonNull::new(handle).ok_or_else(|| {
            Error::Initialization(message.unwrap_or_else(|| "model initialization failed".into()))
        })?;
        Ok(Self {
            handle,
            unload,
            free,
            generate,
            whisper,
            _library: library,
            _worker_owned: PhantomData,
        })
    }

    /// One bounded 16 kHz mono chunk. The owner handles segmentation, progress
    /// and discarding the entire recording when any chunk fails.
    pub fn transcribe(
        &mut self,
        samples: &[f32],
        language: Option<&str>,
        max_tokens: usize,
        cancelled: &AtomicBool,
    ) -> Result<Transcript> {
        if cancelled.load(Ordering::Acquire) {
            return Err(Error::Cancelled);
        }
        if samples.is_empty() || samples.len() > 480_000 || samples.iter().any(|x| !x.is_finite()) {
            return Err(Error::Input("expected 1..=480000 finite samples at 16 kHz"));
        }
        if !(1..=512).contains(&max_tokens) {
            return Err(Error::Input("token budget out of range"));
        }
        let options = CString::new(
            serde_json::json!({"language":language,"maxTokens":max_tokens}).to_string(),
        )
        .map_err(runtime)?;
        // SAFETY: Model exclusively owns the handle. Input, options and the
        // AtomicBool remain borrowed until Swift has joined all work/callbacks.
        let response = unsafe {
            (self.generate)(
                self.handle.as_ptr(),
                samples.as_ptr(),
                samples.len() as i64,
                options.as_ptr(),
                Some(cancel_callback),
                std::ptr::from_ref(cancelled).cast_mut().cast(),
            )
        };
        let response = take_string(response, self.free)
            .ok_or_else(|| Error::Runtime("missing response".into()))?;
        if cancelled.load(Ordering::Acquire) {
            return Err(Error::Cancelled);
        }
        let mut result = completed(&response)?;
        if self.whisper {
            let scores = result
                .whisper_scores
                .as_ref()
                .ok_or_else(|| Error::Runtime("missing Whisper confidence report".into()))?;
            let silence = vesper_core::whisper_no_speech(
                scores.no_speech_probability,
                scores.average_log_probability,
            )
            .ok_or_else(|| Error::Runtime("invalid Whisper confidence report".into()))?;
            if silence {
                result.text.clear();
            }
        }
        Ok(result)
    }
}

impl Drop for Model {
    fn drop(&mut self) {
        // SAFETY: the exclusively owned retained handle is released exactly once,
        // before the library field is dropped. No call can overlap this drop.
        unsafe { (self.unload)(self.handle.as_ptr()) };
    }
}

unsafe extern "C" fn cancel_callback(context: *mut c_void) -> bool {
    // SAFETY: only transcribe supplies this callback; its borrowed AtomicBool
    // remains alive until the native task and every callback have finished.
    unsafe { (&*context.cast::<AtomicBool>()).load(Ordering::Acquire) }
}

fn take_string(pointer: *mut c_char, free: Free) -> Option<String> {
    let pointer = NonNull::new(pointer)?;
    // SAFETY: the pinned ABI returns either NULL or a strdup-owned terminated
    // string. Copy before using its matching allocator to free it once.
    let value = unsafe {
        CStr::from_ptr(pointer.as_ptr())
            .to_string_lossy()
            .into_owned()
    };
    // SAFETY: pointer was allocated by this library and has not been released.
    unsafe { free(pointer.as_ptr()) };
    Some(value)
}

fn runtime(error: impl std::fmt::Display) -> Error {
    Error::Runtime(error.to_string())
}

fn completed(response: &str) -> Result<Transcript> {
    let value: serde_json::Value = serde_json::from_str(response).map_err(runtime)?;
    if let Some(error) = value.get("error").and_then(|v| v.as_str()) {
        return Err(if error == "cancelled" {
            Error::Cancelled
        } else {
            Error::Runtime(error.into())
        });
    }
    let result: Transcript = serde_json::from_value(value).map_err(runtime)?;
    if result.schema_version != 1
        || result.provider != "metal"
        || result.runtime_revision != RUNTIME_REVISION
    {
        return Err(Error::Runtime("incompatible runtime evidence".into()));
    }
    match result.stop_reason.as_str() {
        "eos" | "audio_consumed" => Ok(result),
        "cancelled" => Err(Error::Cancelled),
        _ => Err(Error::Incomplete(result.stop_reason)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires the pinned built bridge, verified model and two public audio files"]
    fn installed_model_isolation_cancellation_and_completion() {
        let path = |key| std::path::PathBuf::from(std::env::var_os(key).expect(key));
        let library = path("TRANSCRIBE_MLX_LIB");
        let directory = path("TRANSCRIBE_MLX_MODEL");
        let architecture = std::env::var("TRANSCRIBE_MLX_ARCH").expect("TRANSCRIBE_MLX_ARCH");
        let language = std::env::var("TRANSCRIBE_MLX_LANGUAGE").unwrap_or_else(|_| "en".into());
        let a = vesper_audio::decode_file(path("TRANSCRIBE_SMOKE_AUDIO"), 0).unwrap();
        let b = vesper_audio::decode_file(path("TRANSCRIBE_SMOKE_AUDIO_B"), 0).unwrap();
        assert!(a.samples().len() <= 480_000 && b.samples().len() <= 480_000);
        let flag = AtomicBool::new(false);
        let mut model = Model::load(&library, &directory, &architecture).unwrap();
        let first = model
            .transcribe(a.samples(), Some(&language), 512, &flag)
            .unwrap()
            .text;
        assert!(!first.trim().is_empty());
        let other = model
            .transcribe(b.samples(), Some(&language), 512, &flag)
            .unwrap()
            .text;
        assert_ne!(first, other);
        assert_eq!(
            model
                .transcribe(a.samples(), Some(&language), 512, &flag)
                .unwrap()
                .text,
            first
        );
        if architecture != "parakeet" {
            assert!(matches!(
                model.transcribe(a.samples(), Some(&language), 1, &flag),
                Err(Error::Incomplete(_))
            ));
        }
        std::thread::scope(|scope| {
            scope.spawn(|| {
                std::thread::sleep(std::time::Duration::from_millis(5));
                flag.store(true, Ordering::Release);
            });
            assert!(matches!(
                model.transcribe(a.samples(), Some(&language), 512, &flag),
                Err(Error::Cancelled)
            ));
        });
        flag.store(false, Ordering::Release);
        assert_eq!(
            model
                .transcribe(a.samples(), Some(&language), 512, &flag)
                .unwrap()
                .text,
            first
        );
        assert!(model
            .transcribe(&[f32::NAN], Some(&language), 512, &flag)
            .is_err());
        // Tiny finite audio must return through the ABI without an STFT shape abort.
        let _ = model.transcribe(&[0.001], Some(&language), 512, &flag);
        drop(model);
        let mut model = Model::load(&library, &directory, &architecture).unwrap();
        assert_eq!(
            model
                .transcribe(a.samples(), Some(&language), 512, &flag)
                .unwrap()
                .text,
            first
        );
    }
    #[test]
    fn refuses_limits_repetition_missing_or_incompatible_evidence() {
        let base = serde_json::json!({"schemaVersion":1,"text":"partial","stopReason":"eos","generationTokens":2,"promptTokens":4,"nativeSeconds":0.1,"peakMemoryGB":0.1,"provider":"metal","runtimeRevision":RUNTIME_REVISION});
        for reason in [
            "eos",
            "audio_consumed",
            "max_tokens",
            "context_limit",
            "repetition",
            "cancelled",
            "unverified",
        ] {
            let mut value = base.clone();
            value["stopReason"] = reason.into();
            assert_eq!(
                completed(&value.to_string()).is_ok(),
                ["eos", "audio_consumed"].contains(&reason)
            );
        }
        for key in ["stopReason", "runtimeRevision", "provider", "schemaVersion"] {
            let mut value = base.clone();
            value.as_object_mut().unwrap().remove(key);
            assert!(completed(&value.to_string()).is_err());
        }
        let mut value = base;
        value["runtimeRevision"] = "unpatched".into();
        assert!(completed(&value.to_string()).is_err());
        assert!(completed(r#"{"schemaVersion":1,"error":"no model"}"#).is_err());
    }
}
