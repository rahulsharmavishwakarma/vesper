//! Typed recognizer vocabulary hooks. A runtime field or ABI entry is not a
//! capability: each hook is `declared` until the fixed term evaluator qualifies it
//! for that adapter, and only `qualified` hooks receive automatic working-set hints.
use crate::{Language, PersonalizationProfile, WorkingSet};
use serde::Serialize;

/// How a recognizer can take per-recording vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SpeechHookKind {
    None,
    /// Decode-time phrase boosting (for example an RNN-T context graph).
    PhraseBias,
    /// Text placed in the model's prompt (for example a speech-LLM system turn).
    PromptContext,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HookStatus {
    /// Code path exists; not yet shown to help without harm on the term evaluator.
    Declared,
    /// Passed the term evaluator for this adapter; receives automatic hints.
    Qualified,
    /// Measured as harmful or unusable; never receives automatic hints.
    Disabled,
}

/// What per-token confidence the pinned runtime actually reports to Vesper.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfidenceKind {
    None,
    TokenLogprob,
    CtcPosterior,
    /// Reported but constant (for example 1.0), so unusable.
    Constant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechHook {
    pub kind: SpeechHookKind,
    pub status: HookStatus,
    pub max_phrases: usize,
    pub max_bytes: usize,
    /// Uniform decode-time boost in thousandths (phrase-bias hooks; 0 otherwise).
    pub boost_milli: u32,
}

impl SpeechHook {
    pub const NONE: Self = Self {
        kind: SpeechHookKind::None,
        status: HookStatus::Disabled,
        max_phrases: 0,
        max_bytes: 0,
        boost_milli: 0,
    };

    /// Automatic working-set hints reach only qualified hooks. The development
    /// evaluator (`evaluate`) may measure any hook that exists, so a disabled
    /// hook can be re-measured; the application never sets it.
    pub fn accepts_working_set(&self, evaluate: bool) -> bool {
        self.kind != SpeechHookKind::None && (self.status == HookStatus::Qualified || evaluate)
    }

    /// The explicit Settings opt-in list (`speech_bias_enabled`) keeps its existing
    /// behavior on phrase-bias hooks. It is the user's own choice, so qualification
    /// status (which governs automatic hints) does not remove it.
    pub fn accepts_manual_phrases(&self) -> bool {
        self.kind == SpeechHookKind::PhraseBias
    }
}

/// Transient per-recording decode input. Never persisted or accepted from callers
/// outside the application process (`PersonalizationProfile` skips it in serde).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DecodeContext {
    pub working_set: WorkingSet,
    /// Development evaluator only: send hints to an unqualified (declared or
    /// disabled) hook so it can be measured. The application never sets this.
    pub evaluate_declared_hooks: bool,
    /// Development evaluator only, with `evaluate_declared_hooks`: replace the
    /// hook's (phrases, bytes) limits to measure list-size degradation. Native
    /// runtimes still enforce their own hard limits.
    pub evaluation_limits: Option<(usize, usize)>,
    /// Development evaluator only, with `evaluate_declared_hooks`: replace the
    /// hook's uniform boost (thousandths) to measure recall against false alarms.
    pub evaluation_boost_milli: Option<u32>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HintSource {
    #[default]
    None,
    WorkingSet,
    /// The user's explicit Settings vocabulary (`speech_bias_enabled`).
    Manual,
}

/// The exact list one decode receives, with its provenance hash.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecognizerHints {
    pub source: HintSource,
    pub phrases: Vec<String>,
    /// Empty when no phrase was sent.
    pub hint_hash: String,
    /// Uniform boost in thousandths for phrase-bias hooks; 0 otherwise.
    pub boost_milli: u32,
}

/// Choose this decode's hints. Working-set hints win once a working set exists and
/// the hook is qualified; otherwise the explicit opt-in list keeps working on
/// phrase-bias hooks. Every list is bounded by the hook's own limits.
///
/// Automatic hints never include a surface that already reads as ordinary
/// English (every word in the CMUdict-derived lexicon, such as `Calico` or
/// `Cedar Point`): such words are not hard for a recognizer, and biasing toward
/// them made recognizers capitalize ordinary speech as terms. The correction
/// gate's references and normalizations keep them. The explicit opt-in list is
/// the user's own choice and is not filtered.
pub fn recognizer_hints(
    hook: &SpeechHook,
    profile: &PersonalizationProfile,
    language: Language,
) -> RecognizerHints {
    let context = profile.decode_context.as_ref();
    if let Some(context) = context.filter(|c| !c.working_set.hints.is_empty()) {
        if hook.accepts_working_set(context.evaluate_declared_hooks) {
            let mut limited = *hook;
            if context.evaluate_declared_hooks {
                if let Some((max_phrases, max_bytes)) = context.evaluation_limits {
                    (limited.max_phrases, limited.max_bytes) = (max_phrases, max_bytes);
                }
                if let Some(boost) = context.evaluation_boost_milli {
                    limited.boost_milli = boost;
                }
            }
            let hard = context
                .working_set
                .hints
                .iter()
                .filter(|hint| !crate::ordinary_language(hint))
                .cloned()
                .collect::<Vec<_>>();
            return hints(HintSource::WorkingSet, &limited, &hard);
        }
    }
    if hook.accepts_manual_phrases() {
        return hints(HintSource::Manual, hook, &profile.speech_phrases(language));
    }
    RecognizerHints::default()
}

