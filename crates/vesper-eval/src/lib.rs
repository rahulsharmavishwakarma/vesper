//! Reproducible, development-only speech evaluation contracts and metrics.
//!
//! This crate is intentionally outside the desktop dependency graph. It consumes
//! the same public audio and inference APIs without becoming a second runtime.

mod manifest;
mod metrics;
mod recommendation;
pub mod term_eval;

pub use manifest::{
    BenchmarkEnvironment, BenchmarkManifest, BenchmarkSample, CorpusSource, ReplayKind, Scenario,
};
pub use metrics::{
    score, score_non_speech, word_edit_distance, ErrorRate, NonSpeechScore, TranscriptScore,
};
pub use recommendation::{
    recommend, BenchmarkEvidence, ModelCapability, Recommendation, RecommendationRequest,
    SupportLevel,
};
