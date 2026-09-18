//! Offline lexical routing experiment. No production storage or inference ownership.
use super::*;
use std::collections::VecDeque;
use transcribe_core::{language_matches, memory_phrase_occurs, project_matches};

pub(super) const MAX_SCOPES: usize = 32;

/// Keys depend only on observable pre-event memory and the current hypothesis.
/// Multiple matching vocabulary entries or cues are deliberately ambiguous.
pub(super) fn key(snapshot: &Snapshot, input: &Input) -> Option<String> {
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
        json!([
            input.project.trim().to_lowercase(),
            input.language,
            cues.first()?
        ])
        .to_string(),
    )
}

/// A correlated component cannot be split across scopes to hide disagreement.
fn grouped(cases: &[FeedbackCase]) -> BTreeMap<String, Vec<FeedbackCase>> {
    let pairs: Vec<_> = cases
        .iter()
        .map(|c| (c.input.source_key.as_str(), c.input.episode_id.as_str()))
        .collect();
    let components = components(&pairs);
    let keys: Vec<_> = cases.iter().map(|c| key(&c.snapshot, &c.input)).collect();
    let mut scopes = BTreeMap::<usize, BTreeSet<Option<String>>>::new();
    for (component, key) in components.iter().zip(&keys) {
        scopes.entry(*component).or_default().insert(key.clone());
    }
    let mut result = BTreeMap::<String, Vec<FeedbackCase>>::new();
    for (i, case) in cases.iter().enumerate() {
        if scopes[&components[i]].len() == 1 {
            if let Some(key) = &keys[i] {
                result.entry(key.clone()).or_default().push(case.clone());
            }
        }
    }
    result
}

#[derive(Default)]
pub(super) struct Scoped {
    controllers: BTreeMap<String, Controller>,
    order: VecDeque<String>,
    pub(super) decisions: Vec<Value>,
    evicted: usize,
}
impl Scoped {
    fn active(&self, key: &str) -> Policy {
        self.controllers
            .get(key)
            .map(|c| c.active)
            .unwrap_or_else(Policy::context_default)
    }
    pub(super) fn propose(&self, development: &[FeedbackCase]) -> BTreeMap<String, Policy> {
        grouped(development)
            .into_iter()
            .filter_map(|(key, cases)| proposal(&cases, self.active(&key)).map(|p| (key, p)))
            .collect()
    }
    pub(super) fn finish(
        &mut self,
        round: &str,
        proposals: &BTreeMap<String, Policy>,
        development: &[FeedbackCase],
        validation: &[FeedbackCase],
    ) {
        let development = grouped(development);
        let validation = grouped(validation);
        let keys: BTreeSet<_> = proposals.keys().chain(validation.keys()).cloned().collect();
        for key in keys {
            let candidate = proposals.get(&key).copied();
            if candidate.is_none() && !self.controllers.contains_key(&key) {
                continue;
            }
            if !self.controllers.contains_key(&key) {
                if self.controllers.len() == MAX_SCOPES {
                    self.controllers.remove(&self.order.pop_front().unwrap());
                    self.evicted += 1;
                }
                self.order.push_back(key.clone());
                let mut controller = Controller::new();
                controller.active = Policy::context_default();
                self.controllers.insert(key.clone(), controller);
            }
            let controller = self.controllers.get_mut(&key).unwrap();
            controller.finish(
                round,
                candidate,
                development.get(&key).map(Vec::as_slice).unwrap_or(&[]),
                validation.get(&key).map(Vec::as_slice).unwrap_or(&[]),
            );
            let mut decision = json!(controller.decisions.pop().unwrap());
            decision["scope"] = json!(key);
            self.decisions.push(decision);
        }
    }
    pub(super) fn stats(&self) -> Value {
        json!({"limit":MAX_SCOPES,"retained":self.controllers.len(),"evicted":self.evicted,
            "policies":self.controllers.iter().map(|(k,c)|json!({"scope":k,"active":c.active})).collect::<Vec<_>>()})
    }
}

#[derive(Clone, Copy)]
pub(super) enum Route<'a> {
    Global(Policy),
    Scoped(&'a Scoped),
}
impl Route<'_> {
    pub(super) fn select(self, snapshot: &Snapshot, input: &Input) -> (Policy, Option<String>) {
        match self {
            Self::Global(policy) => (policy, None),
            Self::Scoped(scopes) => {
                let key = key(snapshot, input);
                let policy = key
                    .as_ref()
                    .map(|k| scopes.active(k))
                    .unwrap_or_else(Policy::context_default);
                (policy, key)
            }
        }
    }
}
