//! Frozen-weight automatic spelling admission and provisional interaction evidence.
//! Scores here are deterministic policy rules, not calibrated model confidence.
use crate::{
    correction_suggestions, memory_words, CorrectionPair, Language, PersonalizationProfile,
    VocabularyEntry,
};
use serde::{Deserialize, Serialize};

pub const LEARNING_POLICY_VERSION: u32 = 5;
pub const MAX_LEARNING_TEXT_BYTES: usize = 8192;
pub const MAX_TEACHING_TERMS: usize = 16;
pub const MAX_TEACHING_TERM_BYTES: usize = 64;

/// Compact observable cues, not a transcript, embedding or semantic intent label.
/// Alias, preferred spelling and project words cannot prove their own context.
pub fn teaching_context_terms(before: &str, pair: &CorrectionPair, project: &str) -> Vec<String> {
    if before.len() > MAX_LEARNING_TEXT_BYTES {
        return vec![];
    }
    let excluded = memory_words(&format!("{} {} {project}", pair.heard, pair.intended));
    memory_words(before)
        .into_iter()
        .filter(|term| {
            term.len() <= MAX_TEACHING_TERM_BYTES
                && term.chars().any(char::is_alphabetic)
                && !excluded.contains(term)
                && !matches!(
                    term.as_str(),
                    "open"
                        | "run"
                        | "show"
                        | "use"
                        | "in"
                        | "for"
                        | "on"
                        | "of"
                        | "with"
                        | "this"
                        | "that"
                        | "it"
                )
                && crate::memory_phrase_occurs(before, term)
        })
        .take(MAX_TEACHING_TERMS)
        .collect()
}

