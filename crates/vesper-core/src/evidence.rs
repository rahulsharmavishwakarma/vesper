//! Asymmetric promotion rule for learned spellings. A rule switches on only when the
//! posterior that the spelling is intended exceeds `t = r / (1 + r)`, where `r` is
//! the cost of a wrong automatic change divided by the cost of a missed one. Only
//! user-authored edits are evidence; recurrence, unedited or generated text is not.
use serde::{Deserialize, Serialize};

use crate::{ordinary_language, technical_spelling, CorrectionPair};

/// What the recognizer heard, which sets how costly a wrong replacement would be.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HeardClass {
    /// Contains a token that is not an ordinary English word (`kube ctl`, `read me`
    /// is ordinary). Replacing a non-word rarely destroys meaning.
    NonWord,
    /// Several ordinary words (`cube control`). Applies only with teaching cues.
    OrdinaryPhrase,
    /// One capitalized dictionary word replaced by another capitalized spelling:
    /// a name-spelling repair (`Jonathon` → `Jonathan`). Treated like a phrase.
    NameSpelling,
    /// One ordinary word (`write`, `Monday`). Never activated automatically.
    OrdinaryWord,
}

pub fn heard_class(pair: &CorrectionPair) -> HeardClass {
    let capitalized = |s: &str| s.chars().next().is_some_and(char::is_uppercase);
    if !ordinary_language(&pair.heard) {
        HeardClass::NonWord
    } else if pair.heard.split_whitespace().count() > 1 {
        HeardClass::OrdinaryPhrase
    } else if capitalized(&pair.heard) && capitalized(&pair.intended) {
        HeardClass::NameSpelling
    } else {
        HeardClass::OrdinaryWord
    }
}

/// Beta prior and cost ratio for one heard class; design assumptions, not calibrated.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PromotionPolicy {
    pub alpha: f64,
    pub beta: f64,
    /// `None` means never automatic: the user must confirm the word.
    pub cost_ratio: Option<f64>,
}

