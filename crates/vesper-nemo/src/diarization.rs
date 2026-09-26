//! Standalone ABI at NVIDIA/NeMo-Speech.cpp 97a15afa5caa9bce5baaa86c1184103877af4101.
//! A model and fresh stream are owned by the calling inference worker for one job.
use crate::{Error, Result};
use libloading::Library;
use std::{
    ffi::{c_char, CStr, CString},
    path::Path,
    ptr,
};

#[repr(C)]
struct Model {
    _private: [u8; 0],
}
#[repr(C)]
struct Stream {
    _private: [u8; 0],
}
#[repr(C)]
struct Config {
    size: usize,
    model_path: *const c_char,
    gpu: i32,
    preset: *const c_char,
    chunk: i32,
    right: i32,
    left: i32,
    fifo: i32,
    cache: i32,
    period: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Segment {
    pub start: f64,
    pub end: f64,
    pub speaker: i32,
}

type Create = unsafe extern "C" fn(*const Config, *mut *mut Model) -> i32;
type Destroy = unsafe extern "C" fn(*mut Model);
type Open = unsafe extern "C" fn(*mut Model, *mut *mut Stream) -> i32;
type Close = unsafe extern "C" fn(*mut Stream);
type Push = unsafe extern "C" fn(*mut Stream, *const f32, usize, i32) -> i32;
type Finish = unsafe extern "C" fn(*mut Stream) -> i32;
type Segments =
    unsafe extern "C" fn(*const Stream, *const (), *mut Segment, usize, *mut usize) -> i32;
type LastError = unsafe extern "C" fn() -> *const c_char;

pub struct Diarizer {
    model: *mut Model,
    destroy: Destroy,
    open: Open,
    close: Close,
    push: Push,
    finish: Finish,
    segments: Segments,
    last_error: LastError,
    _library: Library,
}

impl Diarizer {
    pub fn load(library_path: &Path, model_path: &Path, gpu: i32) -> Result<Self> {
        let path = CString::new(model_path.to_string_lossy().as_bytes())
            .map_err(|_| Error::InvalidCString)?;
        // SAFETY: callers verify the pinned runtime bundle before this call. The
        // handle owns all resolved function pointers through the final model drop.
        #[cfg(unix)]
        let loaded = unsafe {
            libloading::os::unix::Library::open(
                Some(library_path),
                libloading::os::unix::RTLD_NOW | libloading::os::unix::RTLD_LOCAL,
            )
        }
        .map(Library::from);
        #[cfg(not(unix))]
        // SAFETY: same pinned ownership contract as the Unix load above.
        let loaded = unsafe { Library::new(library_path) };
        let library = loaded.map_err(|e| Error::Load {
            path: library_path.into(),
            message: e.to_string(),
        })?;
        macro_rules! symbol {
            ($name:literal, $ty:ty) => {{
                // SAFETY: signatures match the pinned official diar.h; the
                // Library outlives every copied function pointer.
                unsafe {
                    *library
                        .get::<$ty>(concat!($name, "\0").as_bytes())
                        .map_err(|e| Error::Symbol {
                            symbol: $name,
                            message: e.to_string(),
                        })?
                }
            }};
        }
        let create = symbol!("nemo_speech_diar_create", Create);
        let mut this = Self {
            model: ptr::null_mut(),
            destroy: symbol!("nemo_speech_diar_destroy", Destroy),
            open: symbol!("nemo_speech_diar_stream_open", Open),
            close: symbol!("nemo_speech_diar_stream_close", Close),
            push: symbol!("nemo_speech_diar_stream_push_f32", Push),
            finish: symbol!("nemo_speech_diar_stream_finish", Finish),
            segments: symbol!("nemo_speech_diar_segments", Segments),
            last_error: symbol!("nemo_speech_asr_last_error", LastError),
            _library: library,
        };
        let config = Config {
            size: std::mem::size_of::<Config>(),
            model_path: path.as_ptr(),
            gpu,
            preset: ptr::null(),
            chunk: 0,
            right: 0,
            left: -1,
            fifo: 0,
            cache: 0,
            period: 0,
        };
        // SAFETY: initialized size-prefixed config and live path; native code
        // copies metadata, returning an exclusively owned opaque model.
        let status = unsafe { create(&config, &mut this.model) };
        this.check(status, "create diarizer")?;
        if this.model.is_null() {
            return Err(error("create diarizer", "null model"));
        }
        Ok(this)
    }

