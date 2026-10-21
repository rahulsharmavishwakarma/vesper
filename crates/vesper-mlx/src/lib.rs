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
    #[serde(default)]
    pub resources: Option<serde_json::Value>,
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
            .get::<unsafe extern "C" fn() -> bool>(b"vesper_mlx_device_available\0")
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

type LiveOpen = unsafe extern "C" fn(*mut c_void, *mut *mut c_char) -> *mut c_void;
type LiveFeed = unsafe extern "C" fn(*mut c_void, *const f32, i64) -> *mut c_char;
type LivePoll = unsafe extern "C" fn(*mut c_void) -> *mut c_char;
type LiveClose = unsafe extern "C" fn(*mut c_void);

struct LiveState {
    handle: NonNull<c_void>,
    feed: LiveFeed,
    poll: LivePoll,
    close: LiveClose,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveUpdate {
    pub text: Option<String>,
    pub input_seconds: f32,
    /// Input skipped under decoder backlog; absent from older bridges.
    #[serde(default)]
    pub dropped_seconds: f32,
}

pub struct Model {
    live: Option<LiveState>,
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
        if ![
            "cohere",
            "whisper",
            "qwen3_asr",
            "parakeet",
            "granite_speech5_ctc",
            "phonon2",
            "parakeet_redux",
        ]
        .contains(&architecture)
        {
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
                .get::<unsafe extern "C" fn() -> u32>(b"vesper_mlx_abi_version\0")
                .map_err(runtime)?;
            if version() != 1 {
                return Err(Error::Runtime("unsupported bridge ABI".into()));
            }
            (
                *library.get::<Load>(b"vesper_mlx_load\0").map_err(runtime)?,
                *library
                    .get::<Unload>(b"vesper_mlx_unload\0")
                    .map_err(runtime)?,
                *library
                    .get::<Free>(b"vesper_mlx_free_string\0")
                    .map_err(runtime)?,
                *library
                    .get::<Generate>(b"vesper_mlx_transcribe\0")
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
            live: None,
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
        if self.live.is_some() {
            return Err(Error::Input(
                "close live input before completed transcription",
            ));
        }
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

impl Model {
    pub fn begin_live(&mut self) -> Result<()> {
        if self.live.is_some() {
            return Err(Error::Input("live session already exists"));
        }
        // SAFETY: these optional symbols come from the same verified bridge as
        // the model. The session is closed/joined before model or image release.
        let (open, feed, poll, close) = unsafe {
            (
                *self
                    ._library
                    .get::<LiveOpen>(b"vesper_mlx_live_open\0")
                    .map_err(runtime)?,
                *self
                    ._library
                    .get::<LiveFeed>(b"vesper_mlx_live_feed\0")
                    .map_err(runtime)?,
                *self
                    ._library
                    .get::<LivePoll>(b"vesper_mlx_live_poll\0")
                    .map_err(runtime)?,
                *self
                    ._library
                    .get::<LiveClose>(b"vesper_mlx_live_close\0")
                    .map_err(runtime)?,
            )
        };
        let mut error = std::ptr::null_mut();
        // SAFETY: worker exclusively owns the verified model and the out-pointer.
        let handle = unsafe { open(self.handle.as_ptr(), &mut error) };
        let message = take_string(error, self.free);
        let handle = NonNull::new(handle).ok_or_else(|| {
            Error::Runtime(message.unwrap_or_else(|| "live session initialization failed".into()))
        })?;
        self.live = Some(LiveState {
            handle,
            feed,
            poll,
            close,
        });
        Ok(())
    }

    /// Returns the number of samples the native session skipped under backlog.
    pub fn feed_live(&mut self, samples: &[f32]) -> Result<u64> {
        if samples.is_empty() {
            return Ok(0);
        }
        if samples.len() > 16000 || samples.iter().any(|s| !s.is_finite()) {
            return Err(Error::Input("invalid live PCM packet"));
        }
        let live = self
            .live
            .as_ref()
            .ok_or(Error::Input("live session not started"))?;
        // SAFETY: Swift copies the finite slice during this synchronous call.
        let response =
            unsafe { (live.feed)(live.handle.as_ptr(), samples.as_ptr(), samples.len() as i64) };
        self.live_response(response)
            .map(|value| value["droppedSamples"].as_u64().unwrap_or(0))
    }

    pub fn poll_live(&mut self) -> Result<LiveUpdate> {
        let live = self
            .live
            .as_ref()
            .ok_or(Error::Input("live session not started"))?;
        // SAFETY: exclusive worker access; returned allocation uses bridge free.
        let response = unsafe { (live.poll)(live.handle.as_ptr()) };
        serde_json::from_value(self.live_response(response)?).map_err(runtime)
    }

    fn live_response(&self, response: *mut c_char) -> Result<serde_json::Value> {
        let text = take_string(response, self.free)
            .ok_or_else(|| Error::Runtime("missing live response".into()))?;
        let value: serde_json::Value = serde_json::from_str(&text).map_err(runtime)?;
        if let Some(error) = value["error"].as_str() {
            return Err(Error::Runtime(error.into()));
        }
        Ok(value)
    }

    pub fn close_live(&mut self) {
        if let Some(live) = self.live.take() {
            // SAFETY: close cancels AND awaits native work/collector before
            // returning. No in-flight task can outlive this owner or its image.
            unsafe { (live.close)(live.handle.as_ptr()) };
        }
    }
}

impl Drop for Model {
    fn drop(&mut self) {
        self.close_live();
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

fn runtime(error: impl std::error::Error) -> Error {
    // libloading keeps the dyld/dlerror text in `source()`; Display alone is
    // only "dlopen failed", which hid the real cause (#245).
    let mut text = error.to_string();
    let mut source = error.source();
    while let Some(next) = source {
        text.push_str(": ");
        text.push_str(&next.to_string());
        source = next.source();
    }
    Error::Runtime(text)
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
        let path =
            |key| std::path::PathBuf::from(vesper_core::environment::var_os(key).expect(key));
        let library = path("VESPER_MLX_LIB");
        let directory = path("VESPER_MLX_MODEL");
        let architecture =
            vesper_core::environment::var("VESPER_MLX_ARCH").expect("VESPER_MLX_ARCH");
        let language =
            vesper_core::environment::var("VESPER_MLX_LANGUAGE").unwrap_or_else(|_| "en".into());
        let a = vesper_audio::decode_file(path("VESPER_SMOKE_AUDIO"), 0).unwrap();
        let b = vesper_audio::decode_file(path("VESPER_SMOKE_AUDIO_B"), 0).unwrap();
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
        // Non-autoregressive decoders consume the whole input without a token budget.
        if ![
            "parakeet",
            "granite_speech5_ctc",
            "phonon2",
            "parakeet_redux",
        ]
        .contains(&architecture.as_str())
        {
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

#[cfg(test)]
mod load_error_tests {
    use super::*;

    #[test]
    fn library_load_failure_reports_the_system_reason() {
        let missing = Path::new("/nonexistent/vesper-mlx-test/libVesperMLX.dylib");
        let message = device_available(missing).unwrap_err().to_string();
        assert!(message.starts_with("MLX runtime: "), "{message}");
        // The bare libloading Display is only "dlopen failed"; the path and the
        // system text come from the error's source chain.
        assert!(message.contains("vesper-mlx-test"), "{message}");
        assert!(
            message.len() > "MLX runtime: dlopen failed".len(),
            "{message}"
        );
    }
}
