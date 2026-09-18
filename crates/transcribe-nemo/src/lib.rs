//! Small safe wrapper around NeMo-Speech.cpp's versioned C ABI.
//!
//! Unsafe dynamic loading and opaque-pointer ownership live only in this crate.
//! Callers retain a recognizer and create a fresh stream for every utterance.

use std::{
    ffi::{c_char, CStr, CString},
    marker::PhantomData,
    path::{Path, PathBuf},
    ptr,
};

use libloading::Library;

const STATUS_OK: i32 = 0;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("NeMo-Speech.cpp library could not be loaded from {path}: {message}")]
    Load { path: PathBuf, message: String },
    #[error("NeMo-Speech.cpp symbol {symbol} is unavailable: {message}")]
    Symbol {
        symbol: &'static str,
        message: String,
    },
    #[error("NeMo-Speech.cpp {stage} failed: {message}")]
    Native {
        stage: &'static str,
        message: String,
    },
    #[error("path or request metadata contains an interior null byte")]
    InvalidCString,
    #[error("speech contexts exceed bounds or contain an invalid phrase/boost")]
    InvalidSpeechContext,
    #[error("invalid RNNT right context; expected model default (-1) or 0..=64")]
    InvalidStreamingContext,
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Debug, PartialEq)]
pub struct Transcript {
    pub text: String,
    pub is_final: bool,
    pub audio_processed_seconds: f32,
}

#[repr(C)]
struct RecognizerOpaque {
    _private: [u8; 0],
}

#[repr(C)]
struct StreamOpaque {
    _private: [u8; 0],
}

#[repr(C)]
struct ResultOpaque {
    _private: [u8; 0],
}

#[repr(C)]
struct BackendConfig {
    size: usize,
    gpu: i32,
}

#[repr(C)]
struct ModelConfig {
    size: usize,
    path: *const c_char,
    name: *const c_char,
}

#[repr(C)]
struct StreamingConfig {
    size: usize,
    chunk_size: f32,
    ctc_left_padding: f32,
    ctc_right_padding: f32,
    rnnt_right_context: i32,
}

impl StreamingConfig {
    fn new(right_context: i32) -> Result<Self> {
        if !(-1..=64).contains(&right_context) {
            return Err(Error::InvalidStreamingContext);
        }
        // All fields are copied and validated, even for RNNT. Zero-initializing
        // the unrelated CTC geometry is not equivalent to native defaults.
        Ok(Self {
            size: std::mem::size_of::<Self>(),
            chunk_size: 0.16,
            ctc_left_padding: 1.92,
            ctc_right_padding: 1.92,
            rnnt_right_context: right_context,
        })
    }
}

#[repr(C)]
struct RecognizerConfig {
    size: usize,
    backend: *const BackendConfig,
    model: *const ModelConfig,
    streaming: *const StreamingConfig,
    decoder: *const (),
    vad: *const (),
    endpointing: *const (),
    postproc: *const (),
    diar: *const (),
    batching: *const (),
}

#[repr(C)]
#[derive(Clone, Copy)]
struct RecognitionOptions {
    size: usize,
    request_id: *const c_char,
    language_code: *const c_char,
    interim_results: bool,
    enable_word_time_offsets: bool,
    enable_automatic_punctuation: bool,
    verbatim_transcripts: bool,
    profanity_filter: bool,
    stop_history_eou_ms: i32,
    speech_contexts: *const SpeechContext,
    speech_context_count: usize,
    max_alternatives: i32,
    enable_speaker_diarization: bool,
    max_speaker_count: i32,
}

// Pinned v0.1.0 include/nemo_speech/asr.h. All backing allocations remain
// alive through the C call; c_api.cpp copies phrases into native request state.
#[repr(C)]
struct SpeechContext {
    size: usize,
    phrases: *const *const c_char,
    phrase_count: usize,
    boost: f32,
}

