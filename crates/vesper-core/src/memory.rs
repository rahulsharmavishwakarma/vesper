//! Bounded on-device memory operations. No model handles, network or learned weights.
use std::collections::{BTreeSet, HashSet};

use serde::{Deserialize, Serialize};

use crate::{language_matches, Language, OutputIntent, PersonalizationProfile};

const MAX_COMPARE_WORDS: usize = 512;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CorrectionPair {
    pub heard: String,
    pub intended: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntentSuggestion {
    pub intent: OutputIntent,
    pub supporting_examples: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryApplication {
    pub entry_id: String,
    pub occurrences: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryEvidence {
    pub profile_revision: u64,
    pub applied: Vec<MemoryApplication>,
    pub ambiguous_matches: usize,
    pub suggested_intent: Option<IntentSuggestion>,
    pub corrections_enabled: bool,
    #[serde(default)]
    pub budget_exceeded: bool,
    /// Correction-gate changes in this text, with offsets into the gate input.
    /// Authorship record: these spans were written by the gate, not the user.
    #[serde(default)]
    pub gate: Vec<crate::GateChange>,
    /// Hash of the recognizer hint list used for this decode (bias provenance).
    #[serde(default)]
    pub hint_hash: String,
    /// Near-miss proposals recorded in shadow; never applied to this text.
    #[serde(default)]
    pub gate_shadow: Vec<crate::GateChange>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonalizedText {
    pub text: String,
    pub evidence: MemoryEvidence,
}

pub(crate) struct Token<'a> {
    pub(crate) text: &'a str,
    pub(crate) start: usize,
    pub(crate) end: usize,
}

fn edge_punctuation(c: char) -> bool {
    matches!(
        c,
        '.' | ','
            | ';'
            | ':'
            | '!'
            | '?'
            | '"'
            | '\''
            | '('
            | ')'
            | '['
            | ']'
            | '{'
            | '}'
            | '“'
            | '”'
            | '‘'
            | '’'
            | '।'
            | '॥'
    )
}

/// Whitespace tokens preserve combining marks and internal identifier punctuation.
pub(crate) fn tokens(text: &str) -> Vec<Token<'_>> {
    let mut offset = 0;
    text.split_whitespace()
        .filter_map(|part| {
            let original_start = offset + text[offset..].find(part)?;
            offset = original_start + part.len();
            let trimmed = part.trim_matches(edge_punctuation);
            if trimmed.is_empty() {
                return None;
            }
            let start = original_start + part.find(trimmed)?;
            Some(Token {
                text: trimmed,
                start,
                end: start + trimmed.len(),
            })
        })
        .collect()
}

pub fn memory_words(text: &str) -> BTreeSet<String> {
    tokens(text)
        .into_iter()
        .map(|t| t.text.to_lowercase())
        .filter(|w| {
            !matches!(
                w.as_str(),
                "the" | "a" | "an" | "is" | "to" | "and" | "please" | "can" | "you" | "i"
            )
        })
        .collect()
}

pub fn project_matches(entry: &str, selected: &str) -> bool {
    entry.trim().is_empty() || entry.trim().to_lowercase() == selected.trim().to_lowercase()
}

pub fn context_matches(context: &str, text: &str) -> bool {
    let tags = memory_words(context);
    tags.is_empty() || !tags.is_disjoint(&memory_words(text))
}

/// Exact, unquoted token-span occurrence; used for usage, never correctness labels.
pub fn memory_phrase_occurs(text: &str, phrase: &str) -> bool {
    if text.len() > 8192 || phrase.is_empty() {
        return false;
    }
    let needle = tokens(phrase);
    let haystack = tokens(text);
    !needle.is_empty()
        && haystack.windows(needle.len()).any(|span| {
            !quoted(text, span[0].start)
                && !crosses_boundary(text, span)
                && span
                    .iter()
                    .zip(&needle)
                    .all(|(a, b)| a.text.to_lowercase() == b.text.to_lowercase())
        })
}

pub(crate) fn protected(text: &str) -> bool {
    text.chars().any(char::is_numeric)
        || tokens(text).iter().any(|t| {
            let word = t.text.to_lowercase().replace('’', "'");
            word.ends_with("n't")
                || matches!(
                    word.as_str(),
                    "i" | "you"
                        | "he"
                        | "she"
                        | "it"
                        | "we"
                        | "they"
                        | "a"
                        | "an"
                        | "the"
                        | "am"
                        | "is"
                        | "are"
                        | "was"
                        | "were"
                        | "be"
                        | "been"
                        | "being"
                        | "have"
                        | "has"
                        | "had"
                        | "है"
                        | "हैं"
                        | "था"
                        | "थे"
                        | "थी"
                        | "यह"
                        | "वह"
                        | "मैं"
                        | "हम"
                        | "तुम"
                        | "आप"
                        | "je"
                        | "tu"
                        | "il"
                        | "elle"
                        | "nous"
                        | "vous"
                        | "ils"
                        | "elles"
                        | "suis"
                        | "est"
                        | "sommes"
                        | "êtes"
                        | "sont"
                        | "no"
                        | "not"
                        | "never"
                        | "don't"
                        | "dont"
                        | "can't"
                        | "cannot"
                        | "without"
                        | "maybe"
                        | "might"
                        | "perhaps"
                        | "sorry"
                        | "actually"
                        | "rather"
                        | "instead"
                        | "monday"
                        | "tuesday"
                        | "wednesday"
                        | "thursday"
                        | "friday"
                        | "saturday"
                        | "sunday"
                        | "today"
                        | "tomorrow"
                        | "yesterday"
                        | "january"
                        | "february"
                        | "march"
                        | "april"
                        | "may"
                        | "june"
                        | "july"
                        | "august"
                        | "september"
                        | "october"
                        | "november"
                        | "december"
                        | "one"
                        | "two"
                        | "three"
                        | "four"
                        | "five"
                        | "six"
                        | "seven"
                        | "eight"
                        | "nine"
                        | "ten"
                        | "hundred"
                        | "thousand"
                        | "million"
                        | "billion"
                        | "नहीं"
                        | "मत"
                        | "शायद"
                        | "कल"
                        | "आज"
                        | "एक"
                        | "दो"
                        | "तीन"
                        | "non"
                        | "pas"
                        | "jamais"
                        | "peut-être"
                        | "demain"
                        | "hier"
                )
        })
}

pub(crate) fn crosses_boundary(text: &str, words: &[Token<'_>]) -> bool {
    words.windows(2).any(|w| {
        text[w[0].end..w[1].start]
            .chars()
            .any(|c| !c.is_whitespace() || c == '\n' || c == '\r')
    })
}

// Formatting an initialism is not evidence for a recurring recognition confusion.
fn same_initialism(a: &str, b: &str) -> bool {
    let dotted = |s: &str| {
        s.contains('.')
            && s.split('.')
                .filter(|s| !s.is_empty())
                .all(|s| s.len() == 1 && s.chars().all(|c| c.is_ascii_uppercase()))
    };
    let letters = |s: &str| s.chars().filter(|c| *c != '.').collect::<String>();
    let left = letters(a);
    let right = letters(b);
    (dotted(a) || dotted(b))
        && (2..=8).contains(&left.len())
        && left.chars().all(|c| c.is_ascii_uppercase())
        && left == right
}

pub fn correction_suggestions(input: &str, reviewed: &str) -> Vec<CorrectionPair> {
    let a = tokens(input);
    let b = tokens(reviewed);
    if a.is_empty() || b.is_empty() || a.len().max(b.len()) > MAX_COMPARE_WORDS {
        return vec![];
    }
    let prefix = a
        .iter()
        .zip(&b)
        .take_while(|(a, b)| a.text.to_lowercase() == b.text.to_lowercase())
        .count();
    let suffix = a[prefix..]
        .iter()
        .rev()
        .zip(b[prefix..].iter().rev())
        .take_while(|(a, b)| a.text.to_lowercase() == b.text.to_lowercase())
        .count();
    let a = &a[prefix..a.len() - suffix];
    let b = &b[prefix..b.len() - suffix];
    if a.is_empty()
        || b.is_empty()
        || a.len().max(b.len()) > 8
        || crosses_boundary(input, a)
        || crosses_boundary(reviewed, b)
    {
        return vec![];
    }
    let heard = input[a[0].start..a.last().unwrap().end].to_owned();
    let intended = reviewed[b[0].start..b.last().unwrap().end].to_owned();
    if heard == intended
        || same_initialism(&heard, &intended)
        || heard.len().max(intended.len()) > 128
        || protected(&heard)
        || protected(&intended)
    {
        return vec![];
    }
    vec![CorrectionPair { heard, intended }]
}

/// Only admission may ignore minor presentation edits; live replacement remains exact.
/// Keep lexical order, token boundaries, quotes, newlines and sentence structure.
pub(crate) fn spelling_replay_matches(replayed: &str, edited: &str) -> bool {
    let a = tokens(replayed);
    let b = tokens(edited);
    if a.len() != b.len() {
        return false;
    }
    let gap = |text: &str, terminal: bool| {
        let mut value: String = text
            .chars()
            .filter(|c| !matches!(c, ' ' | '\t' | ','))
            .collect();
        if terminal && value == "." {
            value.clear();
        }
        value
    };
    let (mut a_end, mut b_end) = (0, 0);
    for (left, right) in a.iter().zip(&b) {
        if left.text.to_lowercase() != right.text.to_lowercase()
            || gap(&replayed[a_end..left.start], false) != gap(&edited[b_end..right.start], false)
        {
            return false;
        }
        a_end = left.end;
        b_end = right.end;
    }
    gap(&replayed[a_end..], true) == gap(&edited[b_end..], true)
}

/// Used for bounded replay, not a replacement for the native ASR qualification suite.
pub fn word_edits(reference: &str, hypothesis: &str) -> Option<usize> {
    let a = tokens(reference)
        .into_iter()
        .map(|t| t.text.to_lowercase())
        .collect::<Vec<_>>();
    let b = tokens(hypothesis)
        .into_iter()
        .map(|t| t.text.to_lowercase())
        .collect::<Vec<_>>();
    if a.len().max(b.len()) > MAX_COMPARE_WORDS {
        return None;
    }
    let mut row = (0..=b.len()).collect::<Vec<_>>();
    for (i, word) in a.iter().enumerate() {
        let mut prior = row[0];
        row[0] = i + 1;
        for (j, other) in b.iter().enumerate() {
            let old = row[j + 1];
            row[j + 1] = (row[j] + 1)
                .min(old + 1)
                .min(prior + usize::from(word != other));
            prior = old;
        }
    }
    Some(row[b.len()])
}

pub(crate) fn quoted(text: &str, position: usize) -> bool {
    let mut open = None;
    let mut previous = ' ';
    let mut escaped = false;
    let mut chars = text[..position].chars().peekable();
    while let Some(c) = chars.next() {
        if escaped {
            escaped = false;
            previous = c;
            continue;
        }
        if c == '\\' {
            escaped = true;
            previous = c;
            continue;
        }
        let apostrophe = matches!(c, '\'' | '’')
            && previous.is_alphanumeric()
            && chars.peek().is_some_and(|c| c.is_alphanumeric());
        match c {
            '"' | '“' | '”' => {
                if open == Some('"') {
                    open = None;
                } else if open.is_none() {
                    open = Some('"');
                }
            }
            '\'' | '‘' | '’'
                if !apostrophe && (!previous.is_alphanumeric() || open == Some('\'')) =>
            {
                if open == Some('\'') {
                    open = None;
                } else if open.is_none() {
                    open = Some('\'');
                }
            }
            _ => {}
        }
        previous = c;
    }
    open.is_some()
}

impl PersonalizationProfile {
    /// Output form is proposed only when at least two independent remembered sessions agree.
    /// This score is retrieval agreement, not a calibrated probability.
    pub fn suggest_intent(&self, text: &str, language: Language) -> Option<IntentSuggestion> {
        if text.len() > 400_000 {
            return None;
        }
        let query = memory_words(text);
        let mut matches = self
            .examples
            .iter()
            .filter(|e| {
                e.enabled
                    && language_matches(e.language, language)
                    && project_matches(&e.project, &self.active_project)
            })
            .filter_map(|e| {
                let terms = memory_words(&e.input);
                let intersection = terms.intersection(&query).count();
                let union = terms.union(&query).count().max(1);
                (intersection >= 2 && intersection * 3 >= union)
                    .then_some((intersection * 1000 / union, e))
            })
            .collect::<Vec<_>>();
        matches.sort_by(|(a, ea), (b, eb)| b.cmp(a).then_with(|| ea.id.cmp(&eb.id)));
        let mut sessions = HashSet::new();
        let selected = matches
            .into_iter()
            .filter(|(_, e)| sessions.insert(&e.session_id))
            .take(3)
            .collect::<Vec<_>>();
        let intent = selected.first()?.1.intent;
        if selected.len() < 2 || selected.iter().any(|(_, e)| e.intent != intent) {
            return None;
        }
        Some(IntentSuggestion {
            intent,
            supporting_examples: selected.len(),
        })
    }

    pub fn effective_intent(&self, text: &str, language: Language) -> OutputIntent {
        if self.use_remembered_intent {
            self.suggest_intent(text, language)
                .map(|s| s.intent)
                .unwrap_or(self.output_intent)
        } else {
            self.output_intent
        }
    }

    /// Apply only explicitly stored aliases; never fuzzy-match, cascade replacements or infer an accent.
    /// This stage is independent of cleanup/model capabilities and leaves its source untouched.
    pub fn personalize(&self, text: &str, language: Language) -> PersonalizedText {
        let mut result = PersonalizedText {
            text: text.to_owned(),
            evidence: MemoryEvidence {
                profile_revision: self.revision,
                suggested_intent: self.suggest_intent(text, language),
                corrections_enabled: self.apply_corrections,
                ..MemoryEvidence::default()
            },
        };
        if text.len() > 400_000 {
            result.evidence.budget_exceeded = true;
            return result;
        }
        if !self.apply_corrections || self.validate().is_err() {
            return result;
        }
        let words = tokens(text);
        if words.len() > 8192 {
            result.evidence.budget_exceeded = true;
            return result;
        }
        let context = format!("{} {}", self.active_project, text);
        let context_terms = memory_words(&context);
        let lower_words = words
            .iter()
            .map(|t| t.text.to_lowercase())
            .collect::<Vec<_>>();
        let mut candidates = Vec::new();
        for entry in &self.vocabulary {
            if !entry.enabled
                || !language_matches(entry.language, language)
                || !project_matches(&entry.project, &self.active_project)
                || (!entry.context.trim().is_empty()
                    && !memory_words(&entry.context).is_empty()
                    && memory_words(&entry.context).is_disjoint(&context_terms))
                || protected(&entry.phrase)
            {
                continue;
            }
            for alias in &entry.aliases {
                if protected(alias) {
                    continue;
                }
                let expected = tokens(alias);
                if expected.is_empty() || expected.len() > words.len() {
                    continue;
                }
                let lower_expected = expected
                    .iter()
                    .map(|t| t.text.to_lowercase())
                    .collect::<Vec<_>>();
                for (position, slice) in words.windows(expected.len()).enumerate() {
                    if lower_words[position..position + expected.len()] != lower_expected {
                        continue;
                    }
                    let start = slice[0].start;
                    let end = slice.last().unwrap().end;
                    // No matching across punctuation, sentence boundaries, line breaks or quoted text.
                    if crosses_boundary(text, slice)
                        || quoted(text, start)
                        || text[start..end] == entry.phrase
                    {
                        continue;
                    }
                    candidates.push((start, end, entry));
                    if candidates.len() > 512 {
                        result.evidence.budget_exceeded = true;
                        return result;
                    }
                }
            }
        }
        candidates.sort_by(|a, b| (a.0, a.1, &a.2.id).cmp(&(b.0, b.1, &b.2.id)));
        candidates.dedup_by(|a, b| a.0 == b.0 && a.1 == b.1 && a.2.phrase == b.2.phrase);
        let mut changes = Vec::new();
        for (i, candidate) in candidates.iter().enumerate() {
            if candidates
                .iter()
                .enumerate()
                .any(|(j, other)| i != j && candidate.0 < other.1 && other.0 < candidate.1)
            {
                result.evidence.ambiguous_matches += 1;
            } else {
                changes.push(*candidate);
                if changes.len() > 64 {
                    result.evidence.budget_exceeded = true;
                    return result;
                }
            }
        }
        for (start, end, entry) in changes.into_iter().rev() {
            result.text.replace_range(start..end, &entry.phrase);
            if let Some(applied) = result
                .evidence
                .applied
                .iter_mut()
                .find(|a| a.entry_id == entry.id)
            {
                applied.occurrences += 1;
            } else {
                result.evidence.applied.push(MemoryApplication {
                    entry_id: entry.id.clone(),
                    occurrences: 1,
                });
            }
        }
        result
            .evidence
            .applied
            .sort_by(|a, b| a.entry_id.cmp(&b.entry_id));
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CleanupExample, VocabularyEntry};

    fn profile() -> PersonalizationProfile {
        PersonalizationProfile {
            vocabulary: vec![VocabularyEntry {
                id: "term".into(),
                phrase: "kubectl".into(),
                aliases: vec!["cube control".into()],
                context: "Kubernetes".into(),
                enabled: true,
                ..VocabularyEntry::default()
            }],
            ..PersonalizationProfile::default()
        }
    }

    #[test]
    fn aliases_are_scoped_non_cascading_and_preserve_raw_offsets() {
        let p = profile();
        let raw = "In Kubernetes, run cube control. Keep the other cube.";
        let result = p.personalize(raw, Language::English);
        assert_eq!(
            result.text,
            "In Kubernetes, run kubectl. Keep the other cube."
        );
        assert_eq!(result.evidence.applied[0].occurrences, 1);
        for negative in [
            "Use cube control.",
            "Kubernetes: cube, control",
            "Kubernetes: cube\ncontrol",
            "Kubernetes: \"cube control\"",
            "Kubernetes cube controls",
        ] {
            assert_eq!(p.personalize(negative, Language::English).text, negative);
        }
        let mut p = p;
        p.vocabulary[0].project = "work".into();
        assert_eq!(p.personalize(raw, Language::English).text, raw);
        p.active_project = "work".into();
        assert_ne!(p.personalize(raw, Language::English).text, raw);
        p.vocabulary[0].enabled = false;
        assert_eq!(p.personalize(raw, Language::English).text, raw);
    }

    #[test]
    fn conflicts_dates_and_negation_abstain() {
        let mut p = profile();
        let mut other = p.vocabulary[0].clone();
        other.id = "other".into();
        other.phrase = "CubeControl".into();
        p.vocabulary.push(other);
        let raw = "Kubernetes cube control";
        let result = p.personalize(raw, Language::English);
        assert_eq!(result.text, raw);
        assert_eq!(result.evidence.ambiguous_matches, 2);
        for (raw, target) in [
            ("Friday", "Monday"),
            ("not ready", "ready"),
            ("maybe", "certainly"),
            ("cost 15", "cost 50"),
        ] {
            assert!(correction_suggestions(raw, target).is_empty());
        }
    }

    #[test]
    fn unicode_spans_keep_combining_marks_and_avoid_substrings() {
        let p = PersonalizationProfile {
            vocabulary: vec![VocabularyEntry {
                id: "hi".into(),
                phrase: "कुबेक्टल".into(),
                aliases: vec!["क्यूब कंट्रोल".into()],
                enabled: true,
                language: Language::Hindi,
                ..VocabularyEntry::default()
            }],
            ..PersonalizationProfile::default()
        };
        assert_eq!(
            p.personalize("क्यूब कंट्रोल चलाओ।", Language::Hindi).text,
            "कुबेक्टल चलाओ।"
        );
        assert_eq!(
            p.personalize("क्यूब कंट्रोल चलाओ।", Language::English).text,
            "क्यूब कंट्रोल चलाओ।"
        );
        let pairs = correction_suggestions("run cube control now", "run kubectl now");
        assert_eq!(
            pairs,
            vec![CorrectionPair {
                heard: "cube control".into(),
                intended: "kubectl".into()
            }]
        );
    }

    #[test]
    fn intent_requires_independent_agreement_and_opt_in() {
        let mut p = PersonalizationProfile::default();
        for id in ["one", "two"] {
            p.examples.push(CleanupExample {
                id: id.into(),
                session_id: id.into(),
                review_revision_id: id.into(),
                input: "update atlas deployment".into(),
                output: "Update Atlas deployment.".into(),
                intent: OutputIntent::Technical,
                enabled: true,
                ..CleanupExample::default()
            });
        }
        assert_eq!(
            p.suggest_intent("update atlas deployment", Language::English)
                .unwrap()
                .supporting_examples,
            2
        );
        assert_eq!(
            p.effective_intent("update atlas deployment", Language::English),
            OutputIntent::Verbatim
        );
        p.use_remembered_intent = true;
        assert_eq!(
            p.effective_intent("update atlas deployment", Language::English),
            OutputIntent::Technical
        );
        p.examples[1].intent = OutputIntent::Message;
        assert!(p
            .suggest_intent("update atlas deployment", Language::English)
            .is_none());
    }

    #[test]
    fn corpus_corrections_do_not_turn_pronouns_or_grammar_into_global_aliases() {
        for (raw, reviewed) in [("I", "Ama"), ("है", "हैं"), ("the", "they")] {
            assert!(correction_suggestions(raw, reviewed).is_empty());
            let mut p = profile();
            p.vocabulary[0].context.clear();
            p.vocabulary[0].aliases = vec![raw.into()];
            p.vocabulary[0].phrase = reviewed.into();
            assert_eq!(p.personalize(raw, Language::English).text, raw);
        }
        assert_eq!(
            correction_suggestions("use cube control now", "Use kubectl now")[0].heard,
            "cube control"
        );
    }

    #[test]
    fn quoted_contractions_and_escaped_quotes_do_not_expose_aliases() {
        let mut p = profile();
        p.vocabulary[0].context.clear();
        for raw in [
            "'don't change cube control'",
            "‘don’t change cube control’",
            "\"say \\\" cube control\"",
            "'we can't change cube control'",
        ] {
            assert_eq!(p.personalize(raw, Language::English).text, raw);
            let outside = format!("{raw} then cube control.");
            assert_eq!(
                p.personalize(&outside, Language::English).text,
                format!("{raw} then kubectl.")
            );
        }
        assert_eq!(
            p.personalize("don't change cube control", Language::English)
                .text,
            "don't change kubectl"
        );
    }

    #[test]
    fn negation_variants_and_unusable_spans_are_not_learned() {
        for (raw, reviewed) in [
            ("don’t", "do"),
            ("can’t", "can"),
            ("won't", "will"),
            ("wouldn’t", "would"),
            ("UN", "U.N."),
            ("USA", "U.S.A."),
            (
                "practised dialogue, encouraged",
                "practiced dialogue, encourage",
            ),
        ] {
            assert!(correction_suggestions(raw, reviewed).is_empty(), "{raw}");
        }
        let mut p = profile();
        p.vocabulary[0].context.clear();
        p.vocabulary[0].aliases = vec!["don’t".into()];
        p.vocabulary[0].phrase = "do".into();
        assert_eq!(
            p.personalize("don’t deploy", Language::English).text,
            "don’t deploy"
        );
        assert_eq!(
            correction_suggestions("open read me", "open README.md")[0].intended,
            "README.md"
        );
    }

    #[test]
    fn correction_budgets_abstain_without_partial_edits() {
        let mut p = profile();
        p.vocabulary[0].context.clear();
        for raw in [
            "cube control ".repeat(65),
            "word ".repeat(8193),
            "x".repeat(400_001),
        ] {
            let result = p.personalize(&raw, Language::English);
            assert_eq!(result.text, raw);
            assert!(result.evidence.budget_exceeded);
            assert!(result.evidence.applied.is_empty());
        }
    }

    #[test]
    fn old_profiles_load_with_no_new_corrections_and_future_schemas_fail() {
        let p: PersonalizationProfile = serde_json::from_str(r#"{"schemaVersion":1,"revision":7,"vocabulary":[{"id":"old","phrase":"Atlas","language":"en","context":"","enabled":true}]}"#).unwrap();
        assert!(p.validate().is_ok());
        assert_eq!(p.personalize("atlas", Language::English).text, "atlas");
        let mut future = p;
        future.schema_version = 5;
        assert!(future.validate().is_err());
        assert_eq!(word_edits("one two", "one"), Some(1));
    }
}
