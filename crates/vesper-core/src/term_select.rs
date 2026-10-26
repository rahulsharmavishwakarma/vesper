//! Per-recording working-set selection over the term inventory and the profile
//! vocabulary. Pure and deterministic: the same inputs give the same hints and
//! `hint_hash`, so the pre-ASR and post-ASR passes of one recording agree.
use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::memory::tokens;
use crate::{
    language_matches, project_matches, AppCategory, Language, Normalization,
    PersonalizationProfile, TermKind, TermSource, TermTier, WorkingSet, WorkspaceContext,
    MAX_HINTS, MAX_HINT_BYTES, MAX_NORMALIZATIONS, MAX_REFERENCES,
};

/// Longest spoken form, in words, the post-ASR matcher looks for.
const MAX_SPOKEN_WORDS: usize = 6;
/// Recency half-life for user-authored evidence.
const HALF_LIFE_MS: f64 = 30.0 * 24.0 * 60.0 * 60.0 * 1000.0;

/// One usable spelling offered to the selector. Suspended and removed inventory
/// rows are never passed in.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InventoryTerm {
    pub id: String,
    pub surface: String,
    /// Lowercase spoken forms that differ from the surface (split identifiers,
    /// taught aliases). The correction gate decides whether one may apply.
    pub spoken: Vec<String>,
    pub kind: TermKind,
    /// Best tier among the term's sources.
    pub tier: TermTier,
    pub sources: Vec<TermSource>,
    /// Workspace keys or project names; `""` marks global evidence.
    pub scopes: Vec<String>,
    pub language: Language,
    /// Independent user-authored evidence (files, entries or edits).
    pub evidence: u32,
    pub last_seen_ms: i64,
}

#[derive(Debug, Clone, Default)]
pub struct SelectionContext {
    pub workspace: WorkspaceContext,
    /// Resolved manual/rule project name; matched against scopes like a key.
    pub project: String,
    pub language: Language,
    pub now_ms: i64,
}

/// Split an identifier into lowercase spoken words: `fooBarBaz` → `foo bar baz`,
/// `vesper_core::terms` → `vesper core terms`, `HTTPServer` → `http server`.
/// Digits stay attached (`k8s`, `utf8`). Returns `None` when the split form is
/// the same as the lowercase surface or the term is not identifier-like.
pub fn identifier_spoken_form(surface: &str) -> Option<String> {
    let chars = surface.chars().collect::<Vec<_>>();
    let mut words: Vec<String> = Vec::new();
    let mut current = String::new();
    for (i, &c) in chars.iter().enumerate() {
        if !c.is_alphanumeric() {
            if !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
            continue;
        }
        let previous = i.checked_sub(1).map(|p| chars[p]);
        let next = chars.get(i + 1).copied();
        let boundary = c.is_uppercase()
            && previous.is_some_and(|p| {
                p.is_lowercase()
                    || p.is_ascii_digit()
                    || (p.is_uppercase() && next.is_some_and(char::is_lowercase))
            });
        if boundary && !current.is_empty() {
            words.push(std::mem::take(&mut current));
        }
        current.extend(c.to_lowercase());
    }
    if !current.is_empty() {
        words.push(current);
    }
    if words.len() < 2 || words.len() > MAX_SPOKEN_WORDS {
        return None;
    }
    let spoken = words.join(" ");
    (spoken != surface.to_lowercase()).then_some(spoken)
}

/// Stable provenance hash of an ordered hint list; empty when nothing is hinted.
pub fn hint_hash(hints: &[String]) -> String {
    if hints.is_empty() {
        return String::new();
    }
    // FNV-1a 64: dependency-free and stable across builds; provenance, not security.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for hint in hints {
        for byte in hint.bytes().chain([0x1f]) {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
    }
    format!("fnv1a64:{hash:016x}")
}