struct OwnedSpeechContext {
    _strings: Vec<CString>,
    _pointers: Vec<*const c_char>,
    context: SpeechContext,
}
impl OwnedSpeechContext {
    fn new(phrases: &[String], boost: f32) -> Result<Self> {
        if !boost.is_finite()
            || !(0.1..=5.0).contains(&boost)
            || phrases.len() > 32
            || phrases.iter().map(String::len).sum::<usize>() > 2048
            || phrases.iter().any(|p| {
                p.trim().is_empty()
                    || p.len() > 128
                    || p.starts_with('$')
                    || p.contains("<|")
                    || p.chars().any(char::is_control)
            })
        {
            return Err(Error::InvalidSpeechContext);
        }
        let strings = phrases
            .iter()
            .map(|p| CString::new(p.as_str()).map_err(|_| Error::InvalidCString))
            .collect::<Result<Vec<_>>>()?;
        let pointers = strings.iter().map(|s| s.as_ptr()).collect::<Vec<_>>();
        let context = SpeechContext {
            size: std::mem::size_of::<SpeechContext>(),
            phrases: pointers.as_ptr(),
            phrase_count: pointers.len(),
            boost,
        };
        Ok(Self {
            _strings: strings,
            _pointers: pointers,
            context,
        })
    }
    fn apply(&self, options: &mut RecognitionOptions) {
        if self.context.phrase_count > 0 {
            options.speech_contexts = &self.context;
            options.speech_context_count = 1;
        } else {
            options.speech_contexts = ptr::null();
            options.speech_context_count = 0;
        }
    }
}

type OptionsDefault = unsafe extern "C" fn() -> RecognitionOptions;
type Create = unsafe extern "C" fn(*const RecognizerConfig, *mut *mut RecognizerOpaque) -> i32;
type Destroy = unsafe extern "C" fn(*mut RecognizerOpaque);
type Recognize = unsafe extern "C" fn(
    *mut RecognizerOpaque,
    *const RecognitionOptions,
    *const f32,
    usize,
    i32,
    *mut *mut ResultOpaque,
) -> i32;
type StreamingRecognize = unsafe extern "C" fn(
    *mut RecognizerOpaque,
    *const RecognitionOptions,
    *mut *mut StreamOpaque,
) -> i32;
type StreamPush = unsafe extern "C" fn(*mut StreamOpaque, *const f32, usize, i32) -> i32;
type StreamFinish = unsafe extern "C" fn(*mut StreamOpaque) -> i32;
type StreamNext = unsafe extern "C" fn(*mut StreamOpaque, *mut *mut ResultOpaque) -> i32;
type StreamClose = unsafe extern "C" fn(*mut StreamOpaque);
type ResultIsFinal = unsafe extern "C" fn(*const ResultOpaque) -> bool;
type ResultAudioProcessed = unsafe extern "C" fn(*const ResultOpaque) -> f32;
type ResultAlternativeCount = unsafe extern "C" fn(*const ResultOpaque) -> usize;
type ResultTranscript = unsafe extern "C" fn(*const ResultOpaque, usize) -> *const c_char;
type ResultDestroy = unsafe extern "C" fn(*mut ResultOpaque);
type LastError = unsafe extern "C" fn() -> *const c_char;
type Version = unsafe extern "C" fn() -> *const c_char;

struct Api {
    _library: Library,
    options_default: OptionsDefault,
    create: Create,
    destroy: Destroy,
    recognize: Recognize,
    streaming_recognize: StreamingRecognize,
    stream_push: StreamPush,
    stream_finish: StreamFinish,
    stream_next: StreamNext,
    stream_close: StreamClose,
    result_is_final: ResultIsFinal,
    result_audio_processed: ResultAudioProcessed,
    result_alternative_count: ResultAlternativeCount,
    result_transcript: ResultTranscript,
    result_destroy: ResultDestroy,
    last_error: LastError,
    version: Version,
}

