//! Shared types for local dictation, independent of desktop and inference runtimes.

mod adapters;
mod config;
mod diarization;
pub mod environment;
mod error;
mod huggingface;
mod learning;
mod memory;
mod model;
mod personalization;
mod semantic;
mod shortcuts;
mod speech;
mod transcript;

pub use adapters::{
    adapter, mlx_speech_supported, resolve_adapter, AdapterCapabilities, AdapterKind,
    AdapterRegistration, CacheState, ADAPTERS, MLX_REVISION, MLX_RUNTIME,
};
pub use config::{
    AppConfig, AudioFrontendConfig, CaptureDesign, CleanupPreset, ComputeBackend, DictationOutput,
    ImprovementMode, Language, ModelSelection, NoiseReduction, RecordingMode, RewriteQuantization,
    CONFIG_SCHEMA_VERSION,
};
pub use diarization::{SpeakerSegment, SpeakerTimeline};
pub use error::{Error, Result};
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
pub use semantic::*;
pub use shortcuts::{recording_shortcut_identity, same_recording_shortcut};
pub use speech::{
    whisper_no_speech, WHISPER_LOG_PROBABILITY_THRESHOLD, WHISPER_NO_SPEECH_THRESHOLD,
};
pub use transcript::{PipelineStage, Transcript};
