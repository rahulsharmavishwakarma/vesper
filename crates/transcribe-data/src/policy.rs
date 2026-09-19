//! Pure bounded policy evaluation shared by offline replay and application shadow jobs.
use crate::AutomaticLearningState;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use transcribe_core::{memory_words, word_edits, Language, PersonalizationProfile};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
pub const MIN_FEEDBACK: usize = 4;
pub const MIN_IMPROVED: usize = 2;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Policy {
    // An extra eligibility floor. The real learner's stricter guards still apply.
    pub support_floor: u8,
    pub context_overlap: u8,
}
impl Policy {
    pub fn baseline() -> Self {
        Self {
            support_floor: 1,
            context_overlap: 0,
        }
    }
    pub fn context_default() -> Self {
        Self {
            support_floor: 1,
            context_overlap: 1,
        }
    }
    pub fn valid(self) -> bool {
        (1..=3).contains(&self.support_floor) && self.context_overlap <= 1
    }
    pub fn neighbors(self) -> Vec<Self> {
        (1..=3)
            .flat_map(|s| {
                (0..=1).map(move |c| Self {
                    support_floor: s,
                    context_overlap: c,
                })
            })
            .filter(|p| {
                self.support_floor.abs_diff(p.support_floor)
                    + self.context_overlap.abs_diff(p.context_overlap)
                    == 1
            })
            .collect()
    }
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Input {
    pub id: String,
    pub source_key: String,
    pub episode_id: String,
    pub at_ms: u64,
    pub project: String,
    pub language: Language,
    pub model_id: String,
    pub text: String,
}

#[derive(Clone, Default)]
pub struct Snapshot {
    pub profile: PersonalizationProfile,
    pub evidence: AutomaticLearningState,
    // Experimental, transient metadata from observable edits, not hidden references.
    pub teaching_terms: BTreeMap<String, BTreeSet<String>>,
}
pub fn root(parents: &[usize], mut i: usize) -> usize {
    while parents[i] != i {
        i = parents[i];
    }
    i
}
/// Audio and repair-chain identities can connect transitively through duplicate events.
pub fn components(pairs: &[(&str, &str)]) -> Vec<usize> {
    let mut parents: Vec<_> = (0..pairs.len()).collect();
    let mut sources = BTreeMap::new();
    let mut chains = BTreeMap::new();
    for (i, (source, chain)) in pairs.iter().enumerate() {
        for previous in [sources.insert(*source, i), chains.insert(*chain, i)]
            .into_iter()
            .flatten()
        {
            let from = root(&parents, i);
            let to = root(&parents, previous);
            parents[from] = to;
        }
    }
    (0..pairs.len()).map(|i| root(&parents, i)).collect()
}
pub fn terms(text: &str) -> BTreeSet<String> {
    memory_words(text)
        .into_iter()
        .filter(|s| {
            ![
                "open", "run", "show", "use", "in", "for", "on", "of", "with", "this", "that", "it",
            ]
            .contains(&s.as_str())
        })
        .collect()
}
pub fn prediction(snapshot: &Snapshot, input: &Input, policy: Policy) -> Result<String> {
    if !policy.valid() {
        return Err("Invalid eligibility policy".into());
    }
    let mut profile = snapshot.profile.clone();
    profile.active_project = input.project.clone();
    let mut current_terms = terms(&input.text);
    for p in terms(&input.project) {
        current_terms.remove(&p);
    }
    for entry in &mut profile.vocabulary {
        if !entry.enabled {
            continue;
        }
        let Some(source) = &entry.automatic_source else {
            continue;
        };
        let candidate = snapshot
            .evidence
            .candidates
            .iter()
            .find(|c| &c.id == source)
            .ok_or("Automatic provenance missing from the experiment snapshot")?;
        // Retained edit events include duplicate bridges omitted from accepted support.
        let mut links: Vec<_> = candidate
            .support
            .iter()
            .map(|s| (s.source_key.as_str(), s.episode_id.as_str()))
            .collect();
        for event in &snapshot.evidence.events {
            if event.context.language == candidate.language
                && event.context.project == candidate.project
                && event
                    .pair
                    .as_ref()
                    .is_some_and(|p| p.heard == candidate.alias && p.intended == candidate.phrase)
            {
                links.push((&event.context.source_key, &event.context.episode_id));
            }
        }
        let groups = components(&links);
        let independent: BTreeSet<_> = groups.iter().take(candidate.support.len()).collect();
        if independent.len() < policy.support_floor as usize {
            entry.enabled = false;
        }
        if policy.context_overlap > 0
            && !candidate.support.iter().any(|s| {
                snapshot
                    .teaching_terms
                    .get(&s.event_id)
                    .is_some_and(|context| {
                        context.intersection(&current_terms).count()
                            >= policy.context_overlap as usize
                    })
            })
        {
            entry.enabled = false;
        }
    }
    Ok(profile.personalize(&input.text, input.language).text)
}

/// Deliberately cannot contain hidden reference or audit labels.
#[derive(Clone)]
pub struct FeedbackCase {
    pub snapshot: Snapshot,
    pub input: Input,
    pub desired: String,
    pub failure: Option<String>,
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Comparison {
    pub cases: usize,
    pub independent_cases: usize,
    pub incumbent_errors: usize,
    pub candidate_errors: usize,
    pub improved: usize,
    pub regressed: usize,
    pub incumbent_harmful: usize,
    pub candidate_harmful: usize,
    pub failures: Vec<String>,
}
/// Compact immutable counterfactuals. Contains no model-generated correctness labels.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Scores {
    pub id: String,
    pub source_key: String,
    pub episode_id: String,
    pub errors: [Option<usize>; 6],
    pub raw_errors: Option<usize>,
    pub failure: Option<String>,
}
impl Policy {
    pub fn all() -> [Self; 6] {
        std::array::from_fn(|i| Self {
            support_floor: (i / 2 + 1) as u8,
            context_overlap: (i % 2) as u8,
        })
    }
    pub fn index(self) -> usize {
        (self.support_floor as usize - 1) * 2 + self.context_overlap as usize
    }
}
impl Scores {
    pub fn freeze(c: &FeedbackCase) -> Self {
        Self::freeze_for(c, &Policy::all())
    }
    fn freeze_for(c: &FeedbackCase, policies: &[Policy]) -> Self {
        let mut failure = c.failure.clone();
        let mut errors = [None; 6];
        for &p in policies {
            if !p.valid() {
                failure = Some("Invalid policy".into());
                continue;
            }
            errors[p.index()] = match prediction(&c.snapshot, &c.input, p) {
                Ok(text) => word_edits(&c.desired, &text),
                Err(_) => {
                    failure = Some("Prediction or provenance unavailable".into());
                    None
                }
            };
        }
        Self {
            id: c.input.id.clone(),
            source_key: c.input.source_key.clone(),
            episode_id: c.input.episode_id.clone(),
            errors,
            raw_errors: word_edits(&c.desired, &c.input.text),
            failure,
        }
    }
}
pub fn compare(cases: &[FeedbackCase], incumbent: Policy, candidate: Policy) -> Comparison {
    compare_scores(
        &cases
            .iter()
            .map(|c| Scores::freeze_for(c, &[incumbent, candidate]))
            .collect::<Vec<_>>(),
        incumbent,
        candidate,
    )
}
pub fn compare_scores(cases: &[Scores], incumbent: Policy, candidate: Policy) -> Comparison {
    if !incumbent.valid() || !candidate.valid() {
        return Comparison {
            failures: vec!["Invalid policy".into()],
            ..Default::default()
        };
    }
    let mut score = Comparison::default();
    let pairs: Vec<_> = cases
        .iter()
        .map(|c| (c.source_key.as_str(), c.episode_id.as_str()))
        .collect();
    let groups = components(&pairs);
    // Give each independent component one vote, but inspect *every* member.
    // Worst-case errors and any regression/failure make ordering or retry volume
    // unable to hide adverse feedback or manufacture stronger evidence.
    let mut grouped = BTreeMap::<usize, Comparison>::new();
    for (index, c) in cases.iter().enumerate() {
        score.cases += 1;
        if let Some(error) = &c.failure {
            score.failures.push(format!("{}: {error}", c.id));
        }
        let group = grouped.entry(groups[index]).or_default();
        if c.failure.is_some() {
            continue;
        }
        let evaluated = c.errors[incumbent.index()]
            .zip(c.errors[candidate.index()])
            .zip(c.raw_errors)
            .map(|((a, b), r)| (a, b, r))
            .ok_or("Score unavailable");
        match evaluated {
            Ok((old, new, raw)) => {
                group.incumbent_errors = group.incumbent_errors.max(old);
                group.candidate_errors = group.candidate_errors.max(new);
                group.improved |= usize::from(new < old);
                group.regressed |= usize::from(new > old);
                group.incumbent_harmful |= usize::from(old > raw);
                group.candidate_harmful |= usize::from(new > raw);
            }
            Err(e) => score.failures.push(format!("{}: {e}", c.id)),
        }
    }
    score.independent_cases = grouped.len();
    for group in grouped.values() {
        score.incumbent_errors += group.incumbent_errors;
        score.candidate_errors += group.candidate_errors;
        score.improved += usize::from(group.improved > 0 && group.regressed == 0);
        score.regressed += group.regressed;
        score.incumbent_harmful += group.incumbent_harmful;
        score.candidate_harmful += group.candidate_harmful;
    }
    score.failures.sort();
    score
}
pub fn passes(c: &Comparison) -> bool {
    c.failures.is_empty()
        && c.independent_cases >= MIN_FEEDBACK
        && c.improved >= MIN_IMPROVED
        && c.regressed == 0
        && c.candidate_errors < c.incumbent_errors
        && c.candidate_harmful <= c.incumbent_harmful
}
pub fn preserves_history(c: &Comparison) -> bool {
    c.failures.is_empty()
        && c.regressed == 0
        && c.candidate_errors <= c.incumbent_errors
        && c.candidate_harmful <= c.incumbent_harmful
}

pub fn proposal(cases: &[FeedbackCase], active: Policy) -> Option<Policy> {
    proposal_with_history(cases, active, &[])
}
pub fn proposal_with_history(
    cases: &[FeedbackCase],
    active: Policy,
    history: &[FeedbackCase],
) -> Option<Policy> {
    active
        .neighbors()
        .into_iter()
        .filter_map(|p| {
            let c = compare(cases, active, p);
            (passes(&c) && preserves_history(&compare(history, active, p)))
                .then_some(((c.candidate_harmful, c.candidate_errors, p), p))
        })
        .min_by_key(|(key, _)| *key)
        .map(|(_, p)| p)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Decision {
    pub round: String,
    pub action: String,
    pub previous: Policy,
    pub proposed: Option<Policy>,
    pub active: Policy,
    pub development: Option<Comparison>,
    pub validation: Option<Comparison>,
    pub rollback: Option<Comparison>,
    pub historical_validation: Option<Comparison>,
    pub historical_rollback: Option<Comparison>,
    pub bank_cases: usize,
    pub reason: String,
}
#[derive(Default)]
pub struct Controller {
    pub active: Policy,
    pub parent: Option<Policy>,
    pub decisions: Vec<Decision>,
}
impl Controller {
    pub fn new() -> Self {
        Self {
            active: Policy::baseline(),
            ..Default::default()
        }
    }
    pub fn finish(
        &mut self,
        id: &str,
        candidate: Option<Policy>,
        develop: &[FeedbackCase],
        validation: &[FeedbackCase],
    ) {
        self.finish_with_history(id, candidate, develop, validation, &[]);
    }
    pub fn finish_with_history(
        &mut self,
        id: &str,
        candidate: Option<Policy>,
        develop: &[FeedbackCase],
        validation: &[FeedbackCase],
        history: &[FeedbackCase],
    ) {
        let previous = self.active;
        let rollback = self.parent.map(|p| compare(validation, self.active, p));
        let historical_rollback = self.parent.map(|p| compare(history, self.active, p));
        if self.parent.is_some()
            && rollback.as_ref().is_some_and(passes)
            && historical_rollback.as_ref().is_some_and(preserves_history)
        {
            self.active = self.parent.take().unwrap();
            self.decisions.push(Decision { round:id.into(), action:"rolled_back".into(), previous,
                proposed:candidate, active:self.active, development:None, validation:None, rollback,
                historical_validation:None, historical_rollback, bank_cases:history.len(),
                reason:"Fresh independent feedback favors the saved previous policy without per-case regression.".into() });
            return;
        }
        let development = candidate.map(|p| compare(develop, self.active, p));
        let validation_score = candidate.map(|p| compare(validation, self.active, p));
        let historical_validation = candidate.map(|p| compare(history, self.active, p));
        let accepted = candidate.is_some_and(|p| self.active.neighbors().contains(&p))
            && development.as_ref().is_some_and(passes)
            && validation_score.as_ref().is_some_and(passes)
            && historical_validation
                .as_ref()
                .is_some_and(preserves_history);
        if accepted {
            self.parent = Some(self.active);
            self.active = candidate.unwrap();
        }
        self.decisions.push(Decision { round:id.into(), action:if accepted {"promoted"} else if candidate.is_some() {"rejected"} else {"unchanged"}.into(),
            previous, proposed:candidate, active:self.active, development, validation:validation_score, rollback,
            historical_validation, historical_rollback, bank_cases:history.len(),
            reason:if accepted {"Proposal improved development and fresh validation with no per-case regressions."}
                else {"Insufficient independent evidence, no strict gain, or a regression/failure; retain incumbent."}.into() });
    }
}

use transcribe_core::{language_matches, memory_phrase_occurs, project_matches};
/// Keys depend only on observable pre-event memory and the current hypothesis.
/// Multiple matching vocabulary entries or cues are deliberately ambiguous.
pub fn scope_key(snapshot: &Snapshot, input: &Input) -> Option<String> {
    let mut matched = 0;
    let mut cues = BTreeSet::new();
    let current = terms(&input.text);
    for entry in &snapshot.profile.vocabulary {
        if !entry.enabled
            || !language_matches(entry.language, input.language)
            || !project_matches(&entry.project, &input.project)
            || !entry
                .aliases
                .iter()
                .any(|a| memory_phrase_occurs(&input.text, a))
        {
            continue;
        }
        let source = entry.automatic_source.as_ref()?;
        let candidate = snapshot
            .evidence
            .candidates
            .iter()
            .find(|c| &c.id == source)?;
        matched += 1;
        for support in &candidate.support {
            if let Some(teaching) = snapshot.teaching_terms.get(&support.event_id) {
                cues.extend(teaching.intersection(&current).cloned());
            }
        }
    }
    if matched != 1 || cues.len() != 1 {
        return None;
    }
    Some(
        serde_json::json!([
            input.project.trim().to_lowercase(),
            input.language,
            cues.first()?
        ])
        .to_string(),
    )
}