fn hints(source: HintSource, hook: &SpeechHook, phrases: &[String]) -> RecognizerHints {
    let phrases = bounded_hints(hook, phrases);
    if phrases.is_empty() {
        return RecognizerHints::default();
    }
    RecognizerHints {
        source,
        boost_milli: if hook.kind == SpeechHookKind::PhraseBias {
            hook.boost_milli
        } else {
            0
        },
        hint_hash: hint_hash(&phrases),
        phrases,
    }
}

/// Safe, deduplicated (case-insensitive) phrases within the hook's count and byte
/// limits, in input order. Prompt hooks also reject the list separator.
pub fn bounded_hints(hook: &SpeechHook, phrases: &[String]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut bytes = 0;
    let mut kept = Vec::new();
    for phrase in phrases {
        let phrase = phrase.trim();
        if kept.len() >= hook.max_phrases {
            break;
        }
        let unsafe_phrase = phrase.is_empty()
            || phrase.len() > 128
            || phrase.starts_with('$')
            || phrase.contains("<|")
            || phrase.chars().any(char::is_control)
            || (hook.kind == SpeechHookKind::PromptContext && phrase.contains(','));
        if unsafe_phrase || !seen.insert(phrase.to_lowercase()) {
            continue;
        }
        if bytes + phrase.len() > hook.max_bytes {
            continue;
        }
        bytes += phrase.len();
        kept.push(phrase.to_owned());
    }
    kept
}

/// Deterministic provenance hash of the phrases actually sent (FNV-1a 64; not a
/// security property). Identical lists always produce identical hashes.
pub fn hint_hash(phrases: &[String]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in phrases.join("\n").bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("fnv1a64:{hash:016x}")
}