/// Manual words and confirmed corrections from the profile, as selector input.
/// Unticked automatic entries keep their existing raw-text gate in
/// `post_asr_profile`; here they only contribute hints and references.
pub fn profile_terms(profile: &PersonalizationProfile) -> Vec<InventoryTerm> {
    profile
        .vocabulary
        .iter()
        .filter(|e| e.enabled && !e.phrase.trim().is_empty())
        .map(|e| {
            let trusted = e.automatic_source.is_none() || e.confirmed;
            let mut spoken = Vec::new();
            if trusted {
                for alias in &e.aliases {
                    let alias = alias.trim().to_lowercase();
                    if !alias.is_empty() && alias != e.phrase.to_lowercase() {
                        spoken.push(alias);
                    }
                }
            }
            spoken.extend(identifier_spoken_form(&e.phrase));
            spoken.sort();
            spoken.dedup();
            let source = if e.aliases.is_empty() {
                TermSource::Vocabulary
            } else {
                TermSource::Correction
            };
            InventoryTerm {
                id: format!("profile:{}", e.id),
                surface: e.phrase.trim().to_owned(),
                spoken: if trusted || e.aliases.is_empty() {
                    spoken
                } else {
                    vec![]
                },
                kind: term_kind(&e.phrase),
                tier: TermTier::Asserted,
                sources: vec![source],
                scopes: vec![e.project.clone()],
                language: e.language,
                // Never `usage_count`: it counts raw ASR, including biased decodes.
                evidence: e.sources.len() as u32 + 1,
                last_seen_ms: 0,
            }
        })
        .collect()
}

/// Shape class of a surface spelling.
pub fn term_kind(surface: &str) -> TermKind {
    let letters = surface.chars().filter(|c| c.is_alphabetic()).count();
    if surface.split_whitespace().count() > 1 {
        TermKind::Phrase
    } else if letters >= 2
        && surface
            .chars()
            .filter(|c| c.is_alphabetic())
            .all(char::is_uppercase)
        && surface.len() <= 8
    {
        TermKind::Acronym
    } else if identifier_spoken_form(surface).is_some()
        || surface.chars().any(|c| c.is_ascii_digit())
    {
        TermKind::Identifier
    } else {
        TermKind::Name
    }
}

fn category_affinity(sources: &[TermSource], category: AppCategory) -> bool {
    sources.iter().any(|source| match source {
        TermSource::Identifier | TermSource::Manifest | TermSource::ProjectDictionary => {
            matches!(category, AppCategory::Code | AppCategory::Terminal)
        }
        TermSource::Documentation => matches!(
            category,
            AppCategory::Code | AppCategory::Terminal | AppCategory::Document | AppCategory::Notes
        ),
        TermSource::Contact => matches!(category, AppCategory::Email | AppCategory::Chat),
        _ => false,
    })
}

/// 0 = scoped to this workspace/project, 1 = global with category affinity,
/// 2 = global. `None` = scoped elsewhere (never offered: a right term in the wrong
/// scope is a hard negative).
fn scope_class(term: &InventoryTerm, context: &SelectionContext) -> Option<u8> {
    let matches = |scope: &String| {
        !scope.is_empty()
            && (scope == &context.workspace.workspace_key
                || project_matches(scope, &context.project) && !context.project.is_empty())
    };
    if term.scopes.iter().any(matches) {
        Some(0)
    } else if term.scopes.is_empty() || term.scopes.iter().any(String::is_empty) {
        Some(
            if category_affinity(&term.sources, context.workspace.category) {
                1
            } else {
                2
            },
        )
    } else {
        None
    }
}

fn strength(term: &InventoryTerm, now_ms: i64) -> f64 {
    let frequency = (1.0 + f64::from(term.evidence)).ln();
    if term.last_seen_ms <= 0 {
        return frequency;
    }
    let age = (now_ms - term.last_seen_ms).max(0) as f64;
    frequency * 0.5f64.powf(age / HALF_LIFE_MS)
}

