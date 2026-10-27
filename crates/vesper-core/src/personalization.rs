//! Explicit, local preferences. Reviewed examples are data, never live training.
use crate::{CleanupPreset, Language};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputIntent {
    #[default]
    Verbatim,
    Note,
    Message,
    Technical,
}

impl OutputIntent {
    pub fn instruction(self) -> &'static str {
        match self {
            Self::Verbatim => "Preserve the dictated wording and order.",
            Self::Note => "Use ordinary note punctuation. Preserve every detail and its order; do not summarize or add headings.",
            Self::Message => "Use ordinary message punctuation. Do not invent greetings, recipients, sign-offs, or facts.",
            Self::Technical => "Preserve code, filenames and identifiers exactly unless an unambiguous spelling correction is supported by context.",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VocabularyEntry {
    pub id: String,
    pub phrase: String,
    pub language: Language,
    /// Optional lexical context: irrelevant entries are not put in cleanup prompts.
    pub context: String,
    pub enabled: bool,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub project: String,
    #[serde(default)]
    pub pronunciation: bool,
    #[serde(default)]
    pub sources: Vec<MemorySource>,
    /// Written by the local store, not accepted as user-supplied temporal evidence.
    #[serde(default)]
    pub learned_at: String,
    /// Store-owned candidate provenance; distinct from a human review.
    #[serde(default)]
    pub automatic_source: Option<String>,
    /// User confirmation tick: confirmed entries are trusted like manual ones.
    #[serde(default)]
    pub confirmed: bool,
    #[serde(default)]
    pub usage_count: u32,
}

/// Model identity belongs to the source session, never to ownership of a memory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MemorySource {
    pub session_id: String,
    pub review_revision_id: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CleanupExample {
    pub id: String,
    pub session_id: String,
    pub review_revision_id: String,
    pub input: String,
    pub output: String,
    pub language: Language,
    pub preset: CleanupPreset,
    pub intent: OutputIntent,
    pub enabled: bool,
    #[serde(default)]
    pub project: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct PersonalizationProfile {
    pub schema_version: u32,
    pub revision: u64,
    pub speech_bias_enabled: bool,
    pub output_intent: OutputIntent,
    /// Interpret this recording's presentation in the writing pass. Never a memory label.
    pub automatic_writing: bool,
    pub preserve_fillers: bool,
    pub apply_corrections: bool,
    pub use_remembered_intent: bool,
    pub active_project: String,
    pub project_mode: crate::ProjectMode,
    pub project_rules: Vec<crate::ProjectRule>,
    /// Detect repo/folder/website workspaces at shortcut capture start. Opt-in:
    /// it reads the frontmost app's open document, so it is off until enabled.
    pub automatic_workspaces: bool,
    /// Opt-in: read back a pasted span once and treat the user's changes there as
    /// a draft edit (docs/personalization/app-learning.md).
    pub learn_from_app_edits: bool,
    /// Opt-in: use unusual spellings already in the destination field, before the
    /// caret, as references for that recording only. Never stored.
    pub app_field_words: bool,
    /// Rule-based formatting of numbers, times, dates, emails, web addresses,
    /// hesitations and spoken self-corrections (English). Same for every model.
    pub format_dictation: bool,
    pub automatic_learning: bool,
    /// Changes when learning permission changes or all automatic evidence is forgotten.
    pub learning_epoch: u64,
    pub vocabulary: Vec<VocabularyEntry>,
    pub examples: Vec<CleanupExample>,
    /// Transient per-recording decode input (working-set hints). Never stored,
    /// serialized or accepted from the frontend.
    #[serde(skip)]
    pub decode_context: Option<crate::DecodeContext>,
}

impl Default for PersonalizationProfile {
    fn default() -> Self {
        Self {
            schema_version: 4,
            revision: 0,
            speech_bias_enabled: false,
            output_intent: OutputIntent::default(),
            automatic_writing: false,
            preserve_fillers: false,
            apply_corrections: true,
            use_remembered_intent: false,
            active_project: String::new(),
            project_mode: crate::ProjectMode::Manual,
            project_rules: vec![],
            automatic_workspaces: false,
            learn_from_app_edits: false,
            app_field_words: false,
            format_dictation: true,
            automatic_learning: false,
            learning_epoch: 0,
            vocabulary: vec![],
            examples: vec![],
            decode_context: None,
        }
    }
}

impl PersonalizationProfile {
    pub fn validate(&self) -> std::result::Result<(), String> {
        if !(1..=4).contains(&self.schema_version)
            || self.vocabulary.len() > 128
            || self.examples.len() > 64
        {
            return Err(
                "Unsupported profile schema or too many entries (128 words, 64 examples maximum)."
                    .into(),
            );
        }
        if !optional_text(&self.active_project, 64) {
            return Err("Project names must be plain text up to 64 bytes.".into());
        }
        crate::validate_project_rules(&self.project_rules)?;
        let mut ids = std::collections::HashSet::new();
        for entry in &self.vocabulary {
            if !safe_text(&entry.id, 64)
                || !ids.insert(&entry.id)
                || !safe_text(&entry.phrase, 128)
                || entry.phrase.chars().any(char::is_control)
                || entry.phrase.starts_with('$')
                || entry.context.len() > 256
                || unsafe_text(&entry.context)
                || !optional_text(&entry.project, 64)
                || entry.aliases.len() > 8
                || entry
                    .aliases
                    .iter()
                    .any(|a| !safe_text(a, 128) || a.split_whitespace().count() > 8)
                || entry.sources.len() > 16
                || !optional_text(&entry.learned_at, 40)
                || entry
                    .automatic_source
                    .as_ref()
                    .is_some_and(|id| !safe_text(id, 64))
                || entry
                    .sources
                    .iter()
                    .any(|s| !safe_text(&s.session_id, 64) || !safe_text(&s.review_revision_id, 64))
            {
                return Err("Vocabulary entries need unique IDs, a phrase up to 128 bytes and plain context text.".into());
            }
        }
        for entry in &self.examples {
            if !safe_text(&entry.id, 64)
                || !ids.insert(&entry.id)
                || !safe_text(&entry.session_id, 64)
                || !safe_text(&entry.review_revision_id, 64)
                || !safe_text(&entry.input, 2048)
                || !safe_text(&entry.output, 2048)
                || !optional_text(&entry.project, 64)
            {
                return Err(
                    "Examples need unique IDs, an approved review and at most 2048 bytes per text."
                        .into(),
                );
            }
        }
        Ok(())
    }

    /// Uniform score 2.0 is an opt-in starting point, not a measured optimum.
    /// Empty disables native biasing; a nonempty list with score zero does not.
    pub fn speech_phrases(&self, language: Language) -> Vec<String> {
        if !self.speech_bias_enabled {
            return vec![];
        }
        let mut entries = self
            .vocabulary
            .iter()
            .filter(|e| e.enabled && language_matches(e.language, language))
            .filter(|e| crate::memory::project_matches(&e.project, &self.active_project))
            .filter(|e| {
                e.context.trim().is_empty()
                    || crate::memory::context_matches(&e.context, &self.active_project)
            })
            .collect::<Vec<_>>();
        entries.sort_by_key(|e| {
            (
                e.project.is_empty(),
                std::cmp::Reverse(e.usage_count),
                std::cmp::Reverse(e.sources.len()),
                &e.id,
            )
        });
        let mut seen = std::collections::HashSet::new();
        let mut bytes = 0;
        entries
            .into_iter()
            .filter(|e| seen.insert(e.phrase.to_lowercase()))
            .filter(|e| {
                bytes += e.phrase.len();
                bytes <= 2048
            })
            .take(32)
            .map(|e| e.phrase.clone())
            .collect()
    }
}

pub fn language_matches(entry: Language, request: Language) -> bool {
    entry == Language::Auto || entry == request
}

fn unsafe_text(text: &str) -> bool {
    text.contains("<|")
        || text
            .chars()
            .any(|c| c.is_control() && c != '\n' && c != '\t')
}
fn safe_text(text: &str, max: usize) -> bool {
    !text.trim().is_empty() && text.len() <= max && !unsafe_text(text)
}

fn optional_text(text: &str, max: usize) -> bool {
    text.len() <= max && !unsafe_text(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bias_is_explicit_bounded_deduplicated_and_language_scoped() {
        let mut p = PersonalizationProfile::default();
        for n in 0..40 {
            p.vocabulary.push(VocabularyEntry {
                id: n.to_string(),
                phrase: format!("Term{n}"),
                context: String::new(),
                language: Language::English,
                enabled: true,
                ..VocabularyEntry::default()
            });
        }
        assert!(p.speech_phrases(Language::English).is_empty());
        p.speech_bias_enabled = true;
        assert_eq!(p.speech_phrases(Language::English).len(), 32);
        assert!(p.speech_phrases(Language::French).is_empty());
        assert!(p.speech_phrases(Language::Auto).is_empty());
        p.vocabulary[0].phrase = "<|im_start|>".into();
        assert!(p.validate().is_err());
    }
}