    fn check(&self, status: i32, stage: &'static str) -> Result<()> {
        if status == 0 {
            return Ok(());
        }
        // SAFETY: last_error returns a thread-local, null-terminated borrowed
        // string, copied immediately while this library is still loaded.
        let message = unsafe {
            let p = (self.last_error)();
            if p.is_null() {
                "unknown native error".into()
            } else {
                CStr::from_ptr(p).to_string_lossy().into_owned()
            }
        };
        Err(Error::Native { stage, message })
    }

    pub fn diarize(
        &mut self,
        samples: &[f32],
        sample_rate: u32,
        cancelled: impl Fn() -> bool,
    ) -> Result<Vec<Segment>> {
        if sample_rate != 16000 || samples.iter().any(|s| !s.is_finite()) {
            return Err(error("diarize", "expected finite mono 16 kHz samples"));
        }
        if cancelled() {
            return Err(error("diarize", "cancelled"));
        }
        let mut stream = ptr::null_mut();
        self.check(
            // SAFETY: valid model and writable output; a fresh stream never shares
            // recording state. Guard closes it before the borrowed model is dropped.
            unsafe { (self.open)(self.model, &mut stream) },
            "open diarizer",
        )?;
        if stream.is_null() {
            return Err(error("open diarizer", "null stream"));
        }
        let guard = StreamGuard {
            stream,
            owner: self,
        };
        for packet in samples.chunks(5120) {
            if cancelled() {
                return Err(error("diarize", "cancelled"));
            }
            guard.owner.check(
                // SAFETY: non-overlapping borrowed float slice is valid for this
                // synchronous call; the stream is exclusively held on this thread.
                unsafe {
                    (guard.owner.push)(stream, packet.as_ptr(), packet.len(), sample_rate as i32)
                },
                "push diarizer",
            )?;
        }
        if cancelled() {
            return Err(error("diarize", "cancelled"));
        }
        guard.owner.check(
            // SAFETY: live stream, finish exactly once after all input packets.
            unsafe { (guard.owner.finish)(stream) },
            "finish diarizer",
        )?;
        let mut count = 0;
        guard.owner.check(
            // SAFETY: documented two-call count query; null config selects defaults.
            unsafe { (guard.owner.segments)(stream, ptr::null(), ptr::null_mut(), 0, &mut count) },
            "count speaker segments",
        )?;
        if count > 100_000 {
            return Err(error("diarize", "too many speaker segments"));
        }
        let mut segments = vec![Segment::default(); count];
        guard.owner.check(
            // SAFETY: initialized repr(C) buffer has the queried capacity; no pushes
            // or changes can occur between count and copy on this exclusive stream.
            unsafe {
                (guard.owner.segments)(
                    stream,
                    ptr::null(),
                    segments.as_mut_ptr(),
                    segments.len(),
                    &mut count,
                )
            },
            "copy speaker segments",
        )?;
        if count > segments.len() {
            return Err(error("diarize", "segment count changed"));
        }
        segments.truncate(count);
        if cancelled() {
            return Err(error("diarize", "cancelled"));
        }
        Ok(segments)
    }
}

fn error(stage: &'static str, message: &str) -> Error {
    Error::Native {
        stage,
        message: message.into(),
    }
}
struct StreamGuard<'a> {
    stream: *mut Stream,
    owner: &'a Diarizer,
}
impl Drop for StreamGuard<'_> {
    fn drop(&mut self) {
        // SAFETY: stream is owned uniquely and owner/library remain live.
        unsafe { (self.owner.close)(self.stream) };
    }
}
impl Drop for Diarizer {
    fn drop(&mut self) {
        if !self.model.is_null() {
            // SAFETY: all borrowed streams have been closed; destroy before
            // unloading the Library field containing this function.
            unsafe { (self.destroy)(self.model) };
        }
    }
}

#[cfg(all(test, target_pointer_width = "64"))]
mod tests {
    use super::*;

    #[test]
    fn standalone_layout_matches_pinned_c_header() {
        assert_eq!(std::mem::size_of::<Config>(), 56);
        assert_eq!(std::mem::offset_of!(Config, preset), 24);
        assert_eq!(std::mem::offset_of!(Config, period), 52);
        assert_eq!(std::mem::size_of::<Segment>(), 24);
        assert_eq!(std::mem::offset_of!(Segment, speaker), 16);
    }
}