/// Rank: workspace match → category → global; tier A before B; then recency- and
/// frequency-weighted user evidence; ties by spelling. Duplicate spellings collapse
/// to their best-ranked entry.
pub fn rank_terms<'a>(
    terms: &'a [InventoryTerm],
    context: &SelectionContext,
) -> Vec<&'a InventoryTerm> {
    let mut ranked = terms
        .iter()
        .filter(|t| !t.surface.trim().is_empty())
        .filter(|t| language_matches(t.language, context.language))
        .filter_map(|t| {
            scope_class(t, context).map(|class| (class, t.tier, strength(t, context.now_ms), t))
        })
        .collect::<Vec<_>>();
    ranked.sort_by(|a, b| {
        (a.0, a.1)
            .cmp(&(b.0, b.1))
            .then(b.2.total_cmp(&a.2))
            .then_with(|| a.3.surface.cmp(&b.3.surface))
            .then_with(|| a.3.id.cmp(&b.3.id))
    });
    let mut seen = HashSet::new();
    ranked
        .into_iter()
        .filter(|entry| seen.insert(entry.3.surface.to_lowercase()))
        .map(|entry| entry.3)
        .collect()
}

/// Build the working set. Hints and references are ranked identically with or
/// without `raw`. References are single-token, non-dictionary spellings (names,
/// products, identifiers): the gate repairs a near-miss token against them, so an
/// ordinary word must never be a reference. With `raw` (post-ASR), normalizations
/// list the spoken forms that occur in the raw text.
pub fn select_working_set(
    terms: &[InventoryTerm],
    context: &SelectionContext,
    raw: Option<&str>,
) -> WorkingSet {
    let ranked = rank_terms(terms, context);
    let mut hints = Vec::new();
    let mut bytes = 0;
    for term in &ranked {
        if hints.len() == MAX_HINTS {
            break;
        }
        if bytes + term.surface.len() > MAX_HINT_BYTES {
            continue;
        }
        bytes += term.surface.len();
        hints.push(term.surface.clone());
    }
    let hint_hash = hint_hash(&hints);
    let references = ranked
        .iter()
        .filter(|t| reference_eligible(&t.surface))
        .take(MAX_REFERENCES)
        .map(|t| t.surface.clone())
        .collect();
    let Some(raw) = raw else {
        return WorkingSet {
            hints,
            references,
            normalizations: vec![],
            hint_hash,
        };
    };
    let grams = raw_ngrams(raw);
    let mut normalizations = Vec::new();
    for term in &ranked {
        for spoken in &term.spoken {
            if grams.contains(spoken.as_str()) && normalizations.len() < MAX_NORMALIZATIONS {
                normalizations.push(Normalization {
                    spoken: spoken.clone(),
                    spelling: term.surface.clone(),
                    term_id: term.id.clone(),
                    tier: term.tier,
                });
            }
        }
    }
    WorkingSet {
        hints,
        references,
        normalizations,
        hint_hash,
    }
}

/// One token, at least four letters, and not an ordinary English word in any
/// letter case. Three-letter acronyms are excluded because most of their
/// one-edit neighbours are other acronyms (`NPU`/`TPU` → `GPU`).
fn reference_eligible(surface: &str) -> bool {
    !surface.contains(char::is_whitespace)
        && surface.chars().filter(|c| c.is_alphabetic()).count() >= 4
        && !crate::ordinary_word(surface)
}

/// Lowercase word n-grams (n ≤ 6) of the raw text, tokenized like the gate.
fn raw_ngrams(raw: &str) -> HashSet<String> {
    let words = tokens(raw)
        .iter()
        .map(|t| t.text.to_lowercase())
        .collect::<Vec<_>>();
    let mut grams = HashSet::with_capacity(words.len() * MAX_SPOKEN_WORDS);
    for start in 0..words.len() {
        let mut gram = String::new();
        for word in words.iter().skip(start).take(MAX_SPOKEN_WORDS) {
            if !gram.is_empty() {
                gram.push(' ');
            }
            gram.push_str(word);
            grams.insert(gram.clone());
        }
    }
    grams
}

