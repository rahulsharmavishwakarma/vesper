//! Text-level correction gate shared by every recognizer. It applies only exact
//! spoken-form → spelling pairs from the per-recording working set, and refuses to
//! rewrite a span that already reads as ordinary English. Sound-alike retrieval may
//! propose candidates elsewhere; it never decides here.
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::memory::{crosses_boundary, protected, quoted, tokens};
use crate::{
    GateChange, GateDecline, GateDeclined, GateResult, Language, TermTier, WorkingSet,
    MAX_NORMALIZATIONS,
};

/// CMUdict-derived (BSD-style, see `data/CMUDICT-LICENSE`), front-coded by
/// `scripts/generate-english-lexicon.py`. Includes inflections and common names,
/// which makes the ordinary-language test conservative.
const LEXICON: &str = include_str!("../data/english-lexicon.fc");
const MAX_GATE_TEXT_BYTES: usize = 400_000;
const MAX_GATE_TOKENS: usize = 8192;

struct Lexicon {
    words: String,
    ends: Vec<u32>,
}

impl Lexicon {
    fn decode() -> Self {
        let mut words = String::with_capacity(LEXICON.len() * 2);
        let mut ends = Vec::with_capacity(120_000);
        let mut previous = String::new();
        for line in LEXICON.lines() {
            let shared = usize::from(line.as_bytes()[0] - b'0');
            previous.truncate(shared);
            previous.push_str(&line[1..]);
            words.push_str(&previous);
            ends.push(words.len() as u32);
        }
        Self { words, ends }
    }

    fn word(&self, index: usize) -> &str {
        let start = index.checked_sub(1).map_or(0, |i| self.ends[i] as usize);
        &self.words[start..self.ends[index] as usize]
    }

    fn contains(&self, word: &str) -> bool {
        let (mut low, mut high) = (0, self.ends.len());
        while low < high {
            let middle = (low + high) / 2;
            match self.word(middle).cmp(word) {
                std::cmp::Ordering::Less => low = middle + 1,
                std::cmp::Ordering::Greater => high = middle,
                std::cmp::Ordering::Equal => return true,
            }
        }
        false
    }
}

fn lexicon() -> &'static Lexicon {
    static VALUE: OnceLock<Lexicon> = OnceLock::new();
    VALUE.get_or_init(Lexicon::decode)
}

/// A single English word the lexicon knows, including possessives, contractions
/// (`don't`, `we'll`, `I'm`) and hyphenated compounds of known words.
pub fn ordinary_word(word: &str) -> bool {
    let lower = word.to_lowercase().replace('’', "'");
    if lower.contains('-') {
        return lower
            .split('-')
            .all(|part| !part.is_empty() && ordinary_word(part));
    }
    let base = match lower.rsplit_once('\'') {
        Some((base, "s" | "m" | "ll" | "re" | "ve" | "d" | "t" | "")) => base,
        Some(_) => return false,
        None => &lower,
    };
    !base.is_empty() && base.bytes().all(|b| b.is_ascii_lowercase()) && lexicon().contains(base)
}

/// Every token is an ordinary English word, so the span already makes sense as speech.
pub fn ordinary_language(span: &str) -> bool {
    let words = tokens(span);
    !words.is_empty() && words.iter().all(|t| ordinary_word(t.text))
}

fn letters(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn edit_distance(a: &str, b: &str) -> usize {
    let b = b.chars().collect::<Vec<_>>();
    let mut row = (0..=b.len()).collect::<Vec<_>>();
    for (i, x) in a.chars().enumerate() {
        let mut prior = row[0];
        row[0] = i + 1;
        for (j, y) in b.iter().enumerate() {
            let old = row[j + 1];
            row[j + 1] = (row[j] + 1).min(old + 1).min(prior + usize::from(x != *y));
            prior = old;
        }
    }
    row[b.len()]
}

/// How the gate treats near-miss repairs of non-words (`vasper` → `Vesper`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NearMiss {
    Off,
    /// Record proposals in `GateResult::near_misses` without changing text.
    #[default]
    Shadow,
    Apply,
}

