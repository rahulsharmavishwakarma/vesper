//! Text-level correction gate shared by every recognizer. It applies only exact
//! spoken-form → spelling pairs from the per-recording working set, and refuses to
//! rewrite a span that already reads as ordinary English. Sound-alike retrieval may
//! propose candidates elsewhere; it never decides here.
use std::sync::OnceLock;

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

/// A single English word the lexicon knows, including simple possessives.
pub fn ordinary_word(word: &str) -> bool {
    let lower = word.to_lowercase().replace('’', "'");
    let base = lower
        .strip_suffix("'s")
        .or_else(|| lower.strip_suffix('\''))
        .unwrap_or(&lower);
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

/// Apply the working set's exact spoken-form normalizations under the safety rules:
/// - non-English or automatic language fails closed;
/// - protected classes, quotes and sentence/line boundaries are never crossed;
/// - an ordinary-language span changes only to a user-asserted spelling with the
///   same letters (`type script` → `TypeScript`), never to a different word;
/// - overlapping candidates abstain. Replacement is single-pass and non-cascading.
pub fn correction_gate(text: &str, set: &WorkingSet, language: Language) -> GateResult {
    let mut result = GateResult {
        text: text.to_owned(),
        ..GateResult::default()
    };
    if set.normalizations.is_empty() || text.len() > MAX_GATE_TEXT_BYTES {
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
            } else if ordinary_language(heard)
                && !(item.tier == TermTier::Asserted
                    && letters(heard) == letters(&item.spelling)
                    && !ordinary_word(&letters(&item.spelling)))
            {
                result.declined.push(decline(GateDecline::OrdinaryLanguage));
            } else {
                candidates.push((start, end, item));
            }
        }
    }
    candidates.sort_by(|a, b| (a.0, a.1, &a.2.spelling).cmp(&(b.0, b.1, &b.2.spelling)));
    candidates.dedup_by(|a, b| a.0 == b.0 && a.1 == b.1 && a.2.spelling == b.2.spelling);
    let mut changes = Vec::new();
    for (i, candidate) in candidates.iter().enumerate() {
        if candidates
            .iter()
            .enumerate()
            .any(|(j, other)| i != j && candidate.0 < other.1 && other.0 < candidate.1)
        {
            result.declined.push(GateDeclined {
                heard: text[candidate.0..candidate.1].to_owned(),
                spelling: candidate.2.spelling.clone(),
                reason: GateDecline::Ambiguous,
            });
        } else {
            changes.push(*candidate);
        }
    }
    for (start, end, item) in changes.iter().rev() {
        result.text.replace_range(*start..*end, &item.spelling);
    }
    result.applied = changes
        .into_iter()
        .map(|(start, end, item)| GateChange {
            start,
            end,
            heard: text[start..end].to_owned(),
            spelling: item.spelling.clone(),
            term_id: item.term_id.clone(),
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
        for word in ["point", "points", "making", "write", "notes", "Monday's"] {
            assert!(ordinary_word(word), "{word}");
        }
        for word in ["onnx", "kubectl", "sortformer", "etcd", "ctl", ""] {
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
}