/// Matching also uses pre-cleanup text, never model-generated context or feedback targets.
pub fn teaching_context_matches<'a>(
    teaching: impl IntoIterator<Item = &'a String>,
    raw: &str,
    pair: &CorrectionPair,
    project: &str,
) -> bool {
    let current = teaching_context_terms(raw, pair, project);
    teaching.into_iter().any(|term| current.contains(term))
}

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
    /// Bias provenance: hash of the hint list this decode received; empty when
    /// the recognizer was not biased.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub hint_hash: String,
    /// Phrases this decode was biased toward. Their appearance in its output is
    /// never counted as an observation or usage.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hinted_phrases: Vec<String>,
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
    let intended_words = pair.intended.split_whitespace().count();
    if pair.heard.split_whitespace().count() > 3 || intended_words > 3 {
        return None;
    }
    // Multiple target words are a boundary repair, not a phrase-rewriting rule.
    let joined = |text: &str| text.split_whitespace().collect::<String>().to_lowercase();
    if intended_words > 1 && joined(&pair.heard) != joined(&pair.intended) {
        return None;
    }
    let common = [
        "good", "bad", "yes", "okay", "ok", "cat", "dog", "send", "delete", "add", "remove",
        "start", "stop", "more", "less", "formal", "casual", "left", "right", "public", "private",
        "this", "that", "these", "those", "who", "what", "why", "how", "where", "when", "here",
        "there", "and", "or", "but", "if", "then", "can",
    ];
    if [&pair.heard, &pair.intended].iter().any(|text| {
        text.split_whitespace()
            .any(|word| common.contains(&word.to_lowercase().as_str()))
    }) {
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
    let replay = profile.personalize(before, Language::English);
    (!replay.evidence.applied.is_empty()
        && crate::memory::spelling_replay_matches(&replay.text, after))
    .then_some(pair)
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
    // Short acronym repairs still need ordinary independent correction support.
    // Lowercase short words never inherit this exception.
    if (3..=8).contains(&a.len())
        && a.len() == b.len()
        && a.bytes().chain(b.bytes()).all(|c| c.is_ascii_uppercase())
        && a.bytes().zip(b.bytes()).filter(|(a, b)| a != b).count() == 1
    {
        return true;
    }
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
    fn teaching_cues_exclude_self_evidence_quotes_and_uninformative_words() {
        let pair = CorrectionPair {
            heard: "read me".into(),
            intended: "README.md".into(),
        };
        let cues = teaching_context_terms(
            "Open read me in repo for deployment 'secret' 2026",
            &pair,
            "repo",
        );
        assert_eq!(cues, ["deployment"]);
        assert!(teaching_context_matches(
            &cues,
            "Review read me deployment",
            &pair,
            "repo"
        ));
        for text in [
            "Open read me in repo",
            "Open read me for shopping",
            "Open read me 'deployment'",
        ] {
            assert!(!teaching_context_matches(&cues, text, &pair, "repo"));
        }
    }

    #[test]
    fn teaching_cues_are_unicode_safe_deduplicated_and_bounded() {
        let pair = CorrectionPair {
            heard: "कुबेरनेटस".into(),
            intended: "कुबेरनेट्स".into(),
        };
        let cues = teaching_context_terms("कुबेरनेटस परिनियोजन परिनियोजन", &pair, "");
        assert_eq!(cues, ["परिनियोजन"]);
        let many = (0..100)
            .map(|i| format!("topic{i}"))
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(
            teaching_context_terms(&many, &pair, "").len(),
            MAX_TEACHING_TERMS
        );
        let oversized_term = "अ".repeat(22);
        assert!(teaching_context_terms(&oversized_term, &pair, "").is_empty());
        assert!(
            teaching_context_terms(&"a".repeat(MAX_LEARNING_TEXT_BYTES + 1), &pair, "").is_empty()
        );
    }
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
    fn corrected_names_acronyms_and_spelling_with_formatting_are_candidates() {
        for (before, after, heard, intended) in [
            (
                "Route through LakeviewGardens station",
                "Route through Lakeview Gardens station",
                "LakeviewGardens",
                "Lakeview Gardens",
            ),
            (
                "Meet at WhiteRiverFalls station",
                "Meet at White River Falls station",
                "WhiteRiverFalls",
                "White River Falls",
            ),
            (
                "Inspect the JXR driver",
                "Inspect the JXL driver",
                "JXR",
                "JXL",
            ),
            (
                "deploy kubernets, to staging",
                "Deploy kubernetes to staging.",
                "kubernets",
                "kubernetes",
            ),
        ] {
            assert_eq!(
                automatic_spelling_pair(before, after),
                Some(CorrectionPair {
                    heard: heard.into(),
                    intended: intended.into(),
                }),
                "{before} -> {after}"
            );
        }
    }

    #[test]
    fn correction_admission_keeps_meaning_and_structure_guards() {
        for (before, after) in [
            ("This build works", "These build works"),
            ("Those builds work", "These builds work"),
            ("Ask WHO tomorrow", "Ask WHY tomorrow"),
            ("The CAT label", "The CAR label"),
            (
                "Route through NorthGate station",
                "Route through South Gate station",
            ),
            ("Deploy kubernets tomorrow", "Deploy kubernetes today"),
            ("Deploy kubernets twice", "Do not deploy kubernetes twice"),
            (
                "Deploy kubernets to 12 nodes",
                "Deploy kubernetes to 21 nodes",
            ),
            (
                "Deploy 'kubernets' to staging",
                "Deploy 'kubernetes' to staging",
            ),
            (
                "Deploy kubernets to staging",
                "Deploy 'kubernetes' to staging",
            ),
            (
                "Deploy kubernets to staging?",
                "Deploy kubernetes to staging.",
            ),
            (
                "Deploy kubernets. Restart staging",
                "Deploy kubernetes, restart staging",
            ),
            (
                "Deploy kubernets\nRestart staging",
                "Deploy kubernetes Restart staging",
            ),
            (
                "Deploy kubernets and inspect kubernets",
                "Deploy kubernetes and inspect kubernets",
            ),
            ("Inspect JXR driver", "Inspect JXL driver and restart"),
        ] {
            assert!(
                automatic_spelling_pair(before, after).is_none(),
                "{before} -> {after}"
            );
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