impl Api {
    fn load(path: &Path) -> Result<Self> {
        // SAFETY: the library path is an installed, hash-verified project artifact.
        // Each copied symbol is checked against the exact v1 C ABI signature below,
        // and `_library` keeps the image loaded for every stored function pointer.
        let library = unsafe { Library::new(path) }.map_err(|error| Error::Load {
            path: path.to_owned(),
            message: error.to_string(),
        })?;
        macro_rules! symbol {
            ($name:literal, $kind:ty) => {{
                // SAFETY: `$name` and `$kind` are transcribed from the pinned v1 header.
                unsafe { library.get::<$kind>(concat!($name, "\0").as_bytes()) }
                    .map(|symbol| *symbol)
                    .map_err(|error| Error::Symbol {
                        symbol: $name,
                        message: error.to_string(),
                    })?
            }};
        }
        Ok(Self {
            options_default: symbol!(
                "nemo_speech_asr_recognition_options_default",
                OptionsDefault
            ),
            create: symbol!("nemo_speech_asr_create", Create),
            destroy: symbol!("nemo_speech_asr_destroy", Destroy),
            recognize: symbol!("nemo_speech_asr_recognize_f32", Recognize),
            streaming_recognize: symbol!("nemo_speech_asr_streaming_recognize", StreamingRecognize),
            stream_push: symbol!("nemo_speech_asr_stream_push_f32", StreamPush),
            stream_finish: symbol!("nemo_speech_asr_stream_finish", StreamFinish),
            stream_next: symbol!("nemo_speech_asr_stream_next", StreamNext),
            stream_close: symbol!("nemo_speech_asr_stream_close", StreamClose),
            result_is_final: symbol!("nemo_speech_asr_result_is_final", ResultIsFinal),
            result_audio_processed: symbol!(
                "nemo_speech_asr_result_audio_processed",
                ResultAudioProcessed
            ),
            result_alternative_count: symbol!(
                "nemo_speech_asr_result_alternative_count",
                ResultAlternativeCount
            ),
            result_transcript: symbol!("nemo_speech_asr_result_transcript", ResultTranscript),
            result_destroy: symbol!("nemo_speech_asr_result_destroy", ResultDestroy),
            last_error: symbol!("nemo_speech_asr_last_error", LastError),
            version: symbol!("nemo_speech_asr_version", Version),
            _library: library,
        })
    }

    fn error(&self, stage: &'static str) -> Error {
        // SAFETY: the v1 ABI returns a thread-local NUL-terminated string or NULL.
        let message = unsafe { c_string((self.last_error)()) };
        Error::Native { stage, message }
    }

    fn transcript(&self, raw: *mut ResultOpaque) -> Transcript {
        // SAFETY: `raw` is a live result returned by this API and remains valid
        // until `result_destroy` at the end of this method.
        let transcript = unsafe {
            let text = if (self.result_alternative_count)(raw) == 0 {
                String::new()
            } else {
                c_string((self.result_transcript)(raw, 0))
            };
            Transcript {
                text: text.trim().to_owned(),
                is_final: (self.result_is_final)(raw),
                audio_processed_seconds: (self.result_audio_processed)(raw),
            }
        };
        // SAFETY: ownership of `raw` was transferred by the API exactly once.
        unsafe { (self.result_destroy)(raw) };
        transcript
    }
}

/// Long-lived immutable recognizer/model weights.
pub struct Recognizer {
    api: Api,
    raw: *mut RecognizerOpaque,
}

impl Recognizer {
    pub fn load(library_path: &Path, model_path: &Path, gpu: Option<u32>) -> Result<Self> {
        Self::load_config(library_path, model_path, gpu, None)
    }

    /// Select an RNNT policy at model creation; -1 means artifact default.
    /// Callers must validate explicit modes against their checkpoint metadata.
    pub fn load_with_right_context(
        library_path: &Path,
        model_path: &Path,
        gpu: Option<u32>,
        right_context: i32,
    ) -> Result<Self> {
        Self::load_config(
            library_path,
            model_path,
            gpu,
            Some(StreamingConfig::new(right_context)?),
        )
    }

    fn load_config(
        library_path: &Path,
        model_path: &Path,
        gpu: Option<u32>,
        streaming: Option<StreamingConfig>,
    ) -> Result<Self> {
        let api = Api::load(library_path)?;
        let model_path = CString::new(model_path.to_string_lossy().as_bytes())
            .map_err(|_| Error::InvalidCString)?;
        let backend = BackendConfig {
            size: std::mem::size_of::<BackendConfig>(),
            gpu: gpu.map_or(-1, |index| index as i32),
        };
        let model = ModelConfig {
            size: std::mem::size_of::<ModelConfig>(),
            path: model_path.as_ptr(),
            name: ptr::null(),
        };
        let config = RecognizerConfig {
            size: std::mem::size_of::<RecognizerConfig>(),
            backend: &backend,
            model: &model,
            streaming: streaming
                .as_ref()
                .map_or(ptr::null(), |value| value as *const StreamingConfig),
            decoder: ptr::null(),
            vad: ptr::null(),
            endpointing: ptr::null(),
            postproc: ptr::null(),
            diar: ptr::null(),
            batching: ptr::null(),
        };
        let mut raw = ptr::null_mut();
        // SAFETY: all config pointers remain valid for this call and the SDK
        // documents that startup configuration is copied by `create`.
        let status = unsafe { (api.create)(&config, &mut raw) };
        if status != STATUS_OK || raw.is_null() {
            return Err(api.error("recognizer creation"));
        }
        Ok(Self { api, raw })
    }

