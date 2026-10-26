//! Model-independent term memory contracts: where a spelling came from, how far it
//! is trusted, and the small per-recording working set handed to any recognizer.
//! Machine output (ASR, writer, rules) is never a spelling source.
use serde::{Deserialize, Serialize};

/// Receiving-application category; a coarse, fixed table, never inferred from text.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppCategory {
    Code,
    Terminal,
    Email,
    Chat,
    Document,
    Notes,
    Browser,
    #[default]
    Other,
}

/// Transient per-dictation context. Keys are salted hashes; labels are display names
/// (a folder's last component or a web host), never paths, URLs or document text.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceContext {
    pub application_id: String,
    pub category: AppCategory,
    /// Empty when no workspace was detected or it is still latent.
    pub workspace_key: String,
    pub workspace_label: String,
}

/// Trust tier of a spelling source. Tier C (machine output) is not representable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TermTier {
    /// The user asserted the spelling: settings, corrections, text replacements,
    /// spelling dictionaries, contacts.
    Asserted,
    /// The user (or their project) authored the spelling: identifiers, manifests,
    /// documentation headings. Needs independent repetition before admission.
    Authored,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TermSource {
    Vocabulary,
    Correction,
    TextReplacement,
    SpellingDictionary,
    ProjectDictionary,
    Identifier,
    Manifest,
    Documentation,
    Contact,
}

impl TermSource {
    pub fn tier(self) -> TermTier {
        match self {
            Self::Identifier | Self::Manifest | Self::Documentation => TermTier::Authored,
            _ => TermTier::Asserted,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TermKind {
    Name,
    Identifier,
    Acronym,
    Phrase,
}

/// Selected per recording. Hints go to a qualified recognizer hook; references are
/// spellings the writer may copy; normalizations are exact spoken-form → spelling
/// pairs the correction gate may apply. All lists are bounded by the selector.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkingSet {
    pub hints: Vec<String>,
    pub references: Vec<String>,
    pub normalizations: Vec<Normalization>,
    /// Hash of the hint list, recorded as bias provenance for every decode.
    pub hint_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Normalization {
    pub spoken: String,
    pub spelling: String,
    pub term_id: String,
}

pub const MAX_HINTS: usize = 32;
pub const MAX_HINT_BYTES: usize = 2048;
pub const MAX_REFERENCES: usize = 16;
pub const MAX_NORMALIZATIONS: usize = 64;

/// Why the correction gate declined a candidate change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GateDecline {
    /// The heard span already reads as ordinary language.
    OrdinaryLanguage,
    /// Numbers, dates, weekdays, months, negation, pronouns or function words.
    ProtectedClass,
    /// Overlapping candidates disagree.
    Ambiguous,
    /// Inside quotes or across a sentence/line boundary.
    Boundary,
    /// The language has no plausibility lexicon; non-exact changes fail closed.
    UnsupportedLanguage,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GateChange {
    pub start: usize,
    pub end: usize,
    pub heard: String,
    pub spelling: String,
    pub term_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GateDeclined {
    pub heard: String,
    pub spelling: String,
    pub reason: GateDecline,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GateResult {
    pub text: String,
    /// Offsets refer to the input text.
    pub applied: Vec<GateChange>,
    pub declined: Vec<GateDeclined>,
}