/// Non-word heard forms use r = 4 (t = 0.80). A user-typed identifier replacing a
/// non-word, or rejoining the same letters, inside a known project starts from an
/// optimistic Beta(2, 0.5) prior, so one edit suffices; other spellings use
/// Jeffreys Beta(0.5, 0.5), so two do.
/// Ordinary phrases and capitalized name repairs also need two independent edits
/// and matching teaching cues. A single lowercase ordinary word (`write` →
/// `Wright`) is never promoted automatically (r = ∞).
pub fn promotion_policy(pair: &CorrectionPair, known_project: bool) -> PromotionPolicy {
    let letters = |s: &str| {
        s.chars()
            .filter(|c| c.is_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect::<String>()
    };
    // Joining the same letters into an identifier (`aster tool` → `AsterTool`)
    // changes only spacing and case, so it is as low-risk as repairing a non-word.
    let rejoin = letters(&pair.heard) == letters(&pair.intended);
    match heard_class(pair) {
        HeardClass::NonWord | HeardClass::OrdinaryPhrase
            if known_project
                && technical_spelling(&pair.intended)
                && (rejoin || heard_class(pair) == HeardClass::NonWord) =>
        {
            PromotionPolicy {
                alpha: 2.0,
                beta: 0.5,
                cost_ratio: Some(4.0),
            }
        }
        HeardClass::NonWord | HeardClass::OrdinaryPhrase | HeardClass::NameSpelling => {
            PromotionPolicy {
                alpha: 0.5,
                beta: 0.5,
                cost_ratio: Some(4.0),
            }
        }
        HeardClass::OrdinaryWord => PromotionPolicy {
            alpha: 0.5,
            beta: 0.5,
            cost_ratio: None,
        },
    }
}

impl PromotionPolicy {
    pub fn threshold(&self) -> Option<f64> {
        self.cost_ratio.map(|r| r / (1.0 + r))
    }

    pub fn posterior_mean(&self, positives: usize, negatives: usize) -> f64 {
        (self.alpha + positives as f64) / (self.alpha + self.beta + (positives + negatives) as f64)
    }

    /// Any user revert suspends a rule outright (one strike), so promotion only
    /// considers spellings without negatives.
    pub fn promotes(&self, positives: usize, negatives: usize) -> bool {
        negatives == 0
            && self
                .threshold()
                .is_some_and(|t| self.posterior_mean(positives, 0) > t)
    }

    /// Smallest number of independent positive edits that clears the threshold.
    pub fn required_support(&self) -> Option<usize> {
        self.threshold()?;
        (1..=64).find(|s| self.promotes(*s, 0))
    }
}

/// Error patterns differ between recognizers, so learned heard→meant aliases are
/// tied to the family that produced them. Vocabulary itself stays model-independent.
pub fn recognizer_family(model: &str) -> String {
    let lower = model.to_lowercase();
    let family = [
        ("nemotron", "nemotron"),
        ("phonon", "phonon"),
        ("redux", "parakeet-redux"),
        ("parakeet", "parakeet"),
        ("cohere", "cohere"),
        ("qwen", "qwen3-asr"),
        ("whisper", "whisper"),
        ("moonshine", "moonshine"),
        ("granite", "granite"),
        ("lfm", "lfm-audio"),
    ]
    .into_iter()
    .find(|(needle, _)| lower.contains(needle))
    .map(|(_, family)| family);
    family.map_or(lower.trim().to_owned(), str::to_owned)
}

/// The policy breaker trips when at least this many distinct automatic rules are
/// reverted by the user within the window; all unconfirmed automatic rules then
/// return to shadow until the user resumes them or a new policy version ships.
pub const BREAKER_DISTINCT_REVERTS: usize = 2;
pub const BREAKER_WINDOW_MS: i64 = 7 * 24 * 60 * 60 * 1000;

pub fn breaker_trips<'a>(reverts: impl IntoIterator<Item = (&'a str, i64)>, now_ms: i64) -> bool {
    let mut distinct = std::collections::BTreeSet::new();
    for (rule, at) in reverts {
        if now_ms.saturating_sub(at) <= BREAKER_WINDOW_MS {
            distinct.insert(rule);
        }
    }
    distinct.len() >= BREAKER_DISTINCT_REVERTS
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair(heard: &str, intended: &str) -> CorrectionPair {
        CorrectionPair {
            heard: heard.into(),
            intended: intended.into(),
        }
    }

    #[test]
    fn heard_classes_follow_the_lexicon() {
        assert_eq!(
            heard_class(&pair("kube ctl", "kubectl")),
            HeardClass::NonWord
        );
        assert_eq!(
            heard_class(&pair("cube control", "kubectl")),
            HeardClass::OrdinaryPhrase
        );
        assert_eq!(
            heard_class(&pair("write", "Wright")),
            HeardClass::OrdinaryWord
        );
        assert_eq!(
            heard_class(&pair("notes", "Nots")),
            HeardClass::OrdinaryWord
        );
        assert_eq!(
            heard_class(&pair("Jonathon", "Jonathan")),
            HeardClass::NameSpelling
        );
    }

    #[test]
    fn required_support_matches_the_documented_table() {
        let identifier = promotion_policy(&pair("sherpa onnx", "sherpa_onnx"), true);
        assert_eq!(identifier.required_support(), Some(1));
        assert_eq!(
            promotion_policy(&pair("sherpa onnx", "sherpa_onnx"), false).required_support(),
            Some(2)
        );
        assert_eq!(
            promotion_policy(&pair("cube control", "kubectl"), true).required_support(),
            Some(2)
        );
        assert_eq!(
            promotion_policy(&pair("aster tool", "AsterTool"), true).required_support(),
            Some(1)
        );
        assert_eq!(
            promotion_policy(&pair("read me", "README.md"), true).required_support(),
            Some(2)
        );
        for (heard, intended) in [("write", "Wright"), ("monday", "Mandy"), ("notes", "Nots")] {
            assert_eq!(
                promotion_policy(&pair(heard, intended), true).required_support(),
                None
            );
        }
        assert!(!identifier.promotes(10, 1));
        let strict = PromotionPolicy {
            cost_ratio: Some(9.0),
            ..promotion_policy(&pair("cube control", "kubectl"), true)
        };
        assert_eq!(strict.required_support(), Some(5));
    }

    #[test]
    fn families_group_runtimes_but_not_fine_tunes() {
        assert_eq!(recognizer_family("NVIDIA Parakeet TDT 0.6B v3"), "parakeet");
        assert_eq!(recognizer_family("Phonon-2 (MLX)"), "phonon");
        assert_eq!(
            recognizer_family("Moondream Parakeet Redux"),
            "parakeet-redux"
        );
        assert_eq!(recognizer_family("Cohere Transcribe"), "cohere");
        assert_eq!(recognizer_family("Custom Model"), "custom model");
    }

    #[test]
    fn breaker_counts_distinct_recent_rules() {
        let day = 24 * 60 * 60 * 1000;
        let now = 30 * day;
        assert!(!breaker_trips([("a", now), ("a", now)], now));
        assert!(breaker_trips([("a", now), ("b", now - 6 * day)], now));
        assert!(!breaker_trips([("a", now), ("b", now - 8 * day)], now));
    }
}
