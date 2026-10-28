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
    // Names and acronyms (`Vesper`, `NVIDIA`) are what a near-miss repair
    // targets, so they fill references first; identifiers follow. Each group
    // keeps the hint order.
    let mut names = Vec::new();
    let mut others = Vec::new();
    for term in &ranked {
        if names.len() == MAX_REFERENCES {
            break;
        }
        let name = matches!(term.kind, TermKind::Name | TermKind::Acronym)
            && term.surface.chars().any(char::is_uppercase);
        if (name || others.len() < MAX_REFERENCES) && reference_eligible(term) {
            if name {
                names.push(term.surface.clone());
            } else {
                others.push(term.surface.clone());
            }
        }
    }
    let references: Vec<String> = names
        .into_iter()
        .chain(others)
        .take(MAX_REFERENCES)
        .collect();
    let Some(raw) = raw else {
        return WorkingSet {
            hints,
            references,
            normalizations: vec![],
            hint_hash,
        };
    };
    // Post-ASR: spellings a heard non-word could be a near miss of come first,
    // whatever their rank, so the gate sees every close candidate (and abstains
    // when two are equally close).
    let mut near = near_miss_candidates(raw, &ranked);
    for surface in references {
        if near.len() == MAX_REFERENCES {
            break;
        }
        if !near.contains(&surface) {
            near.push(surface);
        }
    }
    let references = near;
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

/// One token of at least four letters that is either not an ordinary English
/// word, or a capitalized name or acronym (`Vesper`, `NVIDIA`) whose lowercase
/// form happens to be in the lexicon. Near-miss safety rests on the *heard*
/// token being a non-word; a lowercase ordinary word is never a reference.
/// Three-letter acronyms are excluded because most of their one-edit neighbours
/// are other acronyms (`NPU`/`TPU` → `GPU`).
fn reference_eligible(term: &InventoryTerm) -> bool {
    let surface = &term.surface;
    !surface.contains(char::is_whitespace)
        && surface.chars().filter(|c| c.is_alphabetic()).count() >= 4
        && (!crate::ordinary_word(surface)
            || (surface.chars().any(char::is_uppercase)
                && matches!(term.kind, TermKind::Name | TermKind::Acronym)))
}

fn letters(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Levenshtein distance, stopping early once it must exceed `limit`.
fn within_edits(a: &str, b: &str, limit: usize) -> bool {
    let (a, b) = (a.chars().collect::<Vec<_>>(), b.chars().collect::<Vec<_>>());
    if a.len().abs_diff(b.len()) > limit {
        return false;
    }
    let mut row = (0..=b.len()).collect::<Vec<_>>();
    for (i, x) in a.iter().enumerate() {
        let mut prior = row[0];
        row[0] = i + 1;
        for (j, y) in b.iter().enumerate() {
            let old = row[j + 1];
            row[j + 1] = (row[j] + 1).min(old + 1).min(prior + usize::from(x != y));
            prior = old;
        }
        if row.iter().min().is_some_and(|m| *m > limit) {
            return false;
        }
    }
    row[b.len()] <= limit
}

/// Reference-eligible spellings within the correction gate's near-miss bound
/// (one edit, two from eight letters) of a heard token that is not an ordinary
/// word and has at least four letters. Rank order, capped at `MAX_REFERENCES`.
fn near_miss_candidates(raw: &str, ranked: &[&InventoryTerm]) -> Vec<String> {
    let heard = tokens(raw)
        .iter()
        .map(|t| letters(t.text))
        .filter(|w| w.chars().count() >= 4 && !crate::ordinary_word(w))
        .collect::<HashSet<_>>();
    if heard.is_empty() {
        return vec![];
    }
    let shortest = heard.iter().map(|h| h.chars().count()).min().unwrap_or(0);
    let longest = heard.iter().map(|h| h.chars().count()).max().unwrap_or(0);
    let mut found = Vec::new();
    for term in ranked {
        // Cheap length window first; letters are never longer than the surface.
        let length = term.surface.chars().count();
        if length + 2 < shortest || term.surface.contains(char::is_whitespace) {
            continue;
        }
        let target = letters(&term.surface);
        let size = target.chars().count();
        if size + 2 < shortest || size > longest + 2 {
            continue;
        }
        let close = heard.iter().any(|h| {
            let bound = if h.chars().count() >= 8 { 2 } else { 1 };
            *h != target && within_edits(h, &target, bound)
        });
        if close && reference_eligible(term) {
            found.push(term.surface.clone());
            if found.len() == MAX_REFERENCES {
                break;
            }
        }
    }
    found
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
    fn capitalized_names_are_references_but_lowercase_ordinary_words_are_not() {
        let terms = vec![
            term(
                "v",
                "Vesper",
                TermTier::Authored,
                TermSource::Documentation,
                "",
            ),
            term(
                "n",
                "NVIDIA",
                TermTier::Authored,
                TermSource::Documentation,
                "",
            ),
            term(
                "p",
                "point",
                TermTier::Asserted,
                TermSource::SpellingDictionary,
                "",
            ),
            term(
                "w",
                "relative_path",
                TermTier::Authored,
                TermSource::Identifier,
                "",
            ),
        ];
        let set = select_working_set(&terms, &context("", AppCategory::Code), None);
        assert_eq!(set.references, ["NVIDIA", "Vesper", "relative_path"]);
        // Names come before higher-ranked identifiers.
        let mut strong = term(
            "i",
            "sherpa_onnx",
            TermTier::Asserted,
            TermSource::ProjectDictionary,
            "",
        );
        strong.evidence = 500;
        let mut all = terms.clone();
        all.push(strong);
        let set = select_working_set(&all, &context("", AppCategory::Code), None);
        assert_eq!(set.hints[0], "sherpa_onnx");
        assert_eq!(
            set.references,
            ["NVIDIA", "Vesper", "sherpa_onnx", "relative_path"]
        );
    }

    #[test]
    fn post_asr_references_lead_with_near_misses_of_heard_non_words() {
        let mut terms = (0..40)
            .map(|i| {
                let mut t = term(
                    &i.to_string(),
                    &format!(
                        "Strongname{}{}",
                        char::from(b'a' + (i / 26) as u8),
                        char::from(b'a' + (i % 26) as u8)
                    ),
                    TermTier::Authored,
                    TermSource::Documentation,
                    "",
                );
                t.evidence = 100;
                t
            })
            .collect::<Vec<_>>();
        let mut weak = term(
            "v",
            "Vesper",
            TermTier::Authored,
            TermSource::Documentation,
            "",
        );
        weak.evidence = 1;
        terms.push(weak);
        let ctx = context("", AppCategory::Code);
        let pre = select_working_set(&terms, &ctx, None);
        assert!(!pre.references.contains(&"Vesper".to_owned()));
        let post = select_working_set(&terms, &ctx, Some("I opened vasper today"));
        assert_eq!(post.references[0], "Vesper");
        assert_eq!(post.references.len(), MAX_REFERENCES);
        assert_eq!(post.hints, pre.hints);
        // An ordinary heard word pulls in nothing.
        let ordinary = select_working_set(&terms, &ctx, Some("I opened paper today"));
        assert_eq!(ordinary.references, pre.references);
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
        // Capitalized names lead the references; identifiers follow in rank order.
        assert_eq!(set.references, ["Zorblax", "quuxCore", "cspellWord"]);
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
        // A capitalized name is a reference even when the lexicon lists it.
        assert_eq!(set.hints, ["Kubernetes", "Mandy"]);
        assert_eq!(set.references, ["Kubernetes", "Mandy"]);
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