/// One token that is not an ordinary word, at least four letters, one edit from a
/// single working-set reference (two edits from eight letters), and not merely an
/// inflection of it. Ordinary words, protected classes and quotes are never touched.
fn near_miss<'a>(heard: &str, references: &'a [String]) -> Option<&'a str> {
    let heard_letters = letters(heard);
    if heard_letters.chars().count() < 4 || ordinary_word(heard) || protected(heard) {
        return None;
    }
    let bound = if heard_letters.chars().count() >= 8 {
        2
    } else {
        1
    };
    let mut matches = references
        .iter()
        .filter(|r| !r.contains(char::is_whitespace))
        .filter_map(|r| {
            let target = letters(r);
            // Either direction: `LLMs` vs `LLM`, `tokenizer` vs `tokenizers`.
            let inflection = ["s", "es", "ed", "ing", "er", "ers"].iter().any(|suffix| {
                heard_letters == format!("{target}{suffix}")
                    || target == format!("{heard_letters}{suffix}")
            });
            (target != heard_letters && !inflection)
                .then(|| (edit_distance(&heard_letters, &target), r.as_str()))
        })
        .filter(|(distance, _)| *distance <= bound)
        .collect::<Vec<_>>();
    matches.sort();
    match matches.as_slice() {
        [(_, only)] => Some(only),
        [(best, spelling), (next, _), ..] if best < next => Some(spelling),
        _ => None,
    }
}

/// Apply the working set's exact spoken-form normalizations under the safety rules:
/// - non-English or automatic language fails closed;
/// - protected classes, quotes and sentence/line boundaries are never crossed;
/// - an ordinary-language span changes only to a user-asserted spelling with the
///   same letters (`type script` → `TypeScript`), never to a different word;
/// - a mined (authored) term, or any snake/dotted identifier, rejoins several
///   words only when none of them is an ordinary word: `sherpa onnx` →
///   `sherpa_onnx`, never `ASR models` → `ASR_MODELS` (found replaying real
///   dictation history);
/// - overlapping candidates abstain. Replacement is single-pass and non-cascading.
pub fn correction_gate(text: &str, set: &WorkingSet, language: Language) -> GateResult {
    correction_gate_with(text, set, language, NearMiss::default())
}

