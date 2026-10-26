//! Fixed term evaluator: per-condition working sets, saved decode rows and
//! scoring. Scoring needs no model, so the correction-gate arm can be recomputed
//! from saved raw outputs whenever the gate changes.
use crate::metrics::normalize;
use crate::CorpusSource;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use vesper_core::{
    correction_gate_with, hint_hash, Language, NearMiss, Normalization, TermTier, WorkingSet,
};

pub const TERM_MANIFEST_SCHEMA_VERSION: u32 = 1;
const LARGE_LIST: usize = 150;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TermManifest {
    pub schema_version: u32,
    pub sources: Vec<CorpusSource>,
    pub workspaces: Vec<TermWorkspace>,
    #[serde(default)]
    pub distractors: Vec<TermEntry>,
    pub samples: Vec<TermSample>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TermWorkspace {
    pub id: String,
    pub label: String,
    pub terms: Vec<TermEntry>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TermEntry {
    pub id: String,
    pub spelling: String,
    pub spoken: Vec<String>,
    pub kind: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TermSample {
    pub id: String,
    pub source_id: String,
    pub split: String,
    pub language: String,
    #[serde(default)]
    pub audio_path: Option<String>,
    #[serde(default)]
    pub sha256: Option<String>,
    #[serde(default)]
    pub duration_seconds: Option<f64>,
    pub reference: String,
    #[serde(default)]
    pub workspace_id: Option<String>,
    #[serde(default)]
    pub target_terms: Vec<String>,
    pub category: String,
    pub subcategory: String,
    #[serde(default)]
    pub related_term_id: Option<String>,
    #[serde(default)]
    pub voice: Option<String>,
    #[serde(default)]
    pub synthetic: bool,
    #[serde(default = "clean")]
    pub noise: String,
    #[serde(default)]
    pub biasing_list: Option<Vec<String>>,
}

fn clean() -> String {
    "clean".into()
}

impl TermManifest {
    pub fn from_slice(bytes: &[u8]) -> Result<Self, String> {
        let manifest: Self =
            serde_json::from_slice(bytes).map_err(|e| format!("invalid term manifest: {e}"))?;
        if manifest.schema_version != TERM_MANIFEST_SCHEMA_VERSION {
            return Err(format!(
                "unsupported term manifest schema {}",
                manifest.schema_version
            ));
        }
        if manifest.workspaces.len() < 2 {
            return Err("term manifests need at least two workspaces".into());
        }
        let mut ids = std::collections::HashSet::new();
        for sample in &manifest.samples {
            if !ids.insert(&sample.id) {
                return Err(format!("duplicate term sample id: {}", sample.id));
            }
            if sample.audio_path.is_some() != sample.sha256.is_some() {
                return Err(format!("sample {} needs both audio and hash", sample.id));
            }
        }
        Ok(manifest)
    }

    fn workspace_index(&self, sample: &TermSample) -> usize {
        sample
            .workspace_id
            .as_ref()
            .and_then(|id| self.workspaces.iter().position(|w| &w.id == id))
            .unwrap_or_else(|| (stable_hash(&sample.id) % self.workspaces.len() as u64) as usize)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Condition {
    None,
    Ws10,
    Ws32,
    Wrong32,
    Large,
}

impl Condition {
    pub const ALL: [Self; 5] = [
        Self::None,
        Self::Ws10,
        Self::Ws32,
        Self::Wrong32,
        Self::Large,
    ];
    pub fn parse(value: &str) -> Result<Self, String> {
        serde_json::from_value(serde_json::Value::String(value.into()))
            .map_err(|_| format!("unknown condition: {value}"))
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Ws10 => "ws10",
            Self::Ws32 => "ws32",
            Self::Wrong32 => "wrong32",
            Self::Large => "large",
        }
    }
}

/// FNV-1a: deterministic ordering independent of Rust's randomized hasher.
fn stable_hash(text: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn word_entry(word: &str) -> TermEntry {
    TermEntry {
        id: format!("word-{}", word.to_lowercase()),
        spelling: word.to_owned(),
        spoken: vec![word.to_lowercase()],
        kind: "name".into(),
    }
}

/// Target terms first, then the related term, then a sample-seeded stable order.
fn ordered<'a>(sample: &TermSample, terms: &'a [TermEntry]) -> Vec<&'a TermEntry> {
    let mut items = terms.iter().collect::<Vec<_>>();
    items.sort_by_key(|term| {
        let target = sample
            .target_terms
            .iter()
            .any(|t| t.eq_ignore_ascii_case(&term.spelling));
        let related = sample.related_term_id.as_deref() == Some(term.id.as_str());
        (
            !target,
            !related,
            stable_hash(&format!("{}\u{0}{}", sample.id, term.id)),
        )
    });
    items
}

/// The term list one condition gives one sample, in selector order.
pub fn condition_terms(
    manifest: &TermManifest,
    sample: &TermSample,
    condition: Condition,
) -> Vec<TermEntry> {
    if let Some(list) = &sample.biasing_list {
        return biasing_condition_terms(manifest, sample, list, condition);
    }
    let index = manifest.workspace_index(sample);
    let own = ordered(sample, &manifest.workspaces[index].terms);
    let take = |n: usize| own.iter().take(n).map(|t| (*t).clone()).collect();
    match condition {
        Condition::None => vec![],
        Condition::Ws10 => take(10),
        Condition::Ws32 => take(32),
        Condition::Wrong32 => {
            let other = &manifest.workspaces[(index + 1) % manifest.workspaces.len()];
            ordered(sample, &other.terms)
                .into_iter()
                .filter(|t| {
                    !sample
                        .target_terms
                        .iter()
                        .any(|s| s.eq_ignore_ascii_case(&t.spelling))
                })
                .take(32)
                .cloned()
                .collect()
        }
        Condition::Large => {
            let mut list = own.into_iter().cloned().collect::<Vec<_>>();
            for offset in 1..manifest.workspaces.len() {
                let other = &manifest.workspaces[(index + offset) % manifest.workspaces.len()];
                list.extend(ordered(sample, &other.terms).into_iter().cloned());
            }
            list.extend(ordered(sample, &manifest.distractors).into_iter().cloned());
            list.truncate(LARGE_LIST);
            list
        }
    }
}

fn biasing_condition_terms(
    manifest: &TermManifest,
    sample: &TermSample,
    list: &[String],
    condition: Condition,
) -> Vec<TermEntry> {
    let entries = list.iter().map(|w| word_entry(w)).collect::<Vec<_>>();
    let own = ordered(sample, &entries);
    let take = |n: usize| own.iter().take(n).map(|t| (*t).clone()).collect();
    match condition {
        Condition::None => vec![],
        Condition::Ws10 => take(10),
        Condition::Ws32 => take(32),
        Condition::Large => take(list.len()),
        Condition::Wrong32 => {
            // Another utterance's list, in manifest order, without this sample's targets.
            let others = manifest
                .samples
                .iter()
                .filter(|s| s.biasing_list.is_some() && s.id != sample.id)
                .collect::<Vec<_>>();
            if others.is_empty() {
                return vec![];
            }
            let other = others[(stable_hash(&sample.id) % others.len() as u64) as usize];
            other
                .biasing_list
                .iter()
                .flatten()
                .filter(|w| {
                    !sample
                        .target_terms
                        .iter()
                        .any(|t| t.eq_ignore_ascii_case(w))
                        && !contains_term(&sample.reference, w, false)
                })
                .take(32)
                .map(|w| word_entry(w))
                .collect()
        }
    }
}

/// Hints, writer references and gate normalizations for a list of terms.
pub fn working_set(terms: &[TermEntry], tier: TermTier) -> WorkingSet {
    let hints = terms.iter().map(|t| t.spelling.clone()).collect::<Vec<_>>();
    WorkingSet {
        hint_hash: hint_hash(&hints),
        references: hints
            .iter()
            .take(vesper_core::MAX_REFERENCES)
            .cloned()
            .collect(),
        normalizations: terms
            .iter()
            .flat_map(|term| {
                term.spoken.iter().map(move |spoken| Normalization {
                    spoken: spoken.clone(),
                    spelling: term.spelling.clone(),
                    term_id: term.id.clone(),
                    tier,
                })
            })
            .collect(),
        hints,
    }
}

/// Word-bounded occurrences of `term` in `text` (alphanumerics delimit words).
pub fn count_term(text: &str, term: &str, case_sensitive: bool) -> usize {
    let (text, term) = if case_sensitive {
        (text.to_owned(), term.to_owned())
    } else {
        (text.to_lowercase(), term.to_lowercase())
    };
    if term.is_empty() {
        return 0;
    }
    let mut count = 0;
    let mut start = 0;
    while let Some(found) = text[start..].find(&term) {
        let begin = start + found;
        let end = begin + term.len();
        let before = text[..begin].chars().next_back();
        let after = text[end..].chars().next();
        if !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphanumeric) {
            count += 1;
        }
        start = begin + term.chars().next().map_or(1, char::len_utf8);
    }
    count
}

pub fn contains_term(text: &str, term: &str, case_sensitive: bool) -> bool {
    count_term(text, term, case_sensitive) > 0
}

/// Word errors split into target-term (B) and other (U) words, after Le et al.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SplitErrors {
    pub target_words: usize,
    pub target_errors: usize,
    pub other_words: usize,
    pub other_errors: usize,
}

pub fn split_errors(
    reference: &str,
    hypothesis: &str,
    targets: &[String],
    hint_terms: &[String],
) -> SplitErrors {
    let reference = normalize(reference);
    let hypothesis = normalize(hypothesis);
    let r = reference.split_whitespace().collect::<Vec<_>>();
    let h = hypothesis.split_whitespace().collect::<Vec<_>>();
    let mut is_target = vec![false; r.len()];
    for target in targets {
        let words = normalize(target);
        let words = words.split_whitespace().collect::<Vec<_>>();
        if words.is_empty() || words.len() > r.len() {
            continue;
        }
        for start in 0..=r.len() - words.len() {
            if r[start..start + words.len()] == words[..] {
                is_target[start..start + words.len()].fill(true);
            }
        }
    }
    let hint_words = hint_terms
        .iter()
        .chain(targets)
        .flat_map(|t| {
            normalize(t)
                .split_whitespace()
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .collect::<std::collections::HashSet<_>>();
    // Levenshtein with backtrace.
    let (n, m) = (r.len(), h.len());
    let mut d = vec![vec![0usize; m + 1]; n + 1];
    for (i, row) in d.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, cell) in d[0].iter_mut().enumerate() {
        *cell = j;
    }
    for i in 1..=n {
        for j in 1..=m {
            let cost = usize::from(r[i - 1] != h[j - 1]);
            d[i][j] = (d[i - 1][j - 1] + cost)
                .min(d[i - 1][j] + 1)
                .min(d[i][j - 1] + 1);
        }
    }
    let mut out = SplitErrors {
        target_words: is_target.iter().filter(|t| **t).count(),
        other_words: is_target.iter().filter(|t| !**t).count(),
        ..SplitErrors::default()
    };
    let (mut i, mut j) = (n, m);
    while i > 0 || j > 0 {
        if i > 0 && j > 0 && d[i][j] == d[i - 1][j - 1] + usize::from(r[i - 1] != h[j - 1]) {
            if r[i - 1] != h[j - 1] {
                if is_target[i - 1] {
                    out.target_errors += 1;
                } else {
                    out.other_errors += 1;
                }
            }
            i -= 1;
            j -= 1;
        } else if i > 0 && d[i][j] == d[i - 1][j] + 1 {
            if is_target[i - 1] {
                out.target_errors += 1;
            } else {
                out.other_errors += 1;
            }
            i -= 1;
        } else {
            if hint_words.contains(h[j - 1]) {
                out.target_errors += 1;
            } else {
                out.other_errors += 1;
            }
            j -= 1;
        }
    }
    out
}

/// One-sided 95% Clopper–Pearson upper bound for `k` events in `n` trials.
pub fn upper_bound_95(k: usize, n: usize) -> f64 {
    if n == 0 {
        return 1.0;
    }
    if k >= n {
        return 1.0;
    }
    let ln_fact = |x: usize| (1..=x).map(|v| (v as f64).ln()).sum::<f64>();
    let ln_n = ln_fact(n);
    let ln_choose = (0..=k)
        .map(|i| ln_n - ln_fact(i) - ln_fact(n - i))
        .collect::<Vec<_>>();
    let cdf = |p: f64| -> f64 {
        (0..=k)
            .map(|i| (ln_choose[i] + i as f64 * p.ln() + (n - i) as f64 * (1.0 - p).ln()).exp())
            .sum()
    };
    let (mut low, mut high) = (k as f64 / n as f64, 1.0);
    for _ in 0..100 {
        let middle = (low + high) / 2.0;
        if cdf(middle) > 0.05 {
            low = middle;
        } else {
            high = middle;
        }
    }
    high
}

fn percentile(values: &mut [f64], fraction: f64) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(f64::total_cmp);
    let rank = ((values.len() as f64 * fraction).ceil() as usize).clamp(1, values.len());
    Some(values[rank - 1])
}

/// One saved decode; the scorer never re-runs a model.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DecodeRow {
    pub model: String,
    pub hook_kind: String,
    pub hook_status: String,
    pub frontend: String,
    pub sample_id: String,
    pub condition: Condition,
    pub hints_sent: usize,
    pub hint_hash: String,
    pub hint_source: String,
    pub raw: Option<String>,
    #[serde(default)]
    pub cleaned: Option<String>,
    pub asr_ms: f64,
    #[serde(default)]
    pub cleanup_ms: Option<f64>,
    pub error: Option<String>,
    #[serde(default)]
    pub cleanup_error: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupScore {
    pub samples: usize,
    pub failures: usize,
    pub target_occurrences: usize,
    pub target_hits_case_sensitive: usize,
    pub target_hits_case_insensitive: usize,
    pub recall_case_sensitive: Option<f64>,
    pub recall_case_insensitive: Option<f64>,
    pub reference_words: usize,
    pub word_errors: usize,
    pub wer: Option<f64>,
    pub non_target_words: usize,
    pub non_target_errors: usize,
    pub non_target_wer: Option<f64>,
    pub target_free_utterances: usize,
    pub false_alarm_utterances: usize,
    pub false_alarm_rate: Option<f64>,
    pub false_alarm_upper95: Option<f64>,
    pub spurious_hint_insertions: usize,
    pub gate_changes: usize,
    pub harmful_gate_changes: usize,
    pub near_miss_changes: usize,
    pub harmful_near_miss_changes: usize,
    pub near_miss_false_alarm_utterances: usize,
    pub harmful_edits_per_1000_words: Option<f64>,
    pub paired_regressions: usize,
    pub paired_improvements: usize,
    pub paired_extra_errors_per_1000_words: Option<f64>,
    pub asr_ms_p50: Option<f64>,
    pub asr_ms_p95: Option<f64>,
    pub added_ms_p50: Option<f64>,
    pub added_ms_p95: Option<f64>,
    pub stage_ms_p50: Option<f64>,
    pub stage_ms_p95: Option<f64>,
    pub examples: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupKey {
    pub model: String,
    pub frontend: String,
    pub noise: String,
    pub category: String,
    pub split: String,
    pub condition: Condition,
    pub arm: String,
}

#[derive(Default)]
struct Accumulator {
    score: GroupScore,
    asr: Vec<f64>,
    added: Vec<f64>,
    stage: Vec<f64>,
    paired_extra: usize,
    paired_words: usize,
}

/// Gate arms: exact normalizations as user-asserted or project-authored
/// spellings, and exact plus near-miss repairs against `references`.
pub const GATE_ARMS: [(&str, TermTier, NearMiss); 3] = [
    ("gate_exact", TermTier::Asserted, NearMiss::Off),
    ("gate_exact_authored", TermTier::Authored, NearMiss::Off),
    ("gate_near_miss", TermTier::Asserted, NearMiss::Apply),
];

/// Near-miss changes carry this term-id prefix in `GateResult::applied`.
const NEAR_MISS_PREFIX: &str = "reference:";

/// Outcome of one arm on one sample, before aggregation.
struct ArmOutcome {
    text: Option<String>,
    stage_ms: Option<f64>,
    gate: Option<vesper_core::GateResult>,
}

/// Score saved decodes. Hook-less models decode only `none`; that decode stands
/// for every condition, and only the working set given to the gate differs.
pub fn score_rows(
    manifest: &TermManifest,
    rows: &[DecodeRow],
    conditions: &[Condition],
    include_text_only: bool,
) -> Vec<(GroupKey, GroupScore)> {
    let samples = manifest
        .samples
        .iter()
        .map(|s| (s.id.as_str(), s))
        .collect::<HashMap<_, _>>();
    let mut decoded: HashMap<(String, String, String, Condition), &DecodeRow> = HashMap::new();
    let mut models = BTreeMap::new();
    for row in rows {
        models.insert(
            (row.model.clone(), row.frontend.clone()),
            row.hook_kind.clone(),
        );
        decoded.insert(
            (
                row.model.clone(),
                row.frontend.clone(),
                row.sample_id.clone(),
                row.condition,
            ),
            row,
        );
    }
    let mut groups: BTreeMap<GroupKey, Accumulator> = BTreeMap::new();
    let mut lanes = models
        .iter()
        .map(|((model, frontend), hook)| (model.clone(), frontend.clone(), hook != "none"))
        .collect::<Vec<_>>();
    if include_text_only {
        lanes.push(("reference-text".into(), "none".into(), false));
    }
    for (model, frontend, hooked) in &lanes {
        let text_only = model == "reference-text";
        for sample in &manifest.samples {
            if text_only != sample.audio_path.is_none() {
                continue;
            }
            let baseline_key = (
                model.clone(),
                frontend.clone(),
                sample.id.clone(),
                Condition::None,
            );
            let baseline = decoded.get(&baseline_key).copied();
            if !text_only && baseline.is_none() && !*hooked {
                continue;
            }
            for &condition in conditions {
                let row = if text_only {
                    None
                } else if *hooked {
                    match decoded.get(&(
                        model.clone(),
                        frontend.clone(),
                        sample.id.clone(),
                        condition,
                    )) {
                        Some(row) => Some(*row),
                        None => continue,
                    }
                } else {
                    baseline
                };
                let terms = condition_terms(manifest, sample, condition);
                let hint_spellings = terms.iter().map(|t| t.spelling.clone()).collect::<Vec<_>>();
                // Under `none`, check against the workspace list to see spontaneous terms.
                let check_terms = if condition == Condition::None {
                    condition_terms(manifest, sample, Condition::Ws32)
                        .into_iter()
                        .map(|t| t.spelling)
                        .collect::<Vec<_>>()
                } else {
                    hint_spellings.clone()
                };
                let raw = if text_only {
                    Some(sample.reference.clone())
                } else {
                    row.and_then(|r| r.raw.clone())
                };
                let mut arms: Vec<(String, ArmOutcome)> = vec![(
                    "raw".into(),
                    ArmOutcome {
                        text: raw.clone(),
                        stage_ms: None,
                        gate: None,
                    },
                )];
                for (name, tier, mode) in GATE_ARMS {
                    let set = working_set(&terms, tier);
                    let started = std::time::Instant::now();
                    let gate = raw
                        .as_deref()
                        .map(|text| correction_gate_with(text, &set, Language::English, mode));
                    let ms = started.elapsed().as_secs_f64() * 1000.0;
                    arms.push((
                        name.into(),
                        ArmOutcome {
                            text: gate.as_ref().map(|g| g.text.clone()),
                            stage_ms: raw.as_ref().map(|_| ms),
                            gate,
                        },
                    ));
                }
                if let Some(cleaned) = row.and_then(|r| r.cleaned.clone()) {
                    let set = working_set(&terms, TermTier::Asserted);
                    let gate =
                        correction_gate_with(&cleaned, &set, Language::English, NearMiss::Off);
                    arms.push((
                        "cleaned".into(),
                        ArmOutcome {
                            text: Some(cleaned),
                            stage_ms: row.and_then(|r| r.cleanup_ms),
                            gate: None,
                        },
                    ));
                    arms.push((
                        "cleaned_gate_exact".into(),
                        ArmOutcome {
                            text: Some(gate.text.clone()),
                            stage_ms: None,
                            gate: Some(gate),
                        },
                    ));
                }
                for (arm, outcome) in arms {
                    let key = GroupKey {
                        model: model.clone(),
                        frontend: frontend.clone(),
                        noise: sample.noise.clone(),
                        category: sample.category.clone(),
                        split: sample.split.clone(),
                        condition,
                        arm: arm.clone(),
                    };
                    let accumulator = groups.entry(key).or_default();
                    // Paired comparison against the same arm without hints.
                    let paired_text = if *hooked && condition != Condition::None && arm == "raw" {
                        decoded
                            .get(&(
                                model.clone(),
                                frontend.clone(),
                                sample.id.clone(),
                                Condition::None,
                            ))
                            .and_then(|r| r.raw.clone())
                    } else {
                        None
                    };
                    add_sample(
                        accumulator,
                        sample,
                        &outcome,
                        &check_terms,
                        &hint_spellings,
                        row,
                        baseline.filter(|_| *hooked && condition != Condition::None),
                        paired_text.as_deref(),
                    );
                }
            }
        }
    }
    let _ = samples;
    groups
        .into_iter()
        .map(|(key, mut acc)| {
            let s = &mut acc.score;
            let ratio = |a: usize, b: usize| (b > 0).then(|| a as f64 / b as f64);
            s.recall_case_sensitive = ratio(s.target_hits_case_sensitive, s.target_occurrences);
            s.recall_case_insensitive = ratio(s.target_hits_case_insensitive, s.target_occurrences);
            s.wer = ratio(s.word_errors, s.reference_words);
            s.non_target_wer = ratio(s.non_target_errors, s.non_target_words);
            s.false_alarm_rate = ratio(s.false_alarm_utterances, s.target_free_utterances);
            s.false_alarm_upper95 = (s.target_free_utterances > 0)
                .then(|| upper_bound_95(s.false_alarm_utterances, s.target_free_utterances));
            s.harmful_edits_per_1000_words = (s.reference_words > 0 && key.arm.contains("gate"))
                .then(|| s.harmful_gate_changes as f64 * 1000.0 / s.reference_words as f64);
            s.paired_extra_errors_per_1000_words = (acc.paired_words > 0)
                .then(|| acc.paired_extra as f64 * 1000.0 / acc.paired_words as f64);
            s.asr_ms_p50 = percentile(&mut acc.asr, 0.5);
            s.asr_ms_p95 = percentile(&mut acc.asr, 0.95);
            s.added_ms_p50 = percentile(&mut acc.added, 0.5);
            s.added_ms_p95 = percentile(&mut acc.added, 0.95);
            s.stage_ms_p50 = percentile(&mut acc.stage, 0.5);
            s.stage_ms_p95 = percentile(&mut acc.stage, 0.95);
            (key, acc.score)
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn add_sample(
    acc: &mut Accumulator,
    sample: &TermSample,
    outcome: &ArmOutcome,
    check_terms: &[String],
    hint_terms: &[String],
    row: Option<&DecodeRow>,
    baseline: Option<&DecodeRow>,
    paired_text: Option<&str>,
) {
    let s = &mut acc.score;
    s.samples += 1;
    let reference_words = normalize(&sample.reference).split_whitespace().count();
    s.reference_words += reference_words;
    let targets = &sample.target_terms;
    for target in targets {
        s.target_occurrences += count_term(&sample.reference, target, false).max(1);
    }
    let target_free = targets.is_empty();
    if target_free {
        s.target_free_utterances += 1;
    }
    if let Some(row) = row {
        if row.error.is_none() {
            acc.asr.push(row.asr_ms);
            if let Some(base) = baseline.filter(|b| b.error.is_none()) {
                acc.added.push(row.asr_ms - base.asr_ms);
            }
        }
    }
    if let Some(ms) = outcome.stage_ms {
        acc.stage.push(ms);
    }
    let Some(text) = outcome.text.as_deref() else {
        // A failed decode stays in every denominator: targets are misses.
        s.failures += 1;
        let split = split_errors(&sample.reference, "", targets, hint_terms);
        s.word_errors += reference_words;
        s.non_target_words += split.other_words;
        s.non_target_errors += split.other_words;
        return;
    };
    for target in targets {
        let occurrences = count_term(&sample.reference, target, false).max(1);
        s.target_hits_case_sensitive += count_term(text, target, true).min(occurrences);
        s.target_hits_case_insensitive += count_term(text, target, false).min(occurrences);
    }
    let split = split_errors(&sample.reference, text, targets, hint_terms);
    s.word_errors += split.target_errors + split.other_errors;
    s.non_target_words += split.other_words;
    s.non_target_errors += split.other_errors;
    let spurious = check_terms
        .iter()
        .filter(|term| {
            !targets.iter().any(|t| t.eq_ignore_ascii_case(term))
                && count_term(text, term, false) > count_term(&sample.reference, term, false)
        })
        .count();
    s.spurious_hint_insertions += spurious;
    let changes = outcome.gate.as_ref().map_or(0, |g| g.applied.len());
    s.gate_changes += changes;
    let mut harmful = 0;
    let mut near = 0;
    if let Some(gate) = &outcome.gate {
        for change in &gate.applied {
            let wanted = contains_term(&sample.reference, &change.spelling, false);
            let heard_was_right = contains_term(&sample.reference, &change.heard, false);
            let bad = !wanted || heard_was_right;
            harmful += usize::from(bad);
            if change.term_id.starts_with(NEAR_MISS_PREFIX) {
                near += 1;
                s.harmful_near_miss_changes += usize::from(bad);
            }
        }
    }
    s.harmful_gate_changes += harmful;
    s.near_miss_changes += near;
    if target_free && near > 0 {
        s.near_miss_false_alarm_utterances += 1;
    }
    if target_free && (spurious > 0 || changes > 0) {
        s.false_alarm_utterances += 1;
        if s.examples.len() < 8 {
            s.examples.push(format!("{} | {}", sample.reference, text));
        }
    } else if !target_free && harmful > 0 && s.examples.len() < 8 {
        s.examples.push(format!("{} | {}", sample.reference, text));
    }
    if let Some(base) = paired_text {
        let with = split.target_errors + split.other_errors;
        let base_split = split_errors(&sample.reference, base, targets, hint_terms);
        let without = base_split.target_errors + base_split.other_errors;
        acc.paired_words += reference_words;
        if with > without {
            s.paired_regressions += 1;
            acc.paired_extra += with - without;
        } else if with < without {
            s.paired_improvements += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest() -> TermManifest {
        let term = |id: &str, spelling: &str, spoken: &[&str]| TermEntry {
            id: id.into(),
            spelling: spelling.into(),
            spoken: spoken.iter().map(|s| (*s).into()).collect(),
            kind: "identifier".into(),
        };
        let sample =
            |id: &str, reference: &str, ws: &str, targets: &[&str], category: &str| TermSample {
                id: id.into(),
                source_id: "synthetic".into(),
                split: "test".into(),
                language: "en-US".into(),
                audio_path: None,
                sha256: None,
                duration_seconds: None,
                reference: reference.into(),
                workspace_id: Some(ws.into()),
                target_terms: targets.iter().map(|t| (*t).into()).collect(),
                category: category.into(),
                subcategory: "carrier".into(),
                related_term_id: None,
                voice: None,
                synthetic: true,
                noise: "clean".into(),
                biasing_list: None,
            };
        TermManifest {
            schema_version: 1,
            sources: vec![],
            workspaces: vec![
                TermWorkspace {
                    id: "devops".into(),
                    label: "DevOps".into(),
                    terms: (0..40)
                        .map(|i| term(&format!("d{i}"), &format!("Term{i}"), &[]))
                        .chain([term("kubectl", "kubectl", &["kube control"])])
                        .collect(),
                },
                TermWorkspace {
                    id: "people".into(),
                    label: "People".into(),
                    terms: vec![term("wright", "Wright", &["wright"])],
                },
            ],
            distractors: (0..200)
                .map(|i| term(&format!("x{i}"), &format!("Distractor{i}"), &[]))
                .collect(),
            samples: vec![
                sample("p1", "Run kubectl now.", "devops", &["kubectl"], "positive"),
                sample(
                    "n1",
                    "I will write it down.",
                    "people",
                    &[],
                    "hard_negative",
                ),
            ],
        }
    }

    #[test]
    fn conditions_put_targets_first_and_bound_each_list() {
        let m = manifest();
        let p = &m.samples[0];
        assert!(condition_terms(&m, p, Condition::None).is_empty());
        let ws10 = condition_terms(&m, p, Condition::Ws10);
        assert_eq!((ws10.len(), ws10[0].spelling.as_str()), (10, "kubectl"));
        assert_eq!(condition_terms(&m, p, Condition::Ws32).len(), 32);
        let wrong = condition_terms(&m, p, Condition::Wrong32);
        assert!(wrong.iter().all(|t| t.spelling != "kubectl"));
        assert_eq!(condition_terms(&m, p, Condition::Large).len(), LARGE_LIST);
        // Deterministic.
        assert_eq!(ws10, condition_terms(&m, p, Condition::Ws10));
    }

    #[test]
    fn term_counting_is_word_bounded_and_case_aware() {
        assert_eq!(count_term("kubectl, and Kubectl", "kubectl", true), 1);
        assert_eq!(count_term("kubectl, and Kubectl", "kubectl", false), 2);
        assert_eq!(count_term("kubectls", "kubectl", false), 0);
        assert_eq!(count_term("use sherpa_onnx.", "sherpa_onnx", true), 1);
        assert_eq!(count_term("See Cedar Point", "Cedar Point", true), 1);
    }

    #[test]
    fn split_errors_separate_target_and_other_words() {
        let targets = vec!["kubectl".to_owned()];
        let e = split_errors("Run kubectl now", "Run kube control now", &targets, &[]);
        assert_eq!((e.target_words, e.other_words), (1, 2));
        assert_eq!(e.other_errors, 1); // one inserted ordinary word
        assert_eq!(e.target_errors, 1);
        let e = split_errors(
            "I will write it down",
            "I will Wright it down",
            &[],
            &["Wright".into()],
        );
        assert_eq!((e.other_errors, e.target_errors), (1, 0));
    }

    #[test]
    fn clopper_pearson_matches_known_values() {
        assert!((upper_bound_95(0, 3000) - 0.000998).abs() < 1e-5);
        assert!((upper_bound_95(0, 300) - 0.00994).abs() < 1e-4);
        assert!((upper_bound_95(1, 100) - 0.0466).abs() < 1e-3);
        assert_eq!(upper_bound_95(0, 0), 1.0);
    }

    #[test]
    fn text_only_negatives_score_the_gate_without_any_decode() {
        let m = manifest();
        let scores = score_rows(&m, &[], &[Condition::None, Condition::Ws32], true);
        let raw = scores
            .iter()
            .find(|(k, _)| {
                k.arm == "raw" && k.category == "hard_negative" && k.condition == Condition::Ws32
            })
            .unwrap();
        assert_eq!(raw.1.target_free_utterances, 1);
        assert_eq!(raw.1.false_alarm_utterances, 0);
        let positive = scores
            .iter()
            .find(|(k, _)| {
                k.arm == "raw" && k.category == "positive" && k.condition == Condition::None
            })
            .unwrap();
        assert_eq!(positive.1.recall_case_sensitive, Some(1.0));
    }

    #[test]
    fn failed_decodes_stay_in_the_denominator_as_misses() {
        let m = manifest();
        let mut audio = m.clone();
        audio.samples[0].audio_path = Some("a.wav".into());
        audio.samples[0].sha256 = Some("0".into());
        let row = DecodeRow {
            model: "m".into(),
            hook_kind: "none".into(),
            hook_status: "disabled".into(),
            frontend: "raw".into(),
            sample_id: "p1".into(),
            condition: Condition::None,
            hints_sent: 0,
            hint_hash: String::new(),
            hint_source: "none".into(),
            raw: None,
            cleaned: None,
            asr_ms: 0.0,
            cleanup_ms: None,
            error: Some("failed".into()),
            cleanup_error: None,
        };
        let scores = score_rows(&audio, &[row], &[Condition::None], false);
        let raw = scores.iter().find(|(k, _)| k.arm == "raw").unwrap();
        assert_eq!(
            (raw.1.failures, raw.1.recall_case_insensitive),
            (1, Some(0.0))
        );
    }
}