    pub fn version(&self) -> String {
        // SAFETY: the version accessor returns a static NUL-terminated string.
        unsafe { c_string((self.api.version)()) }
    }

    pub fn recognize(
        &self,
        samples: &[f32],
        sample_rate: u32,
        language: Option<&str>,
        request_id: Option<&str>,
    ) -> Result<Transcript> {
        self.recognize_with_phrases(samples, sample_rate, language, request_id, &[], 2.0)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn recognize_with_phrases(
        &self,
        samples: &[f32],
        sample_rate: u32,
        language: Option<&str>,
        request_id: Option<&str>,
        phrases: &[String],
        boost: f32,
    ) -> Result<Transcript> {
        let (_request, _language, mut options) = self.options(language, request_id, false)?;
        let speech = OwnedSpeechContext::new(phrases, boost)?;
        speech.apply(&mut options);
        let mut raw = ptr::null_mut();
        // SAFETY: the recognizer is live, options and sample storage remain valid
        // for the call, and `raw` is initialized for ownership transfer.
        let status = unsafe {
            (self.api.recognize)(
                self.raw,
                &options,
                samples.as_ptr(),
                samples.len(),
                sample_rate as i32,
                &mut raw,
            )
        };
        if status != STATUS_OK {
            return Err(self.api.error("offline recognition"));
        }
        if raw.is_null() {
            return Ok(Transcript {
                text: String::new(),
                is_final: true,
                audio_processed_seconds: 0.0,
            });
        }
        Ok(self.api.transcript(raw))
    }

    pub fn stream<'a>(
        &'a self,
        language: Option<&str>,
        request_id: Option<&str>,
    ) -> Result<Stream<'a>> {
        self.stream_with_phrases(language, request_id, &[], 2.0)
    }

    pub fn stream_with_phrases<'a>(
        &'a self,
        language: Option<&str>,
        request_id: Option<&str>,
        phrases: &[String],
        boost: f32,
    ) -> Result<Stream<'a>> {
        let (_request, _language, mut options) = self.options(language, request_id, true)?;
        let speech = OwnedSpeechContext::new(phrases, boost)?;
        speech.apply(&mut options);
        let mut raw = ptr::null_mut();
        // SAFETY: options remain valid for the call and the SDK copies the
        // per-request values into the returned stream.
        let status = unsafe { (self.api.streaming_recognize)(self.raw, &options, &mut raw) };
        if status != STATUS_OK || raw.is_null() {
            return Err(self.api.error("stream creation"));
        }
        Ok(Stream {
            api: &self.api,
            raw,
            finished: false,
            _recognizer: PhantomData,
        })
    }

    fn options(
        &self,
        language: Option<&str>,
        request_id: Option<&str>,
        interim_results: bool,
    ) -> Result<(Option<CString>, Option<CString>, RecognitionOptions)> {
        let request = request_id
            .map(CString::new)
            .transpose()
            .map_err(|_| Error::InvalidCString)?;
        let language = language
            .filter(|value| !value.is_empty())
            .map(CString::new)
            .transpose()
            .map_err(|_| Error::InvalidCString)?;
        // SAFETY: the function returns the POD options struct by value.
        let mut options = unsafe { (self.api.options_default)() };
        options.request_id = request.as_ref().map_or(ptr::null(), |value| value.as_ptr());
        options.language_code = language
            .as_ref()
            .map_or(ptr::null(), |value| value.as_ptr());
        options.interim_results = interim_results;
        // Match the public API/CLI default. The C ABI's POD constructor is
        // zero-initialized, so this request default must be set explicitly.
        options.enable_automatic_punctuation = true;
        Ok((request, language, options))
    }
}

impl Drop for Recognizer {
    fn drop(&mut self) {
        if !self.raw.is_null() {
            // SAFETY: `raw` is owned by this wrapper and destroyed exactly once.
            unsafe { (self.api.destroy)(self.raw) };
        }
    }
}

