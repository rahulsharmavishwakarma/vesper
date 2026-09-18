//! Frozen-weight automatic spelling admission and provisional interaction evidence.
//! Scores here are deterministic policy rules, not calibrated model confidence.
use crate::{
    correction_suggestions, memory_words, CorrectionPair, Language, PersonalizationProfile,
    VocabularyEntry,
};
use serde::{Deserialize, Serialize};

pub const LEARNING_POLICY_VERSION: u32 = 1;
pub const MAX_LEARNING_TEXT_BYTES: usize = 8192;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateState {
    Observed,
    Provisional,
    Active,
    Suspended,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetryRelation {
    PossibleRepair,
    Continuation,
    ChangedIntent,
    Uncertain,
}

/// Server-authored context, frozen for one result. No transcript or audio is stored here.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LearningContext {
    pub result_id: String,
    pub source_key: String,
    pub episode_id: String,
    pub language: Language,
    pub project: String,
    pub model_id: String,
    pub learning_epoch: u64,
    pub learning_enabled: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LearningOutcome {
    pub activated: Vec<String>,
    pub provisional: usize,
    pub suspended: usize,
    pub status: String,
    pub profile_revision: u64,
    pub elapsed_micros: u64,
}

/// A single short, guarded replacement. Pure formatting, quote edits, additions,
/// broad rewriting and unrelated lexical substitutions are not automatic aliases.
pub fn automatic_spelling_pair(before: &str, after: &str) -> Option<CorrectionPair> {
    if before.len().max(after.len()) > MAX_LEARNING_TEXT_BYTES {
        return None;
    }
    let pair = correction_suggestions(before, after).pop()?;
    if pair.heard.split_whitespace().count() > 3 || pair.intended.split_whitespace().count() != 1 {
        return None;
    }
    let common = [
        "good", "bad", "yes", "okay", "ok", "cat", "dog", "send", "delete", "add", "remove",
        "start", "stop", "more", "less", "formal", "casual", "left", "right", "public", "private",
    ];
    if common.contains(&pair.heard.to_lowercase().as_str())
        || common.contains(&pair.intended.to_lowercase().as_str())
    {
        return None;
    }
    // Plausibly related spelling or a supplied identifier; this is not a phonetic decoder.
    if !technical_spelling(&pair.intended) && !related_spelling(&pair.heard, &pair.intended) {
        return None;
    }
    let profile = PersonalizationProfile {
        vocabulary: vec![VocabularyEntry {
            id: "probe".into(),
            phrase: pair.intended.clone(),
            aliases: vec![pair.heard.clone()],
            enabled: true,
            ..Default::default()
        }],
        ..Default::default()
    };
    (profile.personalize(before, Language::English).text == after).then_some(pair)
}

pub fn technical_spelling(text: &str) -> bool {
    text.chars()
        .all(|c| c.is_alphanumeric() || matches!(c, '_' | '.' | '-' | ':' | '/'))
        && text.chars().any(char::is_alphabetic)
        && (text.contains('_')
            || text.contains('.')
            || text.contains("::")
            || text
                .chars()
                .zip(text.chars().skip(1))
                .any(|(a, b)| a.is_lowercase() && b.is_uppercase()))
}

fn related_spelling(a: &str, b: &str) -> bool {
    let a: Vec<_> = a
        .to_lowercase()
        .chars()
        .filter(|c| !c.is_whitespace() && !c.is_ascii_punctuation())
        .collect();
    let b: Vec<_> = b
        .to_lowercase()
        .chars()
        .filter(|c| !c.is_whitespace() && !c.is_ascii_punctuation())
        .collect();
    if a.len().min(b.len()) < 4 {
        return false;
    }
    let mut row: Vec<_> = (0..=b.len()).collect();
    for (i, left) in a.iter().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, right) in b.iter().enumerate() {
            let old = row[j + 1];
            row[j + 1] = (row[j] + 1)
                .min(old + 1)
                .min(diagonal + usize::from(left != right));
            diagonal = old;
        }
    }
    row[b.len()] * 2 <= a.len().max(b.len())
}