/// True when `word` was one of this decode's hints (or a word inside one), so
/// its appearance in the output is not independent evidence.
pub fn hinted(word: &str, hinted_phrases: &[String]) -> bool {
    let word = word.trim().to_lowercase();
    !word.is_empty()
        && hinted_phrases.iter().any(|phrase| {
            let phrase = phrase.to_lowercase();
            phrase == word || crate::memory_phrase_occurs(&phrase, &word)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::VocabularyEntry;

    const QUALIFIED: SpeechHook = SpeechHook {
        kind: SpeechHookKind::PhraseBias,
        status: HookStatus::Qualified,
        max_phrases: 3,
        max_bytes: 24,
        boost_milli: 2000,
    };

    fn profile(hints: &[&str], manual: &[&str], bias: bool) -> PersonalizationProfile {
        PersonalizationProfile {
            speech_bias_enabled: bias,
            vocabulary: manual
                .iter()
                .enumerate()
                .map(|(i, phrase)| VocabularyEntry {
                    id: i.to_string(),
                    phrase: (*phrase).into(),
                    language: Language::English,
                    enabled: true,
                    ..VocabularyEntry::default()
                })
                .collect(),
            decode_context: (!hints.is_empty()).then(|| DecodeContext {
                working_set: WorkingSet {
                    hints: hints.iter().map(|h| (*h).into()).collect(),
                    ..WorkingSet::default()
                },
                evaluate_declared_hooks: false,
                evaluation_limits: None,
                evaluation_boost_milli: None,
            }),
            ..PersonalizationProfile::default()
        }
    }

    #[test]
    fn only_qualified_hooks_receive_working_set_hints() {
        let p = profile(&["kubectl", "etcd"], &[], false);
        let sent = recognizer_hints(&QUALIFIED, &p, Language::English);
        assert_eq!(sent.source, HintSource::WorkingSet);
        assert_eq!(sent.phrases, ["kubectl", "etcd"]);
        assert!(sent.hint_hash.starts_with("fnv1a64:"));
        for status in [HookStatus::Declared, HookStatus::Disabled] {
            let hook = SpeechHook {
                status,
                ..QUALIFIED
            };
            assert!(recognizer_hints(&hook, &p, Language::English)
                .phrases
                .is_empty());
        }
        let none = recognizer_hints(&SpeechHook::NONE, &p, Language::English);
        assert_eq!(none, RecognizerHints::default());
    }

    #[test]
    fn evaluator_can_measure_unqualified_hooks_but_never_hookless_adapters() {
        let mut p = profile(&["kubectl"], &[], false);
        p.decode_context.as_mut().unwrap().evaluate_declared_hooks = true;
        for status in [HookStatus::Declared, HookStatus::Disabled] {
            let hook = SpeechHook {
                status,
                ..QUALIFIED
            };
            assert_eq!(
                recognizer_hints(&hook, &p, Language::English).phrases,
                ["kubectl"]
            );
        }
        assert!(recognizer_hints(&SpeechHook::NONE, &p, Language::English)
            .phrases
            .is_empty());
    }

    #[test]
    fn manual_opt_in_list_keeps_working_without_a_working_set() {
        let declared = SpeechHook {
            status: HookStatus::Declared,
            ..QUALIFIED
        };
        let p = profile(&[], &["Vesper"], true);
        let sent = recognizer_hints(&declared, &p, Language::English);
        assert_eq!(
            (sent.source, sent.phrases),
            (HintSource::Manual, vec!["Vesper".to_owned()])
        );
        assert!(recognizer_hints(
            &declared,
            &profile(&[], &["Vesper"], false),
            Language::English
        )
        .phrases
        .is_empty());
        // Disabling automatic hints leaves the explicit opt-in list in place.
        let disabled = SpeechHook {
            status: HookStatus::Disabled,
            ..QUALIFIED
        };
        let both = profile(&["kubectl"], &["Vesper"], true);
        assert_eq!(
            recognizer_hints(&disabled, &both, Language::English).phrases,
            ["Vesper"]
        );
        // A working set replaces the usage-ranked manual list on a qualified hook.
        let both = profile(&["kubectl"], &["Vesper"], true);
        assert_eq!(
            recognizer_hints(&QUALIFIED, &both, Language::English).phrases,
            ["kubectl"]
        );
    }

    #[test]
    fn automatic_hints_drop_ordinary_language_surfaces_but_manual_lists_do_not() {
        let p = profile(
            &[
                "Calico",
                "kubectl",
                "Cedar Point",
                "Saoirse Brennan",
                "Envoy",
            ],
            &["Calico"],
            true,
        );
        let wide = SpeechHook {
            max_phrases: 32,
            max_bytes: 2048,
            ..QUALIFIED
        };
        let sent = recognizer_hints(&wide, &p, Language::English);
        assert_eq!(sent.phrases, ["kubectl", "Saoirse Brennan"]);
        assert_eq!(sent.boost_milli, 2000);
        let disabled = SpeechHook {
            status: HookStatus::Disabled,
            ..wide
        };
        let manual = recognizer_hints(&disabled, &p, Language::English);
        assert_eq!(
            (manual.source, manual.phrases),
            (HintSource::Manual, vec!["Calico".to_owned()])
        );
    }

    #[test]
    fn evaluator_can_override_the_uniform_boost() {
        let mut p = profile(&["kubectl"], &[], false);
        let context = p.decode_context.as_mut().unwrap();
        context.evaluate_declared_hooks = true;
        context.evaluation_boost_milli = Some(500);
        assert_eq!(
            recognizer_hints(&QUALIFIED, &p, Language::English).boost_milli,
            500
        );
        let prompt = SpeechHook {
            kind: SpeechHookKind::PromptContext,
            ..QUALIFIED
        };
        assert_eq!(
            recognizer_hints(&prompt, &p, Language::English).boost_milli,
            0
        );
    }

    #[test]
    fn hints_are_bounded_deduplicated_and_prompt_safe() {
        let prompt = SpeechHook {
            kind: SpeechHookKind::PromptContext,
            ..QUALIFIED
        };
        let input = ["Helm", "helm", "a,b", "<|x", "kube-proxy", "Istio", "etcd"].map(String::from);
        assert_eq!(
            bounded_hints(&prompt, &input),
            ["Helm", "kube-proxy", "Istio"]
        );
        let tight = SpeechHook {
            max_bytes: 10,
            ..QUALIFIED
        };
        assert_eq!(bounded_hints(&tight, &input), ["Helm", "a,b"]);
        assert_eq!(hint_hash(&input[..2]), hint_hash(&input[..2]));
        assert_ne!(hint_hash(&input[..2]), hint_hash(&input[..3]));
    }

    #[test]
    fn hinted_words_cover_whole_phrases_and_their_words() {
        let hints = ["Cedar Point".to_owned(), "kubectl".to_owned()];
        assert!(hinted("cedar point", &hints));
        assert!(hinted("Cedar", &hints));
        assert!(hinted("KUBECTL", &hints));
        assert!(!hinted("point of view", &hints));
        assert!(!hinted("Helm", &hints));
    }
}