/// Counts per scope class for diagnostics; never includes spellings.
pub fn scope_counts(terms: &[InventoryTerm], context: &SelectionContext) -> HashMap<u8, usize> {
    let mut counts = HashMap::new();
    for term in terms {
        if let Some(class) = scope_class(term, context) {
            *counts.entry(class).or_default() += 1;
        }
    }
    counts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn term(
        id: &str,
        surface: &str,
        tier: TermTier,
        source: TermSource,
        scope: &str,
    ) -> InventoryTerm {
        InventoryTerm {
            id: id.into(),
            surface: surface.into(),
            spoken: identifier_spoken_form(surface).into_iter().collect(),
            kind: term_kind(surface),
            tier,
            sources: vec![source],
            scopes: vec![scope.into()],
            language: Language::Auto,
            evidence: 2,
            last_seen_ms: 0,
        }
    }

    fn context(key: &str, category: AppCategory) -> SelectionContext {
        SelectionContext {
            workspace: WorkspaceContext {
                application_id: "test.editor".into(),
                category,
                workspace_key: key.into(),
                workspace_label: "demo".into(),
            },
            project: String::new(),
            language: Language::English,
            now_ms: 1_000_000,
        }
    }

    #[test]
    fn identifiers_split_into_spoken_words() {
        let cases = [
            ("fooBarBaz", Some("foo bar baz")),
            ("vesper_core", Some("vesper core")),
            ("vesper-core", Some("vesper core")),
            ("vesper_data::learning", Some("vesper data learning")),
            ("HTTPServer", Some("http server")),
            ("parseJSON", Some("parse json")),
            ("WorkingSet", Some("working set")),
            ("MAX_HINTS", Some("max hints")),
            ("Cargo.toml", Some("cargo toml")),
            ("k8s", None),
            ("kubectl", None),
            ("Vesper", None),
            ("utf8", None),
        ];
        for (surface, spoken) in cases {
            assert_eq!(
                identifier_spoken_form(surface).as_deref(),
                spoken,
                "{surface}"
            );
        }
    }

    #[test]
    fn short_acronyms_are_hints_but_never_near_miss_references() {
        let terms = vec![
            term(
                "g",
                "GPU",
                TermTier::Authored,
                TermSource::Documentation,
                "",
            ),
            term(
                "q",
                "Qwen",
                TermTier::Authored,
                TermSource::Documentation,
                "",
            ),
        ];
        let set = select_working_set(&terms, &context("", AppCategory::Code), None);
        assert_eq!(set.hints, ["GPU", "Qwen"]);
        assert_eq!(set.references, ["Qwen"]);
    }

    #[test]
    fn kinds_follow_shape() {
        assert_eq!(term_kind("ASR"), TermKind::Acronym);
        assert_eq!(term_kind("sherpa_onnx"), TermKind::Identifier);
        assert_eq!(term_kind("k8s"), TermKind::Identifier);
        assert_eq!(term_kind("Nemotron"), TermKind::Name);
        assert_eq!(term_kind("Cedar Point"), TermKind::Phrase);
    }

    #[test]
    fn workspace_terms_rank_before_category_and_global_and_other_scopes_are_excluded() {
        let terms = vec![
            term(
                "g",
                "Zorblax",
                TermTier::Asserted,
                TermSource::TextReplacement,
                "",
            ),
            term(
                "c",
                "cspellWord",
                TermTier::Asserted,
                TermSource::Identifier,
                "",
            ),
            term(
                "w",
                "quuxCore",
                TermTier::Authored,
                TermSource::Identifier,
                "ws-1",
            ),
            term(
                "x",
                "otherRepo",
                TermTier::Asserted,
                TermSource::Identifier,
                "ws-2",
            ),
        ];
        let set = select_working_set(&terms, &context("ws-1", AppCategory::Code), None);
        assert_eq!(set.hints, ["quuxCore", "cspellWord", "Zorblax"]);
        assert_eq!(set.references, ["quuxCore", "cspellWord", "Zorblax"]);
        assert!(set.normalizations.is_empty());
        assert!(set.hint_hash.starts_with("fnv1a64:"));
        let email = select_working_set(&terms, &context("", AppCategory::Email), None);
        assert_eq!(email.hints, ["Zorblax", "cspellWord"]);
    }

    #[test]
    fn asserted_before_authored_then_evidence() {
        let mut weak = term(
            "a",
            "alphaTerm",
            TermTier::Authored,
            TermSource::Identifier,
            "",
        );
        weak.evidence = 1;
        let mut strong = term(
            "b",
            "betaTerm",
            TermTier::Authored,
            TermSource::Identifier,
            "",
        );
        strong.evidence = 40;
        let asserted = term(
            "c",
            "Gamma",
            TermTier::Asserted,
            TermSource::SpellingDictionary,
            "",
        );
        let set = select_working_set(
            &[weak, strong, asserted],
            &context("", AppCategory::Other),
            None,
        );
        assert_eq!(set.hints, ["Gamma", "betaTerm", "alphaTerm"]);
    }

    #[test]
    fn caps_and_hash_are_bounded_and_stable() {
        let terms = (0..200)
            .map(|i| {
                term(
                    &i.to_string(),
                    &format!("termNumber{i:03}"),
                    TermTier::Authored,
                    TermSource::Identifier,
                    "",
                )
            })
            .collect::<Vec<_>>();
        let ctx = context("", AppCategory::Code);
        let pre = select_working_set(&terms, &ctx, None);
        assert_eq!(pre.hints.len(), MAX_HINTS);
        assert_eq!(pre.references.len(), MAX_REFERENCES);
        let post = select_working_set(&terms, &ctx, Some("please open term number 150 now"));
        assert_eq!(post.hints, pre.hints);
        assert_eq!(post.hint_hash, pre.hint_hash);
        assert_eq!(hint_hash(&[]), "");
        assert_ne!(
            hint_hash(&["a b".into()]),
            hint_hash(&["a".into(), "b".into()])
        );
    }

    #[test]
    fn post_asr_emits_normalizations_only_for_heard_spoken_forms() {
        let terms = vec![
            term(
                "1",
                "sherpa_onnx",
                TermTier::Authored,
                TermSource::Manifest,
                "",
            ),
            term(
                "2",
                "WorkingSet",
                TermTier::Authored,
                TermSource::Identifier,
                "",
            ),
            term(
                "3",
                "Nemotron",
                TermTier::Authored,
                TermSource::Documentation,
                "",
            ),
        ];
        let set = select_working_set(
            &terms,
            &context("", AppCategory::Code),
            Some("Load Sherpa ONNX, then call Nemotron."),
        );
        assert_eq!(set.normalizations.len(), 1);
        let n = &set.normalizations[0];
        assert_eq!(
            (n.spoken.as_str(), n.spelling.as_str()),
            ("sherpa onnx", "sherpa_onnx")
        );
        assert_eq!(n.tier, TermTier::Authored);
        assert_eq!(set.references, ["Nemotron", "WorkingSet", "sherpa_onnx"]);
    }

    #[test]
    fn profile_words_become_asserted_terms_and_untrusted_aliases_stay_out() {
        let mut profile = PersonalizationProfile::default();
        profile.vocabulary.push(crate::VocabularyEntry {
            id: "m".into(),
            phrase: "Kubernetes".into(),
            enabled: true,
            aliases: vec!["cooper nettys".into()],
            ..Default::default()
        });
        profile.vocabulary.push(crate::VocabularyEntry {
            id: "a".into(),
            phrase: "Mandy".into(),
            enabled: true,
            aliases: vec!["monday".into()],
            automatic_source: Some("cand".into()),
            ..Default::default()
        });
        let terms = profile_terms(&profile);
        assert_eq!(terms[0].spoken, ["cooper nettys"]);
        assert_eq!(terms[0].tier, TermTier::Asserted);
        assert!(terms[1].spoken.is_empty());
        let set = select_working_set(
            &terms,
            &context("", AppCategory::Other),
            Some("deploy cooper nettys on monday"),
        );
        assert_eq!(set.normalizations.len(), 1);
        assert_eq!(set.normalizations[0].spelling, "Kubernetes");
        // "Mandy" is an ordinary name in the lexicon: a hint, never a reference.
        assert_eq!(set.hints, ["Kubernetes", "Mandy"]);
        assert_eq!(set.references, ["Kubernetes"]);
    }

    #[test]
    fn project_scoped_profile_words_follow_the_resolved_project() {
        let scoped = term(
            "p",
            "Ledgerly",
            TermTier::Asserted,
            TermSource::Vocabulary,
            "Acme",
        );
        let mut ctx = context("", AppCategory::Other);
        assert!(
            select_working_set(std::slice::from_ref(&scoped), &ctx, None)
                .hints
                .is_empty()
        );
        ctx.project = "Acme".into();
        assert_eq!(
            select_working_set(&[scoped], &ctx, None).hints,
            ["Ledgerly"]
        );
    }
}