pub fn correction_gate_with(
    text: &str,
    set: &WorkingSet,
    language: Language,
    near_misses: NearMiss,
) -> GateResult {
    let mut result = GateResult {
        text: text.to_owned(),
        ..GateResult::default()
    };
    if (set.normalizations.is_empty() && set.references.is_empty())
        || text.len() > MAX_GATE_TEXT_BYTES
    {
        return result;
    }
    let words = tokens(text);
    if words.len() > MAX_GATE_TOKENS {
        return result;
    }
    let lower = words
        .iter()
        .map(|t| t.text.to_lowercase())
        .collect::<Vec<_>>();
    let mut candidates = Vec::new();
    for item in set.normalizations.iter().take(MAX_NORMALIZATIONS) {
        let spoken = tokens(&item.spoken)
            .iter()
            .map(|t| t.text.to_lowercase())
            .collect::<Vec<_>>();
        if spoken.is_empty() || spoken.len() > words.len() || item.spelling.trim().is_empty() {
            continue;
        }
        for (position, slice) in words.windows(spoken.len()).enumerate() {
            if lower[position..position + spoken.len()] != spoken[..] {
                continue;
            }
            let (start, end) = (slice[0].start, slice[spoken.len() - 1].end);
            let heard = &text[start..end];
            if heard == item.spelling {
                continue;
            }
            let decline = |reason| GateDeclined {
                heard: heard.to_owned(),
                spelling: item.spelling.clone(),
                reason,
            };
            if language != Language::English {
                result
                    .declined
                    .push(decline(GateDecline::UnsupportedLanguage));
            } else if crosses_boundary(text, slice) || quoted(text, start) {
                result.declined.push(decline(GateDecline::Boundary));
            } else if protected(heard) || protected(&item.spelling) {
                result.declined.push(decline(GateDecline::ProtectedClass));
            } else if (ordinary_language(heard)
                && !(item.tier == TermTier::Asserted
                    && letters(heard) == letters(&item.spelling)
                    && !ordinary_word(&letters(&item.spelling))))
                || ((item.tier == TermTier::Authored || item.spelling.contains(['_', '.', ':']))
                    && slice.len() > 1
                    && slice.iter().any(|t| ordinary_word(t.text)))
            {
                result.declined.push(decline(GateDecline::OrdinaryLanguage));
            } else {
                candidates.push((start, end, item.spelling.clone(), item.term_id.clone()));
            }
        }
    }
    if language == Language::English && near_misses != NearMiss::Off {
        for word in &words {
            let covered = candidates
                .iter()
                .any(|(start, end, ..)| word.start < *end && *start < word.end);
            if covered || quoted(text, word.start) {
                continue;
            }
            if let Some(spelling) = near_miss(word.text, &set.references) {
                let change = GateChange {
                    start: word.start,
                    end: word.end,
                    heard: word.text.to_owned(),
                    spelling: spelling.to_owned(),
                    term_id: format!("reference:{spelling}"),
                };
                if near_misses == NearMiss::Apply {
                    candidates.push((
                        word.start,
                        word.end,
                        spelling.to_owned(),
                        change.term_id.clone(),
                    ));
                }
                result.near_misses.push(change);
            }
        }
    }
    candidates.sort();
    candidates.dedup_by(|a, b| a.0 == b.0 && a.1 == b.1 && a.2 == b.2);
    let mut changes = Vec::new();
    for (i, candidate) in candidates.iter().enumerate() {
        if candidates
            .iter()
            .enumerate()
            .any(|(j, other)| i != j && candidate.0 < other.1 && other.0 < candidate.1)
        {
            result.declined.push(GateDeclined {
                heard: text[candidate.0..candidate.1].to_owned(),
                spelling: candidate.2.clone(),
                reason: GateDecline::Ambiguous,
            });
        } else {
            changes.push(candidate.clone());
        }
    }
    for (start, end, spelling, _) in changes.iter().rev() {
        result.text.replace_range(*start..*end, spelling);
    }
    result.applied = changes
        .into_iter()
        .map(|(start, end, spelling, term_id)| GateChange {
            start,
            end,
            heard: text[start..end].to_owned(),
            spelling,
            term_id,
        })
        .collect();
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Normalization;

    fn set(items: &[(&str, &str, TermTier)]) -> WorkingSet {
        WorkingSet {
            normalizations: items
                .iter()
                .enumerate()
                .map(|(i, (spoken, spelling, tier))| Normalization {
                    spoken: (*spoken).into(),
                    spelling: (*spelling).into(),
                    term_id: format!("t{i}"),
                    tier: *tier,
                })
                .collect(),
            ..WorkingSet::default()
        }
    }

    #[test]
    fn lexicon_decodes_and_knows_inflections_but_not_jargon() {
        for word in [
            "point",
            "points",
            "making",
            "write",
            "notes",
            "Monday's",
            "don't",
            "I'm",
            "we’ll",
            "you're",
            "can't",
            "self-improvement",
            "drop-down",
        ] {
            assert!(ordinary_word(word), "{word}");
        }
        for word in [
            "onnx",
            "kubectl",
            "sortformer",
            "etcd",
            "ctl",
            "",
            "agents.md",
            "sherpa-onnx",
            "x'y",
            "-",
        ] {
            assert!(!ordinary_word(word), "{word}");
        }
        assert!(ordinary_language("see the point"));
        assert!(!ordinary_language("sherpa onnx"));
    }

    #[test]
    fn review_failures_cannot_be_rewritten_by_any_tier() {
        let tiers = [TermTier::Asserted, TermTier::Authored];
        for tier in tiers {
            let rules = set(&[
                ("see the point", "Cedar Point", tier),
                ("monday", "Mandy", tier),
                ("may", "Mae", tier),
                ("4b is free", "Forbes", tier),
                ("write", "Wright", tier),
                ("notes", "Nots", tier),
            ]);
            for raw in [
                "I see the point you are making.",
                "See you on Monday then.",
                "We launch in May.",
                "Room 4B is free.",
                "I will write it down.",
                "Please send the notes now.",
            ] {
                let out = correction_gate(raw, &rules, Language::English);
                assert_eq!(out.text, raw, "{tier:?}");
                assert!(out.applied.is_empty());
            }
        }
    }

    #[test]
    fn non_words_and_asserted_rejoins_apply_once_without_cascading() {
        let rules = set(&[
            ("sherpa onnx", "sherpa_onnx", TermTier::Authored),
            ("sortformer", "Sortformer", TermTier::Authored),
            ("type script", "TypeScript", TermTier::Asserted),
            ("cube control", "kubectl", TermTier::Asserted),
            ("for each", "forEach", TermTier::Authored),
        ]);
        let raw =
            "Link sherpa onnx, then run sortformer in type script. Use cube control for each pod.";
        let out = correction_gate(raw, &rules, Language::English);
        assert_eq!(
            out.text,
            "Link sherpa_onnx, then run Sortformer in TypeScript. Use cube control for each pod."
        );
        assert_eq!(out.applied.len(), 3);
        assert_eq!(
            &raw[out.applied[0].start..out.applied[0].end],
            "sherpa onnx"
        );
        assert!(out
            .declined
            .iter()
            .all(|d| d.reason == GateDecline::OrdinaryLanguage));
        assert_eq!(out.declined.len(), 2);
    }

    #[test]
    fn boundaries_quotes_protected_languages_and_overlaps_abstain() {
        let rules = set(&[("sherpa onnx", "sherpa_onnx", TermTier::Authored)]);
        for raw in ["sherpa. onnx", "sherpa\nonnx", "say \"sherpa onnx\" twice"] {
            assert_eq!(correction_gate(raw, &rules, Language::English).text, raw);
        }
        let hindi = correction_gate("sherpa onnx", &rules, Language::Hindi);
        assert_eq!(hindi.text, "sherpa onnx");
        assert_eq!(hindi.declined[0].reason, GateDecline::UnsupportedLanguage);
        assert_eq!(
            correction_gate("sherpa onnx", &rules, Language::Auto).text,
            "sherpa onnx"
        );
        let dated = set(&[("onnx friday", "OnnxFriday", TermTier::Asserted)]);
        assert_eq!(
            correction_gate("onnx friday", &dated, Language::English).declined[0].reason,
            GateDecline::ProtectedClass
        );
        let overlap = set(&[
            ("sherpa onnx", "sherpa_onnx", TermTier::Authored),
            ("onnx runtime", "onnxruntime", TermTier::Authored),
        ]);
        let out = correction_gate("sherpa onnx runtime", &overlap, Language::English);
        assert_eq!(out.text, "sherpa onnx runtime");
        assert_eq!(out.declined.len(), 2);
    }

    #[test]
    fn near_misses_repair_only_non_words_close_to_one_reference() {
        let set = WorkingSet {
            references: [
                "Vesper",
                "NVIDIA",
                "Gemma",
                "Colab",
                "diarization",
                "LLM",
                "Canada",
                "tokenizers",
            ]
            .map(String::from)
            .to_vec(),
            ..WorkingSet::default()
        };
        let raw = "Open vasper with Envidia GEMA in collab for DRization.";
        let shadow = correction_gate(raw, &set, Language::English);
        assert_eq!(shadow.text, raw);
        assert_eq!(shadow.near_misses.len(), 5);
        let applied = correction_gate_with(raw, &set, Language::English, NearMiss::Apply);
        assert_eq!(
            applied.text,
            "Open Vesper with NVIDIA Gemma in Colab for diarization."
        );
        for raw in [
            "We visited Kannada and Canada.",
            "Compare two LLMs today.",
            "Check the tokenizer also.",
            "Write vespers at dusk.",
            "Say \"vasper\" slowly.",
            "Send the notes now.",
        ] {
            let out = correction_gate_with(raw, &set, Language::English, NearMiss::Apply);
            assert_eq!(out.text, raw, "{raw}");
        }
        assert_eq!(
            correction_gate_with("open vasper", &set, Language::Hindi, NearMiss::Apply).text,
            "open vasper"
        );
        let tied = WorkingSet {
            references: vec!["Vesper".into(), "Vasser".into()],
            ..WorkingSet::default()
        };
        assert!(correction_gate("open vasper", &tied, Language::English)
            .near_misses
            .is_empty());
    }

    #[test]
    fn mined_identifiers_never_rejoin_prose_words() {
        let rules = set(&[
            ("asr models", "ASR_MODELS", TermTier::Authored),
            ("asr model", "AsrModel", TermTier::Authored),
            ("sherpa onnx", "sherpa_onnx", TermTier::Authored),
        ]);
        let raw = "We compare ASR models and one ASR model with sherpa onnx.";
        let out = correction_gate(raw, &rules, Language::English);
        assert_eq!(
            out.text,
            "We compare ASR models and one ASR model with sherpa_onnx."
        );
        let asserted = set(&[("asr models", "ASR_MODELS", TermTier::Asserted)]);
        assert_eq!(
            correction_gate("compare ASR models", &asserted, Language::English).text,
            "compare ASR models"
        );
    }
}