/// Per-utterance state. A stream must be driven from one thread.
pub struct Stream<'recognizer> {
    api: &'recognizer Api,
    raw: *mut StreamOpaque,
    finished: bool,
    _recognizer: PhantomData<&'recognizer Recognizer>,
}

impl Stream<'_> {
    pub fn push(&mut self, samples: &[f32], sample_rate: u32) -> Result<()> {
        // SAFETY: the stream is live and the sample slice remains valid for the call.
        let status = unsafe {
            (self.api.stream_push)(
                self.raw,
                samples.as_ptr(),
                samples.len(),
                sample_rate as i32,
            )
        };
        self.status(status, "stream audio push")
    }

    pub fn poll_result(&mut self) -> Result<Option<Transcript>> {
        let mut raw = ptr::null_mut();
        // SAFETY: the stream is live and `raw` is initialized for ownership transfer.
        let status = unsafe { (self.api.stream_next)(self.raw, &mut raw) };
        self.status(status, "stream decode")?;
        Ok((!raw.is_null()).then(|| self.api.transcript(raw)))
    }

    pub fn finish(&mut self) -> Result<()> {
        if self.finished {
            return Ok(());
        }
        // SAFETY: the stream is live and finish is invoked at most once.
        let status = unsafe { (self.api.stream_finish)(self.raw) };
        self.status(status, "stream finish")?;
        self.finished = true;
        Ok(())
    }

    fn status(&self, status: i32, stage: &'static str) -> Result<()> {
        if status == STATUS_OK {
            Ok(())
        } else {
            Err(self.api.error(stage))
        }
    }
}

impl Drop for Stream<'_> {
    fn drop(&mut self) {
        if !self.raw.is_null() {
            // SAFETY: `raw` is owned by this wrapper and closed exactly once.
            unsafe { (self.api.stream_close)(self.raw) };
        }
    }
}

unsafe fn c_string(pointer: *const c_char) -> String {
    if pointer.is_null() {
        return "unknown native error".into();
    }
    // SAFETY: callers only pass strings returned by the v1 C ABI, which are
    // documented as NUL-terminated and live for the duration of this copy.
    unsafe { CStr::from_ptr(pointer) }
        .to_string_lossy()
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_streaming_policy_preserves_complete_native_geometry() {
        let config = StreamingConfig::new(-1).unwrap();
        assert_eq!(config.size, std::mem::size_of::<StreamingConfig>());
        assert_eq!(config.rnnt_right_context, -1);
        assert_eq!(config.chunk_size, 0.16);
        assert_eq!(config.ctc_left_padding, 1.92);
        assert_eq!(config.ctc_right_padding, 1.92);
        assert_eq!(StreamingConfig::new(3).unwrap().rnnt_right_context, 3);
        assert!(StreamingConfig::new(-2).is_err());
        assert!(StreamingConfig::new(i32::MAX).is_err());
    }

    #[test]
    fn speech_context_owns_utf8_and_rejects_invalid_boosts_and_controls() {
        let speech = OwnedSpeechContext::new(&["राहुल".into(), "Atlas".into()], 2.0).unwrap();
        assert_eq!(speech.context.size, std::mem::size_of::<SpeechContext>());
        assert_eq!(speech.context.phrase_count, 2);
        assert_eq!(
            // SAFETY: backing strings and pointers are retained by `speech`.
            unsafe { CStr::from_ptr(*speech.context.phrases) }
                .to_str()
                .unwrap(),
            "राहुल"
        );
        for boost in [0.0, -1.0, f32::NAN, f32::INFINITY, 5.1] {
            assert!(OwnedSpeechContext::new(&["Atlas".into()], boost).is_err());
        }
        for phrase in ["", "bad\0phrase", "bad\nphrase", "$NAME", "<|im_start|>"] {
            assert!(OwnedSpeechContext::new(&[phrase.into()], 2.0).is_err());
        }
        assert!(OwnedSpeechContext::new(&vec!["word".into(); 33], 2.0).is_err());
        assert_eq!(
            OwnedSpeechContext::new(&[], 2.0)
                .unwrap()
                .context
                .phrase_count,
            0
        );
    }

    #[test]
    fn ffi_layout_starts_with_current_size() {
        assert!(std::mem::size_of::<RecognizerConfig>() >= 10 * std::mem::size_of::<usize>());
        assert!(std::mem::size_of::<RecognitionOptions>() > std::mem::size_of::<usize>());
    }
}
