//! Bounded, evaluation-only policy decisions. Nothing here selects a live policy.
use crate::{policy::*, DatasetError, DatasetStore, Result};
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use vesper_core::LearningContext;

const TTL: i64 = 30 * 24 * 60 * 60;
const MAX_CASES: usize = 256;
const MAX_SCOPES: usize = 32;
const MAX_DECISIONS: usize = 8;

/// Owned before the edit is learned. Never serialize a whole personal profile.
pub struct PolicyShadowJob {
    pub snapshot: Snapshot,
    pub input: Input,
    pub desired: String,
    pub reservation: PolicyReservation,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PolicyReservation {
    pub schema_version: u32,
    pub token: String,
    pub created_at: i64,
    pub context: LearningContext,
    pub revision: u64,
    pub session_id: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Evidence {
    sequence: u64,
    created_at: i64,
    result_id: String,
    session_id: Option<String>,
    // Frozen at admission, before any proposal is selected.
    validation: bool,
    scope: Option<String>,
    scores: Scores,
    supports: BTreeSet<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PolicyShadowDecision {
    pub scope: String,
    pub version: u64,
    pub previous_version: u64,
    pub incumbent: Policy,
    pub candidate: Option<Policy>,
    pub development_trials: Vec<(Policy, Comparison)>,
    pub development: Option<Comparison>,
    pub validation: Option<Comparison>,
    pub eligible: bool,
    pub reason: String,
    pub supporting_results: BTreeSet<String>,
    pub created_at: i64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PolicyShadowState {
    pub schema_version: u32,
    pub version: u64,
    evidence: Vec<Evidence>,
    /// Identity links carry no scores or success labels.
    correlations: Vec<(String, String, i64)>,
    pub decisions: Vec<PolicyShadowDecision>,
    pub failures: Vec<String>,
}
impl Default for PolicyShadowState {
    fn default() -> Self {
        Self {
            schema_version: 1,
            version: 0,
            evidence: vec![],
            correlations: vec![],
            decisions: vec![],
            failures: vec![],
        }
    }
}
impl PolicyShadowState {
    fn prune(&mut self) {
        let cutoff = chrono::Utc::now().timestamp() - TTL;
        let expired_results: BTreeSet<_> = self
            .evidence
            .iter()
            .filter(|e| e.created_at <= cutoff)
            .map(|e| e.result_id.clone())
            .collect();
        self.evidence.retain(|e| e.created_at > cutoff);
        self.correlations.retain(|(_, _, at)| *at > cutoff);
        self.decisions
            .retain(|d| d.supporting_results.is_disjoint(&expired_results));
        self.decisions.retain(|d| d.created_at > cutoff);
        if self.evidence.len() > MAX_CASES {
            self.evidence.drain(..self.evidence.len() - MAX_CASES);
        }
        let mut retained_scopes = BTreeSet::new();
        for e in self.evidence.iter_mut().rev() {
            if let Some(scope) = &e.scope {
                if retained_scopes.contains(scope) || retained_scopes.len() < MAX_SCOPES {
                    retained_scopes.insert(scope.clone());
                } else {
                    e.scope = None;
                } // Retain identity bridges, never reuse an evicted scope's votes.
            }
        }
        let mut counts = BTreeMap::<String, usize>::new();
        self.decisions.reverse();
        self.decisions.retain(|d| {
            if !counts.contains_key(&d.scope) && counts.len() == MAX_SCOPES {
                return false;
            }
            let count = counts.entry(d.scope.clone()).or_default();
            *count += 1;
            *count <= MAX_DECISIONS
        });
        self.decisions.reverse();
    }
    /// Strong fixed fallback is also the shadow incumbent: eligible proposals do
    /// not become fresh labels or another round's incumbent. Live promotion is gated.
    fn evaluate(&mut self, scope: &str) {
        let incumbent = Policy::context_default();
        let mut pairs: Vec<_> = self
            .evidence
            .iter()
            .map(|e| (e.scores.source_key.as_str(), e.scores.episode_id.as_str()))
            .collect();
        pairs.extend(
            self.correlations
                .iter()
                .map(|(source, episode, _)| (source.as_str(), episode.as_str())),
        );
        let groups = components(&pairs);
        let mut routes = BTreeMap::<usize, BTreeSet<(Option<String>, bool)>>::new();
        for (e, g) in self.evidence.iter().zip(&groups) {
            routes
                .entry(*g)
                .or_default()
                .insert((e.scope.clone(), e.validation));
        }
        let mut develop = vec![];
        let mut validation = vec![];
        let mut supporting_results = BTreeSet::new();
        for (e, g) in self.evidence.iter().zip(&groups) {
            // A late transitive bridge can invalidate an earlier partition/scope.
            if e.scope.as_deref() == Some(scope) && routes[g].len() == 1 {
                if e.validation {
                    validation.push(e.scores.clone());
                } else {
                    develop.push(e.scores.clone());
                }
                supporting_results.insert(e.result_id.clone());
                supporting_results.extend(e.supports.clone());
            }
        }
        let development_trials: Vec<_> = incumbent
            .neighbors()
            .into_iter()
            .map(|p| (p, compare_scores(&develop, incumbent, p)))
            .collect();
        let candidate = development_trials
            .iter()
            .filter(|(_, c)| passes(c))
            .min_by_key(|(p, c)| (c.candidate_harmful, c.candidate_errors, *p))
            .map(|(p, _)| *p);
        let development = candidate.map(|p| compare_scores(&develop, incumbent, p));
        let validation = candidate.map(|p| compare_scores(&validation, incumbent, p));
        let eligible =
            development.as_ref().is_some_and(passes) && validation.as_ref().is_some_and(passes);
        let previous_version = self
            .decisions
            .iter()
            .rev()
            .find(|d| d.scope == scope)
            .map_or(0, |d| d.version);
        self.decisions.push(PolicyShadowDecision { scope: scope.into(), version: self.version, previous_version,
            incumbent, candidate, development_trials, development, validation, eligible, supporting_results,
            reason: if eligible { "Shadow gates passed; active policy unchanged. Natural qualification and live promotion remain gated." }
                else { "Fixed fallback: insufficient independent feedback, failed score, or regression." }.into(),
            created_at: chrono::Utc::now().timestamp() });
        self.prune();
    }
}
impl DatasetStore {
    pub fn prepare_policy_shadow(
        &self,
        context: &LearningContext,
        revision: u64,
        session_id: Option<String>,
        raw: &str,
        desired: &str,
    ) -> Result<Option<PolicyShadowJob>> {
        if !context.learning_enabled
            || raw.len() > 8192
            || desired.len() > 8192
            || context.result_id.is_empty()
            || context.source_key.is_empty()
            || context.episode_id.is_empty()
            || context.result_id.len() > 64
            || context.source_key.len() > 128
            || context.episode_id.len() > 64
        {
            return Ok(None);
        }
        let tx = rusqlite::Transaction::new_unchecked(
            &self.connection,
            rusqlite::TransactionBehavior::Deferred,
        )?;
        let profile = self.personalization()?;
        if !profile.automatic_learning
            || profile.learning_epoch != context.learning_epoch
            || !self.learning_operation_current(context)?
        {
            return Ok(None);
        }
        let evidence = self.automatic_learning_state()?;
        let teaching_terms = evidence
            .candidates
            .iter()
            .flat_map(|c| c.support.iter())
            .map(|s| {
                (
                    s.event_id.clone(),
                    s.teaching_terms.iter().cloned().collect(),
                )
            })
            .collect();
        tx.commit()?;
        Ok(Some(PolicyShadowJob {
            snapshot: Snapshot {
                profile,
                evidence,
                teaching_terms,
            },
            input: Input {
                id: format!("{}:{revision}", context.result_id),
                source_key: context.source_key.clone(),
                episode_id: context.episode_id.clone(),
                at_ms: chrono::Utc::now().timestamp_millis() as u64,
                project: context.project.clone(),
                language: context.language,
                model_id: context.model_id.clone(),
                text: raw.into(),
            },
            desired: desired.into(),
            reservation: PolicyReservation {
                schema_version: 1,
                token: uuid::Uuid::new_v4().to_string(),
                created_at: chrono::Utc::now().timestamp(),
                context: context.clone(),
                revision,
                session_id,
            },
        }))
    }
    /// Called only after the ordinary draft transaction succeeds.
    pub fn reserve_policy_shadow(&self, job: &PolicyShadowJob) -> Result<bool> {
        let tx = self.connection.unchecked_transaction()?;
        let r = &job.reservation;
        if !self.policy_reservation_current(r)? {
            return Ok(false);
        }
        if let Some(id) = &r.session_id {
            let record = crate::read_record(&tx, id)?;
            if record.personalization_rejected
                || record.draft_revision != r.revision
                || record.draft_transcript.as_deref() != Some(&job.desired)
            {
                return Ok(false);
            }
        }
        tx.execute("INSERT INTO policy_shadow_job (id, request_json) VALUES (1, ?1) ON CONFLICT(id) DO UPDATE SET request_json=excluded.request_json", [serde_json::to_string(r)?])?;
        tx.commit()?;
        Ok(true)
    }
    fn policy_reservation_current(&self, r: &PolicyReservation) -> Result<bool> {
        let p = self.personalization()?;
        Ok(r.created_at > chrono::Utc::now().timestamp() - TTL
            && r.schema_version == 1
            && r.context.learning_enabled
            && p.automatic_learning
            && p.learning_epoch == r.context.learning_epoch
            && self.learning_operation_current(&r.context)?)
    }
    pub fn finish_policy_shadow(
        &self,
        job: PolicyShadowJob,
        cancelled: &std::sync::atomic::AtomicBool,
    ) -> Result<bool> {
        use std::sync::atomic::Ordering;
        if cancelled.load(Ordering::Acquire) {
            return Ok(false);
        }
        let scope = scope_key(&job.snapshot, &job.input);
        let matched_sources: BTreeSet<_> = job
            .snapshot
            .profile
            .vocabulary
            .iter()
            .filter(|v| {
                scope.is_some()
                    && v.enabled
                    && v.aliases
                        .iter()
                        .any(|a| vesper_core::memory_phrase_occurs(&job.input.text, a))
            })
            .filter_map(|v| v.automatic_source.as_ref())
            .collect();
        let supports = job
            .snapshot
            .evidence
            .candidates
            .iter()
            .filter(|c| matched_sources.contains(&c.id))
            .flat_map(|c| &c.support)
            .filter_map(|s| {
                s.event_id
                    .rsplit_once(':')
                    .map(|(result, _)| result.to_owned())
            })
            .collect();
        let reservation = job.reservation;
        let scores = Scores::freeze(&FeedbackCase {
            snapshot: job.snapshot,
            input: job.input,
            desired: job.desired,
            failure: None,
        });
        if cancelled.load(Ordering::Acquire) {
            return Ok(false);
        }
        let tx = self.connection.unchecked_transaction()?;
        let saved: Option<String> = tx
            .query_row(
                "SELECT request_json FROM policy_shadow_job WHERE id=1",
                [],
                |r| r.get(0),
            )
            .optional()?;
        if saved.as_deref() != Some(&serde_json::to_string(&reservation)?)
            || !self.policy_reservation_current(&reservation)?
        {
            return Ok(false);
        }
        let mut state = self.policy_shadow_state()?;
        state.version = state
            .version
            .checked_add(1)
            .ok_or_else(|| DatasetError::InvalidConfig("Shadow version exhausted".into()))?;
        // Even cancelled/coalesced edits and raw observations can connect two
        // identities. They never supply a score but must not disappear as bridges.
        state.correlations = self
            .automatic_learning_state()?
            .events
            .into_iter()
            .map(|e| {
                (
                    e.context.source_key,
                    e.context.episode_id,
                    e.timestamp_ms / 1000,
                )
            })
            .take(1024)
            .collect();
        state.evidence.push(Evidence {
            sequence: state.version,
            validation: state.version % 2 == 0,
            created_at: chrono::Utc::now().timestamp(),
            result_id: reservation.context.result_id.clone(),
            session_id: reservation.session_id.clone(),
            scope: scope.clone(),
            scores,
            supports,
        });
        state.prune();
        // Reevaluate every affected scope: unknown/split-scope bridges must not leave
        // an older eligible proposal appearing current. Historical records remain evidence.
        let scopes: BTreeSet<_> = state
            .evidence
            .iter()
            .filter_map(|e| e.scope.clone())
            .collect();
        for scope in scopes.into_iter().take(MAX_SCOPES) {
            state.evaluate(&scope);
        }
        if scope.is_none() {
            state.failures = vec!["Ambiguous or missing pre-event scope; fixed fallback.".into()];
        }
        if cancelled.load(Ordering::Acquire) {
            return Ok(false);
        }
        tx.execute("INSERT INTO policy_shadow (id, state_json) VALUES (1, ?1) ON CONFLICT(id) DO UPDATE SET state_json=excluded.state_json", [serde_json::to_string(&state)?])?;
        tx.execute("DELETE FROM policy_shadow_job WHERE id=1", [])?;
        tx.commit()?;
        Ok(true)
    }
    pub fn policy_shadow_state(&self) -> Result<PolicyShadowState> {
        // Pruning is a read-modify-write operation too. Reuse the caller's
        // mutation transaction, or reserve a writer before reading the state.
        let tx = self
            .connection
            .is_autocommit()
            .then(|| self.connection.unchecked_transaction())
            .transpose()?;
        self.connection.execute("DELETE FROM policy_shadow_job WHERE CASE WHEN json_valid(request_json) THEN coalesce(json_extract(request_json,'$.createdAt'),0) ELSE 0 END <= ?1", [chrono::Utc::now().timestamp()-TTL])?;
        let raw: Option<String> = self
            .connection
            .query_row("SELECT state_json FROM policy_shadow WHERE id=1", [], |r| {
                r.get(0)
            })
            .optional()?;
        let mut state = match raw {
            None => PolicyShadowState::default(),
            Some(raw) => serde_json::from_str::<PolicyShadowState>(&raw)
                .ok()
                .filter(|s| {
                    s.schema_version == 1
                        && s.evidence.len() <= MAX_CASES
                        && s.correlations.len() <= 1024
                        && s.version < u64::MAX
                        && s.evidence.iter().all(|e| {
                            e.sequence <= s.version
                                && e.scores.errors.iter().flatten().all(|n| *n <= 8192)
                                && e.scores.raw_errors.is_none_or(|n| n <= 8192)
                        })
                        && s.decisions.len() <= MAX_SCOPES * MAX_DECISIONS
                        && s.decisions
                            .iter()
                            .all(|d| d.incumbent.valid() && d.candidate.is_none_or(Policy::valid))
                })
                .unwrap_or_else(|| PolicyShadowState {
                    failures: vec!["Unsupported or corrupt shadow schema; fixed fallback.".into()],
                    ..Default::default()
                }),
        };
        state.prune();
        let encoded = serde_json::to_string(&state)?;
        self.connection.execute(
            "UPDATE policy_shadow SET state_json=?1 WHERE id=1 AND state_json!=?1",
            [encoded],
        )?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(state)
    }
    pub(crate) fn invalidate_shadow_session(&self, session_id: &str) -> Result<()> {
        let state = self.policy_shadow_state()?;
        let mut ids: BTreeSet<_> = state
            .evidence
            .iter()
            .filter(|e| e.session_id.as_deref() == Some(session_id))
            .map(|e| e.result_id.clone())
            .collect();
        let pending: Option<String> = self
            .connection
            .query_row(
                "SELECT request_json FROM policy_shadow_job WHERE id=1",
                [],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(pending) = pending
            .and_then(|s| serde_json::from_str::<PolicyReservation>(&s).ok())
            .filter(|r| r.session_id.as_deref() == Some(session_id))
        {
            ids.insert(pending.context.result_id);
        }
        for id in ids {
            self.invalidate_policy_result(&id)?;
        }
        Ok(())
    }
    pub(crate) fn invalidate_policy_result(&self, result_id: &str) -> Result<()> {
        let mut s = self.policy_shadow_state()?;
        s.evidence
            .retain(|e| e.result_id != result_id && !e.supports.contains(result_id));
        s.decisions
            .retain(|d| !d.supporting_results.contains(result_id));
        // Conservatively invalidate the one pending job, including its pre-edit snapshot.
        self.connection
            .execute("DELETE FROM policy_shadow_job", [])?;
        self.connection.execute(
            "UPDATE policy_shadow SET state_json=?1 WHERE id=1",
            [serde_json::to_string(&s)?],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;
    fn context(store: &DatasetStore, id: &str) -> LearningContext {
        let p = store.dictation_personalization().unwrap();
        let context = LearningContext {
            result_id: id.into(),
            source_key: format!("audio-{id}"),
            episode_id: id.into(),
            learning_epoch: p.learning_epoch,
            learning_enabled: true,
            language: vesper_core::Language::English,
            ..Default::default()
        };
        store
            .observe_learning(&context, "authored fixture", None)
            .unwrap();
        context
    }
    fn evidence(i: u64) -> Evidence {
        Evidence {
            sequence: i,
            created_at: chrono::Utc::now().timestamp(),
            result_id: format!("r{i}"),
            session_id: None,
            validation: i % 2 == 0,
            scope: Some("scope".into()),
            supports: BTreeSet::from(["teacher".into()]),
            scores: Scores {
                id: format!("r{i}:1"),
                source_key: format!("s{i}"),
                episode_id: format!("e{i}"),
                errors: [Some(2), Some(2), Some(2), Some(0), Some(2), Some(2)],
                raw_errors: Some(2),
                failure: None,
            },
        }
    }
    #[test]
    fn independent_frozen_partitions_gate_against_strong_fixed_fallback() {
        let mut s = PolicyShadowState {
            version: 8,
            evidence: (1..=8).map(evidence).collect(),
            ..Default::default()
        };
        s.evaluate("scope");
        let d = s.decisions.last().unwrap();
        assert!(d.eligible);
        assert_eq!(d.incumbent, Policy::context_default());
        assert_eq!(
            d.candidate,
            Some(Policy {
                support_floor: 2,
                context_overlap: 1
            })
        );
        let original = d.clone();
        let mut bridge = evidence(9);
        bridge.scores.source_key = "s1".into();
        bridge.scores.episode_id = "e2".into();
        s.evidence.push(bridge);
        s.version = 9;
        s.evaluate("scope");
        assert!(!s.decisions.last().unwrap().eligible);
        assert_eq!(
            s.decisions.last().unwrap().previous_version,
            original.version
        );
        assert_eq!(
            s.decisions.last().unwrap().incumbent,
            Policy::context_default()
        );
    }
    #[test]
    fn unscored_coalesced_or_observed_bridge_cannot_manufacture_independence() {
        let mut s = PolicyShadowState {
            version: 8,
            evidence: (1..=8).map(evidence).collect(),
            ..Default::default()
        };
        s.evaluate("scope");
        assert!(s.decisions.last().unwrap().eligible);
        s.correlations
            .push(("s1".into(), "e2".into(), chrono::Utc::now().timestamp()));
        s.evaluate("scope");
        assert!(!s.decisions.last().unwrap().eligible);
    }
    #[test]
    fn scope_ambiguity_regression_and_failures_cannot_hide_in_duplicate_feedback() {
        for mode in 0..3 {
            let mut s = PolicyShadowState {
                version: 9,
                evidence: (1..=8).map(evidence).collect(),
                ..Default::default()
            };
            let mut bad = evidence(9);
            bad.scores.source_key = "s2".into();
            bad.validation = true;
            match mode {
                0 => bad.scope = None,
                1 => bad.scores.errors[3] = Some(3),
                _ => bad.scores.failure = Some("unscored failure".into()),
            }
            s.evidence.push(bad);
            s.evaluate("scope");
            assert!(!s.decisions.last().unwrap().eligible);
        }
    }
    #[test]
    fn restart_corruption_undo_and_forget_never_activate_or_resurrect_policy() {
        let dir = tempfile::tempdir().unwrap();
        let s = DatasetStore::open(dir.path().into()).unwrap();
        let c = context(&s, "r");
        let profile = s.personalization().unwrap();
        let a = s
            .prepare_policy_shadow(&c, 1, None, "raw", "edited")
            .unwrap()
            .unwrap();
        let b = s
            .prepare_policy_shadow(&c, 2, None, "raw", "new edit")
            .unwrap()
            .unwrap();
        assert!(s.reserve_policy_shadow(&a).unwrap());
        assert!(s.reserve_policy_shadow(&b).unwrap());
        assert!(!s.finish_policy_shadow(a, &AtomicBool::new(false)).unwrap());
        assert!(s.finish_policy_shadow(b, &AtomicBool::new(false)).unwrap());
        assert_eq!(s.personalization().unwrap(), profile);
        assert_eq!(s.policy_shadow_state().unwrap().evidence.len(), 1);
        drop(s);
        let s = DatasetStore::open(dir.path().into()).unwrap();
        assert_eq!(s.policy_shadow_state().unwrap().evidence.len(), 1);
        s.undo_learning_for_result("r").unwrap();
        assert!(s.policy_shadow_state().unwrap().evidence.is_empty());
        let a = s
            .prepare_policy_shadow(&c, 3, None, "raw", "edited")
            .unwrap()
            .unwrap();
        s.reserve_policy_shadow(&a).unwrap();
        s.forget_automatic_learning(profile.revision).unwrap();
        assert!(!s.finish_policy_shadow(a, &AtomicBool::new(false)).unwrap());
        for corrupt in ["{", "{\"schemaVersion\":999,\"version\":1,\"evidence\":[],\"decisions\":[],\"failures\":[]}"] {
            s.connection.execute("INSERT OR REPLACE INTO policy_shadow(id,state_json) VALUES(1,?1)",[corrupt]).unwrap();
            let state=s.policy_shadow_state().unwrap();assert!(state.evidence.is_empty());assert!(!state.failures.is_empty());
        }
    }
    #[test]
    fn snapshot_precedes_teaching_and_cancellation_preserves_live_memory() {
        let dir = tempfile::tempdir().unwrap();
        let s = DatasetStore::open(dir.path().into()).unwrap();
        for i in 0..2 {
            let c = context(&s, &format!("teacher{i}"));
            s.learn_from_edit(&c, 0, "cluster kubernets", "cluster kubernetes")
                .unwrap();
        }
        let c = context(&s, "feedback");
        let before = s.personalization().unwrap();
        let job = s
            .prepare_policy_shadow(&c, 1, None, "cluster kubernets", "cluster kubernetes")
            .unwrap()
            .unwrap();
        assert_eq!(job.snapshot.profile, before);
        assert!(scope_key(&job.snapshot, &job.input).is_some());
        s.learn_from_edit(&c, 0, "cluster kubernets", "cluster kubernetes")
            .unwrap();
        assert_eq!(job.snapshot.profile, before);
        s.reserve_policy_shadow(&job).unwrap();
        let live = s.personalization().unwrap();
        assert!(!s.finish_policy_shadow(job, &AtomicBool::new(true)).unwrap());
        assert_eq!(s.personalization().unwrap(), live);
        assert!(s.policy_shadow_state().unwrap().evidence.is_empty());
    }
    #[test]
    fn retention_limits_and_expiry_are_bounded() {
        let mut s = PolicyShadowState {
            version: 300,
            evidence: (1..=300).map(evidence).collect(),
            ..Default::default()
        };
        s.prune();
        assert_eq!(s.evidence.len(), MAX_CASES);
        for scope in 0..40 {
            for _ in 0..10 {
                s.evaluate(&format!("scope{scope}"));
            }
        }
        assert_eq!(s.decisions.len(), MAX_SCOPES * MAX_DECISIONS);
        for e in &mut s.evidence {
            e.created_at = 0;
        }
        for d in &mut s.decisions {
            d.created_at = 0;
        }
        s.prune();
        assert!(s.evidence.is_empty());
        assert!(s.decisions.is_empty());
    }
    #[test]
    fn saved_result_invalidation_preserves_unrelated_evidence_and_decisions() {
        let dir = tempfile::tempdir().unwrap();
        let store = DatasetStore::open(dir.path().into()).unwrap();
        let mut state = PolicyShadowState {
            version: 8,
            evidence: (1..=8).map(evidence).collect(),
            ..Default::default()
        };
        state.evidence[0].session_id = Some("saved".into());
        state.evaluate("scope");
        store
            .connection
            .execute(
                "INSERT INTO policy_shadow(id,state_json) VALUES(1,?1)",
                [serde_json::to_string(&state).unwrap()],
            )
            .unwrap();
        store.invalidate_shadow_session("saved").unwrap();
        let state = store.policy_shadow_state().unwrap();
        assert_eq!(state.evidence.len(), 7);
        assert!(state.decisions.is_empty());
        assert!(state.evidence.iter().all(|e| e.result_id != "r1"));
    }
}