/// English cues are intentionally a limited baseline. No evidence -> uncertain.
/// This never produces a desired spelling or permission to apply a replacement.
pub fn retry_relation(
    previous: &str,
    current: &str,
    elapsed_ms: u64,
    same_scope: bool,
) -> RetryRelation {
    if !same_scope
        || elapsed_ms > 120_000
        || previous.len().max(current.len()) > MAX_LEARNING_TEXT_BYTES
    {
        return RetryRelation::Uncertain;
    }
    let lower = current.to_lowercase();
    if ["actually", "change that", "make it", "instead"]
        .iter()
        .any(|s| lower.starts_with(s))
    {
        return RetryRelation::ChangedIntent;
    }
    if ["also", "and then", "next", "add to that"]
        .iter()
        .any(|s| lower.starts_with(s))
    {
        return RetryRelation::Continuation;
    }
    let a = memory_words(previous);
    let b = memory_words(current);
    let shared = a.intersection(&b).count();
    let repair = [
        "i said ",
        "i meant ",
        "not what i said",
        "you heard ",
        "that was wrong",
        "let me repeat",
    ]
    .iter()
    .any(|s| lower.contains(s));
    if repair && shared >= 2 {
        RetryRelation::PossibleRepair
    } else {
        RetryRelation::Uncertain
    }
}

/// Similar successive attempts share support even when repair classification is uncertain.
/// Conservative grouping reduces evidence; it never asserts that a retry happened.
pub fn correlated_attempts(
    previous: &str,
    current: &str,
    elapsed_ms: u64,
    same_scope: bool,
) -> bool {
    if !same_scope
        || elapsed_ms > 120_000
        || previous.len().max(current.len()) > MAX_LEARNING_TEXT_BYTES
    {
        return false;
    }
    let a = memory_words(previous);
    let b = memory_words(current);
    let shared = a.intersection(&b).count();
    shared >= 2 && shared * 2 >= a.union(&b).count()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn spelling_edits_are_narrow_and_do_not_convert_intent_or_quotes() {
        assert!(automatic_spelling_pair("यह ज्यादा उपयोगी है", "यह ज़्यादा उपयोगी है").is_some());
        assert!(
            automatic_spelling_pair("Open read me in the repo", "Open README.md in the repo")
                .is_some()
        );
        assert!(
            automatic_spelling_pair("Meet Martellis tomorrow", "Meet Martellys tomorrow").is_some()
        );
        for (a, b) in [
            ("Send it Friday", "Send it Monday"),
            ("a cat", "a dog"),
            ("I can go", "I can't go"),
            ("Open 'read me'", "Open 'README.md'"),
            ("Open read me", "Open README.md and send it"),
            ("say foo", "say banana"),
        ] {
            assert!(automatic_spelling_pair(a, b).is_none(), "{a} -> {b}");
        }
    }
    #[test]
    fn retries_need_more_than_similarity_and_never_resolve_a_spelling() {
        let prior = "Open the atlas deployment";
        assert_eq!(
            retry_relation(prior, prior, 1000, true),
            RetryRelation::Uncertain
        );
        assert_eq!(
            retry_relation(prior, "I said open the atlas deployment", 1000, true),
            RetryRelation::PossibleRepair
        );
        assert_eq!(
            retry_relation(prior, "Also open the atlas deployment", 1000, true),
            RetryRelation::Continuation
        );
        assert_eq!(
            retry_relation(prior, "Actually open the atlas deployment", 1000, true),
            RetryRelation::ChangedIntent
        );
        assert_eq!(
            retry_relation(prior, "I said open the atlas deployment", 121000, true),
            RetryRelation::Uncertain
        );
        assert_eq!(
            retry_relation(prior, "I said open the atlas deployment", 1000, false),
            RetryRelation::Uncertain
        );
        assert!(correlated_attempts(prior, prior, 1000, true));
    }
}
