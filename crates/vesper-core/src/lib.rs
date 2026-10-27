//! Shared types for local dictation, independent of desktop and inference runtimes.

mod adapters;
mod app_text;
mod config;
mod diarization;
pub mod environment;
mod error;
mod evidence;
mod formatting;
mod gate;
mod generated;
mod hooks;
mod huggingface;
mod learning;
mod memory;
mod model;
mod personalization;
mod projects;
mod semantic;
mod shortcuts;
mod speech;
mod term_select;
mod terms;
mod transcript;
mod workspace;

pub use adapters::{
    adapter, mlx_speech_only, mlx_speech_supported, nemo_gguf_supported, neural_engine_supported,
    resolve_adapter, AdapterCapabilities, AdapterKind, AdapterRegistration, CacheState, LiveMode,
    ADAPTERS, MLX_REVISION, MLX_RUNTIME,
};
pub use app_text::{aligned_edit, field_terms, MAX_ALIGNED_WORDS, MAX_FIELD_TERMS};
pub use config::{
    AppConfig, AsrEngine, AudioFrontendConfig, CaptureDesign, CleanupPreset, ComputeBackend,
    DictationOutput, ImprovementMode, Language, ModelSelection, NoiseReduction, RecordingMode,
    RewriteQuantization, CONFIG_SCHEMA_VERSION,
};
pub use diarization::{SpeakerSegment, SpeakerTimeline};
pub use error::{Error, Result};
pub use evidence::*;
pub use formatting::{format_dictation, Formatted};
pub use gate::{correction_gate, correction_gate_with, ordinary_language, ordinary_word, NearMiss};
pub use generated::runtime_language_code;
pub use hooks::{
    bounded_hints, hint_hash, hinted, recognizer_hints, ConfidenceKind, DecodeContext, HintSource,
    HookStatus, RecognizerHints, SpeechHook, SpeechHookKind,
};
pub use huggingface::{HuggingFaceFile, HuggingFaceProfile, ModelFileRole, ModelRuntime};
pub use learning::*;
pub use memory::{
    context_matches, correction_suggestions, memory_phrase_occurs, memory_words, project_matches,
    word_edits, CorrectionPair, IntentSuggestion, MemoryApplication, MemoryEvidence,
    PersonalizedText,
};
pub use model::{default_model_catalog, AsrModel, ModelRole, ModelSpec, RewriteModel};
pub use personalization::{
    language_matches, CleanupExample, MemorySource, OutputIntent, PersonalizationProfile,
    VocabularyEntry,
};
pub use projects::*;
pub use semantic::*;
pub use shortcuts::{recording_shortcut_identity, same_recording_shortcut};
pub use speech::{
    whisper_no_speech, WHISPER_LOG_PROBABILITY_THRESHOLD, WHISPER_NO_SPEECH_THRESHOLD,
};
pub use term_select::*;
pub use terms::*;
pub use transcript::{PipelineStage, Transcript};
pub use workspace::*;
