//! Offline bounded policy adaptation. Never opens the live app's dataset or runs inference.
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    time::Instant,
};
use vesper_core::{automatic_spelling_pair, word_edits, LearningContext, PersonalizationProfile};
#[path = "policy_learning/scoped.rs"]
mod scoped;
use vesper_data::{policy::*, DatasetStore};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const FAILURE_BANK_LIMIT: usize = 64;

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Episode {
    input: Input,
    /// Externally supplied simulated correction/review target, never implicit acceptance.
    edit: Option<String>,
    /// Hidden desired text, used only by the reporting layer.
    reference: String,
}
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Round {
    id: String,
    learn: Vec<Episode>,
    develop: Vec<Episode>,
    validate: Vec<Episode>,
    audit: Vec<Episode>,
}
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Manifest {
    schema_version: u32,
    source: Value,
    rounds: Vec<Round>,
    final_test: Vec<Episode>,
}
impl Manifest {
    fn validate(&self) -> Result<()> {
        if self.schema_version != 1 || self.rounds.is_empty() || self.rounds.len() > 16 {
            return Err("Unsupported schema or round count".into());
        }
        let mut groups = vec![];
        let mut round_ids = BTreeSet::new();
        for r in &self.rounds {
            if r.id.is_empty() || !round_ids.insert(&r.id) {
                return Err("Duplicate/empty round ID".into());
            }
            if r.develop.len() > 64 || r.validate.len() > 64 {
                return Err("Per-round policy feedback budget exceeded".into());
            }
            groups.extend([
                (format!("{}:learn", r.id), &r.learn, true),
                (format!("{}:develop", r.id), &r.develop, true),
                (format!("{}:validate", r.id), &r.validate, true),
                (format!("{}:audit", r.id), &r.audit, false),
            ]);
        }
        groups.push(("final-test".into(), &self.final_test, false));
        let mut ids = BTreeSet::new();
        let mut sources = BTreeMap::new();
        let mut chains = BTreeMap::new();
        let mut previous = None;
        let mut count = 0;
        for (group, episodes, feedback_allowed) in groups {
            for e in episodes {
                let i = &e.input;
                count += 1;
                if !ids.insert(&i.id)
                    || i.id.is_empty()
                    || i.id.len() > 128
                    || i.source_key.is_empty()
                    || i.source_key.len() > 128
                    || i.episode_id.is_empty()
                    || i.episode_id.len() > 128
                    || i.model_id.is_empty()
                    || i.model_id.len() > 128
                    || i.project.len() > 128
                    || i.text.len() > 8192
                    || e.reference.len() > 8192
                    || e.edit.as_ref().is_some_and(|s| s.len() > 8192)
                    || previous.is_some_and(|p| p >= i.at_ms)
                {
                    return Err(
                        "Invalid input bounds, duplicate ID or nonchronological event".into(),
                    );
                }
                if !feedback_allowed && e.edit.is_some() {
                    return Err("Hidden audit/test labels cannot be learning feedback".into());
                }
                previous = Some(i.at_ms);
                for (key, seen) in [(&i.source_key, &mut sources), (&i.episode_id, &mut chains)] {
                    if seen.insert(key, group.clone()).is_some_and(|g| g != group) {
                        return Err(
                            "Source or correlated episode crosses experiment partitions".into()
                        );
                    }
                }
            }
        }
        if count > 2048 {
            return Err("Episode budget exceeded".into());
        }
        Ok(())
    }
}

struct Learner {
    // TempDir remains owned for the complete run; no user database path is accepted.
    _root: tempfile::TempDir,
    store: DatasetStore,
    teaching_terms: BTreeMap<String, BTreeSet<String>>,
}
impl Learner {
    fn new() -> Result<Self> {
        let root = tempfile::tempdir()?;
        let store = DatasetStore::open(root.path().join("data"))?;
        store.save_personalization(PersonalizationProfile {
            automatic_learning: true,
            ..Default::default()
        })?;
        Ok(Self {
            _root: root,
            store,
            teaching_terms: BTreeMap::new(),
        })
    }
    fn snapshot(&self) -> Result<Snapshot> {
        Ok(Snapshot {
            profile: self.store.personalization()?,
            evidence: self.store.automatic_learning_state()?,
            teaching_terms: self.teaching_terms.clone(),
        })
    }
    fn retained_counts(&self) -> Result<Value> {
        let s = self.snapshot()?;
        Ok(
            json!({"vocabulary":s.profile.vocabulary.len(), "candidates":s.evidence.candidates.len(),
            "events":s.evidence.events.len(), "teachingContexts":s.teaching_terms.len(),
            "maxSupport":s.evidence.candidates.iter().map(|c|c.support.len()).max().unwrap_or(0)}),
        )
    }
    fn feedback(&mut self, input: &Input, before: &str, edited: &str) -> Result<()> {
        let p = self.store.personalization()?;
        let context = LearningContext {
            result_id: input.id.clone(),
            source_key: input.source_key.clone(),
            episode_id: input.episode_id.clone(),
            language: input.language,
            project: input.project.clone(),
            model_id: input.model_id.clone(),
            learning_epoch: p.learning_epoch,
            learning_enabled: true,
        };
        self.store.learn_from_edit(&context, 0, before, edited)?;
        // This experiment compares eligibility policies; the live policy breaker
        // (two rejected rules pause all automatic ones) is evaluated separately.
        self.store.resume_automatic_spellings()?;
        if let Some(pair) = automatic_spelling_pair(before, edited) {
            let id = format!("{}:0", input.id);
            let state = self.store.automatic_learning_state()?;
            if state
                .candidates
                .iter()
                .any(|c| c.support.iter().any(|s| s.event_id == id))
            {
                let mut context = terms(before);
                for excluded in terms(&pair.heard)
                    .into_iter()
                    .chain(terms(&pair.intended))
                    .chain(terms(&input.project))
                {
                    context.remove(&excluded);
                }
                self.teaching_terms.insert(id, context);
            }
            // Match metadata lifetime to the real learner's retained support.
            self.teaching_terms.retain(|id, _| {
                state
                    .candidates
                    .iter()
                    .any(|c| c.support.iter().any(|s| &s.event_id == id))
            });
        }
        Ok(())
    }
}

/// Experimental memory of observable development/validation failures only.
/// Historical cases can veto a change, but cannot supply fresh improvement votes.
#[derive(Default)]
struct FailureBank {
    cases: Vec<FeedbackCase>,
    admitted: usize,
    evicted: usize,
    skipped_failures: usize,
}
impl FailureBank {
    fn observe(&mut self, cases: &[FeedbackCase], observed_policy: Policy) {
        for case in cases {
            if case.failure.is_some() {
                self.skipped_failures += 1;
                continue;
            }
            let errors = prediction(&case.snapshot, &case.input, observed_policy)
                .ok()
                .and_then(|text| word_edits(&case.desired, &text));
            match errors {
                Some(0) => {}
                Some(_) => {
                    self.cases.push(case.clone());
                    self.admitted += 1;
                }
                None => self.skipped_failures += 1,
            }
        }
        while self.cases.len() > FAILURE_BANK_LIMIT {
            let pairs: Vec<_> = self
                .cases
                .iter()
                .map(|c| (c.input.source_key.as_str(), c.input.episode_id.as_str()))
                .collect();
            let groups = components(&pairs);
            let oldest = groups[0];
            let mut index = 0;
            self.cases.retain(|_| {
                let keep = groups[index] != oldest;
                index += 1;
                self.evicted += usize::from(!keep);
                keep
            });
        }
    }
}
#[derive(Default)]
struct Timings {
    apply: Vec<f64>,
    feedback: Vec<f64>,
    select: Vec<f64>,
    gate: Vec<f64>,
    bank_update: Vec<f64>,
}
fn timed<T>(times: &mut Vec<f64>, f: impl FnOnce() -> T) -> T {
    let t = Instant::now();
    let value = f();
    times.push(t.elapsed().as_secs_f64() * 1000.0);
    value
}
fn timing(values: &[f64]) -> Value {
    if values.is_empty() {
        return Value::Null;
    }
    let mut a = values.to_vec();
    a.sort_by(f64::total_cmp);
    let percentile = |p: f64| {
        a[((a.len() as f64 * p).ceil() as usize)
            .saturating_sub(1)
            .min(a.len() - 1)]
    };
    json!({"count":a.len(),"meanMs":a.iter().sum::<f64>()/a.len() as f64,"p50Ms":percentile(0.5),"p95Ms":percentile(0.95),"p99Ms":percentile(0.99)})
}

/// Only reporting sees Episode.reference. Learning and controller APIs do not.
fn execute(
    episodes: &[Episode],
    phase: &str,
    round: &str,
    learners: (&mut Learner, &mut Learner),
    route: scoped::Route<'_>,
    rows: &mut Vec<Value>,
    times: &mut Timings,
) -> Result<Vec<FeedbackCase>> {
    let (adaptive, baseline) = learners;
    let mut cases = vec![];
    for e in episodes {
        let snapshot = adaptive.snapshot()?;
        let base_snapshot = baseline.snapshot()?;
        let mut failures = vec![];
        let (policy, scope, result) = timed(&mut times.apply, || {
            let (policy, scope) = route.select(&snapshot, &e.input);
            (policy, scope, prediction(&snapshot, &e.input, policy))
        });
        let actual = result.unwrap_or_else(|err| {
            failures.push(err.to_string());
            e.input.text.clone()
        });
        let fixed =
            prediction(&base_snapshot, &e.input, Policy::baseline()).unwrap_or_else(|err| {
                failures.push(err.to_string());
                e.input.text.clone()
            });
        let mut feedback_error = None;
        if let Some(edit) = &e.edit {
            if let Err(err) = timed(&mut times.feedback, || {
                adaptive.feedback(&e.input, &actual, edit)
            }) {
                feedback_error = Some(err.to_string());
                failures.push(err.to_string());
            }
            if let Err(err) = baseline.feedback(&e.input, &fixed, edit) {
                feedback_error = Some(err.to_string());
                failures.push(err.to_string());
            }
            cases.push(FeedbackCase {
                snapshot,
                input: e.input.clone(),
                desired: edit.clone(),
                failure: feedback_error.or_else(|| failures.first().cloned()),
            });
        }
        let raw_errors = word_edits(&e.reference, &e.input.text);
        let adaptive_errors = word_edits(&e.reference, &actual);
        let baseline_errors = word_edits(&e.reference, &fixed);
        if raw_errors.is_none() || adaptive_errors.is_none() || baseline_errors.is_none() {
            failures
                .push("Hidden scoring budget exceeded; row retained as unscored failure".into());
        }
        rows.push(json!({"round":round,"phase":phase,"id":e.input.id,"language":e.input.language,
            "modelId":e.input.model_id,"policy":policy,"actual":actual,"baseline":fixed,
            "rawErrors":raw_errors,"adaptiveErrors":adaptive_errors,"baselineErrors":baseline_errors,"failures":failures}));
        if matches!(route, scoped::Route::Scoped(_)) {
            rows.last_mut().unwrap()["scope"] = json!(scope);
        }
    }
    Ok(cases)
}
fn aggregate(rows: &[Value]) -> Value {
    let mut raw = 0;
    let mut adaptive = 0;
    let mut baseline = 0;
    let mut scored = 0;
    let mut improved = 0;
    let mut regressed = 0;
    let mut failure_rows = 0;
    let mut adaptive_harm = 0;
    let mut baseline_harm = 0;
    for r in rows {
        failure_rows += usize::from(!r["failures"].as_array().unwrap().is_empty());
        if let (Some(a), Some(b), Some(c)) = (
            r["rawErrors"].as_u64(),
            r["adaptiveErrors"].as_u64(),
            r["baselineErrors"].as_u64(),
        ) {
            scored += 1;
            raw += a;
            adaptive += b;
            baseline += c;
            improved += usize::from(b < c);
            regressed += usize::from(b > c);
            adaptive_harm += usize::from(b > a);
            baseline_harm += usize::from(c > a);
        }
    }
    json!({"attempted":rows.len(),"scored":scored,"rawWordEdits":raw,"adaptiveWordEdits":adaptive,
        "baselineWordEdits":baseline,"improved":improved,"regressed":regressed,"failureRows":failure_rows,
        "adaptiveHarmfulComparedWithRaw":adaptive_harm,"baselineHarmfulComparedWithRaw":baseline_harm})
}
/// Fixed-policy trajectories are post-hoc comparators, never controller input.
fn fixed_comparators(manifest: &Manifest) -> Result<Vec<Value>> {
    let mut results = vec![];
    for support_floor in 1..=3 {
        for context_overlap in 0..=1 {
            let policy = Policy {
                support_floor,
                context_overlap,
            };
            let mut learner = Learner::new()?;
            let mut rows = vec![];
            let mut groups = vec![];
            for r in &manifest.rounds {
                groups.extend([
                    (&r.learn, false),
                    (&r.develop, false),
                    (&r.validate, false),
                    (&r.audit, true),
                ]);
            }
            groups.push((&manifest.final_test, true));
            for (episodes, heldout) in groups {
                for e in episodes {
                    let mut failures = vec![];
                    let actual =
                        prediction(&learner.snapshot()?, &e.input, policy).unwrap_or_else(|err| {
                            failures.push(err.to_string());
                            e.input.text.clone()
                        });
                    if let Some(edit) = &e.edit {
                        if let Err(err) = learner.feedback(&e.input, &actual, edit) {
                            failures.push(err.to_string());
                        }
                    }
                    if heldout {
                        let errors = word_edits(&e.reference, &actual);
                        let raw = word_edits(&e.reference, &e.input.text);
                        if errors.is_none() || raw.is_none() {
                            failures.push("Score budget exceeded".into());
                        }
                        rows.push(json!({"rawErrors":raw,"adaptiveErrors":errors,"baselineErrors":raw,"failures":failures}));
                    } else if !failures.is_empty() {
                        return Err(format!(
                            "Fixed comparator {policy:?} failed before heldout: {}",
                            failures.join("; ")
                        )
                        .into());
                    }
                }
            }
            let counts = aggregate(&rows);
            results.push(json!({"policy":policy,"heldout":{"attempted":counts["attempted"],"scored":counts["scored"],
                "wordEdits":counts["adaptiveWordEdits"],"harmfulComparedWithRaw":counts["adaptiveHarmfulComparedWithRaw"],
                "failureRows":counts["failureRows"]}}));
        }
    }
    Ok(results)
}

fn replay(manifest: &Manifest) -> Result<Value> {
    replay_variant(manifest, false)
}
fn replay_variant(manifest: &Manifest, use_failure_bank: bool) -> Result<Value> {
    replay_mode(
        manifest,
        if use_failure_bank {
            Mode::FailureBank
        } else {
            Mode::Current
        },
    )
}
#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Current,
    FailureBank,
    Strong,
    Scoped,
}
impl Mode {
    fn name(self) -> &'static str {
        match self {
            Self::Current => "current",
            Self::FailureBank => "failure_bank",
            Self::Strong => "strong_global",
            Self::Scoped => "scoped",
        }
    }
}
fn replay_mode(manifest: &Manifest, mode: Mode) -> Result<Value> {
    let use_failure_bank = mode == Mode::FailureBank;
    let started = Instant::now();
    manifest.validate()?;
    let mut adaptive = Learner::new()?;
    let mut baseline = Learner::new()?;
    let mut controller = Controller::new();
    if matches!(mode, Mode::Strong | Mode::Scoped) {
        controller.active = Policy::context_default();
    }
    let initial_policy = controller.active;
    let mut scopes = scoped::Scoped::default();
    let mut rows = vec![];
    let mut times = Timings::default();
    let mut bank = FailureBank::default();
    let mut bank_rounds = vec![];
    for r in &manifest.rounds {
        let observed_policy = controller.active;
        execute(
            &r.learn,
            "learn",
            &r.id,
            (&mut adaptive, &mut baseline),
            if mode == Mode::Scoped {
                scoped::Route::Scoped(&scopes)
            } else {
                scoped::Route::Global(controller.active)
            },
            &mut rows,
            &mut times,
        )?;
        let development = execute(
            &r.develop,
            "develop",
            &r.id,
            (&mut adaptive, &mut baseline),
            if mode == Mode::Scoped {
                scoped::Route::Scoped(&scopes)
            } else {
                scoped::Route::Global(controller.active)
            },
            &mut rows,
            &mut times,
        )?;
        // Candidate is irrevocably selected before the fresh validation feedback is seen.
        let mut scoped_candidates = BTreeMap::new();
        let candidate = timed(&mut times.select, || {
            if mode == Mode::Scoped {
                scoped_candidates = scopes.propose(&development);
                None
            } else if use_failure_bank {
                proposal_with_history(&development, controller.active, &bank.cases)
            } else {
                proposal(&development, controller.active)
            }
        });
        let validation = execute(
            &r.validate,
            "validate",
            &r.id,
            (&mut adaptive, &mut baseline),
            if mode == Mode::Scoped {
                scoped::Route::Scoped(&scopes)
            } else {
                scoped::Route::Global(controller.active)
            },
            &mut rows,
            &mut times,
        )?;
        timed(&mut times.gate, || {
            if mode == Mode::Scoped {
                scopes.finish(&r.id, &scoped_candidates, &development, &validation);
            } else if use_failure_bank {
                controller.finish_with_history(
                    &r.id,
                    candidate,
                    &development,
                    &validation,
                    &bank.cases,
                )
            } else {
                controller.finish(&r.id, candidate, &development, &validation)
            }
        });
        if use_failure_bank {
            timed(&mut times.bank_update, || {
                bank.observe(&development, observed_policy);
                bank.observe(&validation, observed_policy);
            });
            bank_rounds.push(json!({"round":r.id,"retained":bank.cases.len(),
                "admitted":bank.admitted,"evicted":bank.evicted,"skippedFailures":bank.skipped_failures}));
        }
        execute(
            &r.audit,
            "audit",
            &r.id,
            (&mut adaptive, &mut baseline),
            if mode == Mode::Scoped {
                scoped::Route::Scoped(&scopes)
            } else {
                scoped::Route::Global(controller.active)
            },
            &mut rows,
            &mut times,
        )?;
    }
    execute(
        &manifest.final_test,
        "final_test",
        "final",
        (&mut adaptive, &mut baseline),
        if mode == Mode::Scoped {
            scoped::Route::Scoped(&scopes)
        } else {
            scoped::Route::Global(controller.active)
        },
        &mut rows,
        &mut times,
    )?;
    let heldout: Vec<_> = rows
        .iter()
        .filter(|r| r["phase"] == "audit" || r["phase"] == "final_test")
        .cloned()
        .collect();
    let final_rows: Vec<_> = rows
        .iter()
        .filter(|r| r["phase"] == "final_test")
        .cloned()
        .collect();
    let adaptive_and_baseline_ms = started.elapsed().as_secs_f64() * 1000.0;
    let fixed = fixed_comparators(manifest)?;
    Ok(
        json!({"schemaVersion":1,"experimentVersion":4,"kind":"offline bounded eligibility-policy search; no live integration or open-ended algorithm discovery",
        "controllerVariant":mode.name(),"scopedPolicies":scopes.stats(),
        "failureBank":{"enabled":use_failure_bank,"limit":FAILURE_BANK_LIMIT,"retained":bank.cases.len(),
            "admitted":bank.admitted,"evicted":bank.evicted,"skippedFailures":bank.skipped_failures,"rounds":bank_rounds},
        "adaptiveAndBaselineReplayMs":adaptive_and_baseline_ms,
        "totalReplayMs":started.elapsed().as_secs_f64()*1000.0,
        "source":manifest.source,"policyFamily":{"supportFloor":[1,2,3],"contextOverlap":[0,1],"proposal":"one-coordinate neighbors only"},
        "gates":{"minimumIndependentFeedback":MIN_FEEDBACK,"minimumImproved":MIN_IMPROVED,"maximumRegressed":0,"strictWordEditReduction":true},
        "initialPolicy":initial_policy,"finalPolicy":controller.active,
        "decisions":if mode == Mode::Scoped { json!(scopes.decisions) } else { json!(controller.decisions) },
        "adaptiveMemoryCount":adaptive.store.personalization()?.vocabulary.len(),
        "baselineMemoryCount":baseline.store.personalization()?.vocabulary.len(),
        "adaptiveRetained":adaptive.retained_counts()?,"baselineRetained":baseline.retained_counts()?,
        "allEpisodes":aggregate(&rows),"heldout":aggregate(&heldout),"finalTest":aggregate(&final_rows),
        "timing":{"application":timing(&times.apply),"feedback":timing(&times.feedback),"proposal":timing(&times.select),"validationAndRollback":timing(&times.gate),"failureBankUpdate":timing(&times.bank_update)},
        "timingDefinitions":{"application":"clone/filter existing profile and guarded application; excludes SQLite snapshot read","feedback":"existing open SQLite learner transaction plus metadata update; excludes connection setup","proposal":"bounded neighbor comparison on pre-event snapshots","validationAndRollback":"fresh and optional historical feedback comparison, rollback and policy decision; in-memory","failureBankUpdate":"score observed development/validation feedback on pre-event snapshots, clone failed cases and evict oldest whole components; excludes hidden labels","adaptiveAndBaselineReplayMs":"setup, SQLite snapshots, adaptive plus independent baseline replay, reporting rows and controller/bank work; excludes six fixed comparators and JSON file serialization","totalReplayMs":"adaptiveAndBaselineReplayMs plus six fixed comparator trajectories; excludes JSON file serialization"},
        "fixedPolicyComparators":fixed,
        "fixedComparatorLimit":"Post-hoc six-policy comparison; the best fixed policy is not controller input.",
        "rows":rows}),
    )
}
fn main() -> Result<()> {
    let args: Vec<_> = env::args().skip(1).collect();
    if args.len() != 2
        && !(args.len() == 3
            && ["--failure-bank", "--strong-global", "--scoped"].contains(&args[2].as_str()))
    {
        return Err(
            "usage: policy_learning_replay MANIFEST_JSON RESULTS_JSON [--failure-bank|--strong-global|--scoped]".into(),
        );
    }
    let bytes = fs::read(&args[0])?;
    let manifest: Manifest = serde_json::from_slice(&bytes)?;
    let mut result = match args.get(2).map(String::as_str) {
        Some("--scoped") => replay_mode(&manifest, Mode::Scoped)?,
        Some("--strong-global") => replay_mode(&manifest, Mode::Strong)?,
        Some("--failure-bank") => replay_variant(&manifest, true)?,
        _ => replay(&manifest)?,
    };
    result["manifestSha256"] = json!(format!("{:x}", Sha256::digest(bytes)));
    fs::write(&args[1], serde_json::to_vec_pretty(&result)?)?;
    println!(
        "{}",
        json!({"heldout":result["heldout"],"decisions":result["decisions"].as_array().unwrap().iter().map(|d|json!({"round":d["round"],"action":d["action"],"active":d["active"]})).collect::<Vec<_>>()})
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use vesper_core::Language;
    fn fixture() -> Manifest {
        serde_json::from_str(include_str!(
            "../../../benchmarks/asr/fixtures/policy-learning-episodes.json"
        ))
        .unwrap()
    }
    fn input(id: &str, text: &str) -> Input {
        Input {
            id: id.into(),
            source_key: format!("source-{id}"),
            episode_id: format!("episode-{id}"),
            at_ms: 1,
            project: "workspace".into(),
            language: Language::English,
            model_id: "model-a".into(),
            text: text.into(),
        }
    }
    fn learned() -> Learner {
        let mut learner = Learner::new().unwrap();
        let i = input("teach", "Open readme for onboarding");
        learner
            .feedback(&i, &i.text, "Open README.md for onboarding")
            .unwrap();
        learner
    }
    fn cases() -> Vec<FeedbackCase> {
        let learner = learned();
        (0..4)
            .map(|n| {
                let i = input(&format!("case-{n}"), "Discuss readme in linguistics");
                FeedbackCase {
                    snapshot: learner.snapshot().unwrap(),
                    desired: i.text.clone(),
                    input: i,
                    failure: None,
                }
            })
            .collect()
    }
    fn scoped_cases(topic: &str, prefix: &str) -> Vec<FeedbackCase> {
        let mut snapshot = learned().snapshot().unwrap();
        for context in snapshot.teaching_terms.values_mut() {
            *context = BTreeSet::from([topic.to_owned()]);
        }
        (0..4)
            .map(|n| {
                let input = input(
                    &format!("{prefix}-{n}"),
                    &format!("Review readme during {topic}"),
                );
                FeedbackCase {
                    snapshot: snapshot.clone(),
                    desired: input.text.clone(),
                    input,
                    failure: None,
                }
            })
            .collect()
    }
    #[test]
    fn scoped_update_changes_only_its_context_and_can_later_roll_back() {
        let development = scoped_cases("onboarding", "develop");
        let validation = scoped_cases("onboarding", "validate");
        let mut scopes = scoped::Scoped::default();
        let proposals = scopes.propose(&development);
        assert_eq!(proposals.len(), 1);
        scopes.finish("negative", &proposals, &development, &validation);
        let query = &development[0];
        let (policy, _) = scoped::Route::Scoped(&scopes).select(&query.snapshot, &query.input);
        assert_eq!(
            policy,
            Policy {
                support_floor: 2,
                context_overlap: 1
            }
        );
        let other = &scoped_cases("rendering", "other")[0];
        assert_eq!(
            scoped::Route::Scoped(&scopes)
                .select(&other.snapshot, &other.input)
                .0,
            Policy::context_default()
        );
        let mut positive = scoped_cases("onboarding", "changed-preference");
        for c in &mut positive {
            c.desired = c.input.text.replace("readme", "README.md");
        }
        let proposals = scopes.propose(&positive);
        scopes.finish("positive", &proposals, &positive, &positive);
        assert_eq!(scopes.decisions.last().unwrap()["action"], "rolled_back");
        assert_eq!(
            scoped::Route::Scoped(&scopes)
                .select(&query.snapshot, &query.input)
                .0,
            Policy::context_default()
        );
    }
    #[test]
    fn routing_uses_only_unambiguous_observable_context_and_scope() {
        let mut case = scoped_cases("onboarding", "route").remove(0);
        let original = scoped::key(&case.snapshot, &case.input).unwrap();
        case.input.model_id = "different-model".into();
        assert_eq!(scoped::key(&case.snapshot, &case.input), Some(original));
        for text in [
            "Review readme",
            "Keep 'readme' during onboarding",
            "Review an unknown term during onboarding",
        ] {
            let mut input = case.input.clone();
            input.text = text.into();
            assert!(scoped::key(&case.snapshot, &input).is_none());
        }
        let mut input = case.input.clone();
        input.project = "another-project".into();
        assert!(scoped::key(&case.snapshot, &input).is_none());
        input = case.input.clone();
        input.language = Language::Hindi;
        assert!(scoped::key(&case.snapshot, &input).is_none());
        for terms in case.snapshot.teaching_terms.values_mut() {
            terms.insert("rendering".into());
        }
        case.input.text.push_str(" rendering");
        assert!(scoped::key(&case.snapshot, &case.input).is_none());
        case.input.text = "Review readme during onboarding".into();
        let duplicate = case.snapshot.profile.vocabulary[0].clone();
        case.snapshot.profile.vocabulary.push(duplicate);
        assert!(scoped::key(&case.snapshot, &case.input).is_none());
    }
    #[test]
    fn correlated_or_ambiguous_components_do_not_become_scoped_votes() {
        let mut cases = scoped_cases("onboarding", "linked");
        for case in &mut cases {
            case.input.source_key = "one-source".into();
        }
        let scopes = scoped::Scoped::default();
        assert!(scopes.propose(&cases).is_empty());
        let mut independent = scoped_cases("onboarding", "mixed");
        let mut adverse = independent[0].clone();
        adverse.input.text = "Review readme during unknown-context".into();
        independent.push(adverse);
        // One ambiguous connected component leaves only three eligible independent cases.
        assert!(scopes.propose(&independent).is_empty());
    }
    #[test]
    fn scoped_policy_capacity_evicts_oldest_scope_and_restores_fallback() {
        let mut scopes = scoped::Scoped::default();
        for i in 0..=scoped::MAX_SCOPES {
            let cases = scoped_cases(&format!("topic{i}"), &format!("recording{i}"));
            scopes.finish(
                &format!("round{i}"),
                &scopes.propose(&cases),
                &cases,
                &cases,
            );
        }
        assert_eq!(scopes.stats()["retained"], scoped::MAX_SCOPES);
        assert_eq!(scopes.stats()["evicted"], 1);
        let first = scoped_cases("topic0", "first").remove(0);
        assert_eq!(
            scoped::Route::Scoped(&scopes)
                .select(&first.snapshot, &first.input)
                .0,
            Policy::context_default()
        );
    }
    #[test]
    fn scoped_flow_is_independent_of_hidden_labels_and_model_identity() {
        let a = fixture();
        let mut b = a.clone();
        for e in b
            .rounds
            .iter_mut()
            .flat_map(|r| {
                r.learn
                    .iter_mut()
                    .chain(&mut r.develop)
                    .chain(&mut r.validate)
                    .chain(&mut r.audit)
            })
            .chain(&mut b.final_test)
        {
            e.reference = "hidden ".repeat(513);
            e.input.model_id = "changed-model-metadata".into();
        }
        b.source = json!({"scenario":"untrusted", "expectedPolicy":"ignore"});
        let x = replay_mode(&a, Mode::Scoped).unwrap();
        let y = replay_mode(&b, Mode::Scoped).unwrap();
        assert_eq!(x["decisions"], y["decisions"]);
        assert_eq!(x["scopedPolicies"], y["scopedPolicies"]);
        for (a, b) in x["rows"]
            .as_array()
            .unwrap()
            .iter()
            .zip(y["rows"].as_array().unwrap())
        {
            assert_eq!(a["actual"], b["actual"]);
            assert_eq!(a.get("scope"), b.get("scope"));
        }
        assert_eq!(y["allEpisodes"]["scored"], 0);
    }
    #[test]
    fn bank_admits_only_observable_scorable_failures() {
        let mut feedback = cases();
        feedback[1].desired = "Discuss README.md in linguistics".into();
        feedback[2].failure = Some("injected transaction failure".into());
        feedback[3].desired = "word ".repeat(513);
        let mut bank = FailureBank::default();
        bank.observe(&feedback, Policy::baseline());
        assert_eq!(bank.cases.len(), 1);
        assert_eq!(bank.cases[0].input.id, "case-0");
        assert_eq!(bank.skipped_failures, 2);
    }
    #[test]
    fn historical_failures_veto_rollback_without_manufacturing_fresh_votes() {
        let history = cases();
        let active = Policy {
            support_floor: 1,
            context_overlap: 1,
        };
        let mut fresh = cases();
        for case in &mut fresh {
            case.desired = "Discuss README.md in linguistics".into();
        }
        let mut controller = Controller::new();
        controller.active = active;
        controller.parent = Some(Policy::baseline());
        controller.finish_with_history(
            "guarded",
            Some(Policy::baseline()),
            &fresh,
            &fresh,
            &history,
        );
        assert_eq!(controller.active, active);
        assert_eq!(controller.decisions[0].action, "rejected");
        assert_eq!(
            controller.decisions[0]
                .historical_rollback
                .as_ref()
                .unwrap()
                .regressed,
            4
        );
        controller.finish("original", Some(Policy::baseline()), &fresh, &fresh);
        assert_eq!(controller.active, Policy::baseline());
        assert_eq!(controller.decisions[1].action, "rolled_back");
        assert!(proposal_with_history(&history[..1], Policy::baseline(), &history).is_none());
    }
    #[test]
    fn bank_evicts_whole_oldest_correlated_components_at_capacity() {
        let original = cases()[0].clone();
        let feedback: Vec<_> = (0_usize..65)
            .map(|i| {
                let mut case = original.clone();
                case.input.id = format!("capacity-{i}");
                case.input.source_key = format!("source-{}", i.saturating_sub(1));
                case.input.episode_id = format!("episode-{i}");
                case
            })
            .collect();
        let mut bank = FailureBank::default();
        bank.observe(&feedback, Policy::baseline());
        assert_eq!((bank.cases.len(), bank.evicted, bank.admitted), (63, 2, 65));
        assert_eq!(bank.cases[0].input.id, "capacity-2");
        let mut correlated = feedback;
        for case in &mut correlated {
            case.input.source_key = "one-source".into();
        }
        let mut bank = FailureBank::default();
        bank.observe(&correlated, Policy::baseline());
        assert!(bank.cases.is_empty());
        assert_eq!(bank.evicted, 65);
    }
    #[test]
    fn bank_blocks_the_known_rollback_but_does_not_see_hidden_labels() {
        let a = fixture();
        let mut b = a.clone();
        for episode in b
            .rounds
            .iter_mut()
            .flat_map(|r| {
                r.learn
                    .iter_mut()
                    .chain(&mut r.develop)
                    .chain(&mut r.validate)
                    .chain(&mut r.audit)
            })
            .chain(&mut b.final_test)
        {
            episode.reference = "hidden ".repeat(513);
            episode.input.model_id = "different-model-provenance".into();
        }
        b.source = json!({"expectedDecisionSequence":["untrusted"]});
        let x = replay_variant(&a, true).unwrap();
        let y = replay_variant(&b, true).unwrap();
        assert_eq!(x["decisions"], y["decisions"]);
        assert_eq!(x["failureBank"], y["failureBank"]);
        assert_eq!(
            x["decisions"][2]["active"],
            json!({"supportFloor":2,"contextOverlap":1})
        );
        assert_ne!(x["decisions"][2]["action"], "rolled_back");
        for (left, right) in x["rows"]
            .as_array()
            .unwrap()
            .iter()
            .zip(y["rows"].as_array().unwrap())
        {
            assert_eq!(left["actual"], right["actual"]);
        }
        assert_eq!(y["allEpisodes"]["scored"], 0);
        assert_eq!(y["allEpisodes"]["failureRows"], 66);
    }
    #[test]
    fn locked_sequence_changes_policy_twice_and_rolls_back_on_fresh_feedback() {
        let result = replay(&fixture()).unwrap();
        let actions: Vec<_> = result["decisions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| d["action"].as_str().unwrap())
            .collect();
        assert_eq!(actions, ["promoted", "promoted", "rolled_back"]);
        assert_eq!(
            result["decisions"][0]["active"],
            json!({"supportFloor":1,"contextOverlap":1})
        );
        assert_eq!(
            result["decisions"][1]["active"],
            json!({"supportFloor":2,"contextOverlap":1})
        );
        assert_eq!(
            result["finalPolicy"],
            json!({"supportFloor":1,"contextOverlap":1})
        );
        assert_eq!(result["allEpisodes"]["failureRows"], 0);
    }
    #[test]
    fn hidden_labels_and_expected_metadata_cannot_change_any_decision() {
        let a = fixture();
        let mut b = a.clone();
        b.source = json!({"expectedDecisionSequence":["never","follow","this"]});
        for r in &mut b.rounds {
            for e in r
                .learn
                .iter_mut()
                .chain(&mut r.develop)
                .chain(&mut r.validate)
                .chain(&mut r.audit)
            {
                e.reference = "An unrelated hidden scoring target".into();
            }
        }
        for e in &mut b.final_test {
            e.reference = "Another independently hidden target".into();
        }
        let x = replay(&a).unwrap();
        let y = replay(&b).unwrap();
        assert_eq!(x["decisions"], y["decisions"]);
        assert_eq!(x["finalPolicy"], y["finalPolicy"]);
        assert_ne!(x["heldout"], y["heldout"]);
    }
    #[test]
    fn future_regression_rejects_a_development_winner() {
        let development = cases();
        let active = Policy::baseline();
        let candidate = proposal(&development, active).unwrap();
        let mut validation = development.clone();
        for c in &mut validation {
            c.desired = "Discuss README.md in linguistics".into();
        }
        let mut controller = Controller::new();
        controller.finish("fresh", Some(candidate), &development, &validation);
        assert_eq!(controller.active, active);
        assert_eq!(controller.decisions[0].action, "rejected");
        assert_eq!(
            controller.decisions[0]
                .validation
                .as_ref()
                .unwrap()
                .regressed,
            4
        );
    }
    #[test]
    fn duplicate_feedback_cannot_manufacture_a_policy_gate() {
        let mut feedback = cases();
        let c = Policy {
            support_floor: 1,
            context_overlap: 1,
        };
        assert!(passes(&compare(&feedback, Policy::baseline(), c)));
        for f in &mut feedback {
            f.input.source_key = "one-recording".into();
        }
        let score = compare(&feedback, Policy::baseline(), c);
        assert_eq!(score.independent_cases, 1);
        assert!(!passes(&score));
        for (n, f) in feedback.iter_mut().enumerate() {
            f.input.source_key = format!("different-{n}");
            f.input.episode_id = "one-repair-chain".into();
        }
        assert_eq!(
            compare(&feedback, Policy::baseline(), c).independent_cases,
            1
        );
    }
    #[test]
    fn correlated_later_regressions_and_overflow_cannot_hide_behind_first_case() {
        let mut feedback = cases();
        let candidate = Policy {
            support_floor: 1,
            context_overlap: 1,
        };
        let mut later = feedback[0].clone();
        later.input.id = "later-correlated-correction".into();
        later.desired = "Discuss README.md in linguistics".into();
        feedback.push(later);
        let score = compare(&feedback, Policy::baseline(), candidate);
        assert_eq!(score.independent_cases, 4);
        assert_eq!(
            score.regressed, 1,
            "a correlated regression still vetoes promotion"
        );
        assert!(!passes(&score));
        feedback.last_mut().unwrap().desired = "word ".repeat(513);
        let score = compare(&feedback, Policy::baseline(), candidate);
        assert_eq!(
            score.failures.len(),
            1,
            "all correlated cases must be evaluated"
        );
        assert!(!passes(&score));
    }
    #[test]
    fn correlated_comparison_is_order_invariant_and_repeats_do_not_add_weight() {
        let candidate = Policy {
            support_floor: 1,
            context_overlap: 1,
        };
        let mut feedback = cases();
        let original = compare(&feedback, Policy::baseline(), candidate);
        feedback.extend(vec![feedback[0].clone(); 20]);
        let repeated = compare(&feedback, Policy::baseline(), candidate);
        assert_eq!(original.incumbent_errors, repeated.incumbent_errors);
        assert_eq!(original.improved, repeated.improved);
        feedback.last_mut().unwrap().desired = "Discuss README.md in linguistics".into();
        let forward = compare(&feedback, Policy::baseline(), candidate);
        feedback.reverse();
        let reverse = compare(&feedback, Policy::baseline(), candidate);
        assert_eq!(
            forward, reverse,
            "group order cannot hide contradictory feedback"
        );
    }
    #[test]
    fn transitive_source_and_episode_links_count_as_one_independent_case() {
        let mut feedback = cases();
        feedback.truncate(3);
        feedback[1].input.source_key = feedback[0].input.source_key.clone();
        feedback[2].input.episode_id = feedback[1].input.episode_id.clone();
        assert_eq!(
            compare(
                &feedback,
                Policy::baseline(),
                Policy {
                    support_floor: 1,
                    context_overlap: 1
                }
            )
            .independent_cases,
            1
        );
    }
    #[test]
    fn duplicate_edit_bridges_cannot_satisfy_the_experimental_support_floor() {
        let mut learner = learned();
        let mut bridge = input("bridge", "Open readme for onboarding");
        bridge.source_key = "source-teach".into();
        learner
            .feedback(&bridge, &bridge.text, "Open README.md for onboarding")
            .unwrap();
        let mut related = input("related", "Open readme for onboarding");
        related.episode_id = bridge.episode_id.clone();
        learner
            .feedback(&related, &related.text, "Open README.md for onboarding")
            .unwrap();
        assert_eq!(
            prediction(
                &learner.snapshot().unwrap(),
                &related,
                Policy {
                    support_floor: 2,
                    context_overlap: 0
                }
            )
            .unwrap(),
            related.text
        );
    }
    #[test]
    fn missing_feedback_failure_and_score_overflow_keep_the_incumbent() {
        assert!(proposal(&[], Policy::baseline()).is_none());
        let mut feedback = cases();
        feedback[0].failure = Some("injected SQLite failure".into());
        assert!(proposal(&feedback, Policy::baseline()).is_none());
        feedback[0].failure = None;
        feedback[0].desired = "word ".repeat(513);
        let c = compare(
            &feedback,
            Policy::baseline(),
            Policy {
                support_floor: 1,
                context_overlap: 1,
            },
        );
        assert_eq!(c.failures.len(), 1);
        assert!(!passes(&c));
    }
    #[test]
    fn stale_or_forged_non_neighbor_proposals_cannot_promote() {
        let feedback = cases();
        let mut controller = Controller::new();
        controller.finish(
            "forged",
            Some(Policy {
                support_floor: 3,
                context_overlap: 1,
            }),
            &feedback,
            &feedback,
        );
        assert_eq!(controller.active, Policy::baseline());
        assert_eq!(controller.decisions[0].action, "rejected");
        assert!(prediction(&Snapshot::default(), &input("a", "hi"), Policy::default()).is_err());
    }
    #[test]
    fn actual_learner_deduplicates_support_and_policy_cannot_reactivate_undo() {
        let mut learner = learned();
        let policy = Policy {
            support_floor: 2,
            context_overlap: 0,
        };
        let mut again = input("teach-again", "Open readme for onboarding");
        again.source_key = "source-teach".into();
        learner
            .feedback(&again, &again.text, "Open README.md for onboarding")
            .unwrap();
        assert_eq!(
            prediction(&learner.snapshot().unwrap(), &again, policy).unwrap(),
            again.text
        );
        let independent = input("independent", "Open readme for onboarding");
        learner
            .feedback(
                &independent,
                &independent.text,
                "Open README.md for onboarding",
            )
            .unwrap();
        assert_eq!(
            prediction(&learner.snapshot().unwrap(), &independent, policy).unwrap(),
            "Open README.md for onboarding"
        );
        let undo = input("undo", "Open readme for onboarding");
        learner
            .feedback(&undo, "Open README.md for onboarding", &undo.text)
            .unwrap();
        for s in 1..=3 {
            for c in 0..=1 {
                assert_eq!(
                    prediction(
                        &learner.snapshot().unwrap(),
                        &undo,
                        Policy {
                            support_floor: s,
                            context_overlap: c
                        }
                    )
                    .unwrap(),
                    undo.text
                );
            }
        }
    }
    #[test]
    fn partitions_reject_hidden_feedback_duplicate_audio_and_reversed_time() {
        let mut m = fixture();
        m.final_test[0].edit = Some("hidden target".into());
        assert!(m.validate().is_err());
        let mut m = fixture();
        m.rounds[0].validate[0].input.source_key = m.rounds[0].learn[0].input.source_key.clone();
        assert!(m.validate().is_err());
        let mut m = fixture();
        m.rounds[0].audit[0].input.episode_id = m.rounds[0].develop[0].input.episode_id.clone();
        assert!(m.validate().is_err());
        let mut m = fixture();
        m.final_test[0].input.at_ms = 0;
        assert!(m.validate().is_err());
    }
    #[test]
    fn application_keeps_quotes_scope_and_language_guards_across_models() {
        let learner = learned();
        let s = learner.snapshot().unwrap();
        for text in [
            "Keep 'readme' during onboarding",
            "Do not send 15 files Friday",
        ] {
            let i = input("protected", text);
            assert_eq!(prediction(&s, &i, Policy::baseline()).unwrap(), text);
        }
        let mut i = input("other", "Open readme for onboarding");
        i.model_id = "another-model".into();
        assert_eq!(
            prediction(&s, &i, Policy::baseline()).unwrap(),
            "Open README.md for onboarding"
        );
        i.project = "another-project".into();
        assert_eq!(prediction(&s, &i, Policy::baseline()).unwrap(), i.text);
        i.project = "workspace".into();
        i.language = Language::Hindi;
        assert_eq!(prediction(&s, &i, Policy::baseline()).unwrap(), i.text);
    }
    #[test]
    fn grouping_matches_graph_reachability_for_512_seeded_link_graphs() {
        for seed in 0..512_u64 {
            let mut rng = seed + 1;
            let mut next = || {
                rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1);
                (rng >> 32) as usize % 7
            };
            let pairs: Vec<_> = (0..12)
                .map(|_| (format!("s{}", next()), format!("e{}", next())))
                .collect();
            let borrowed: Vec<_> = pairs
                .iter()
                .map(|(s, e)| (s.as_str(), e.as_str()))
                .collect();
            let actual = components(&borrowed);
            // Independent graph closure oracle, not another union-find.
            let mut reachable = [[false; 12]; 12];
            for i in 0..12 {
                for j in 0..12 {
                    reachable[i][j] = pairs[i].0 == pairs[j].0 || pairs[i].1 == pairs[j].1;
                }
            }
            for k in 0..12 {
                for i in 0..12 {
                    for j in 0..12 {
                        reachable[i][j] |= reachable[i][k] && reachable[k][j];
                    }
                }
            }
            for i in 0..12 {
                for j in 0..12 {
                    assert_eq!(actual[i] == actual[j], reachable[i][j], "seed {seed}");
                }
            }
        }
    }
    #[test]
    fn repeated_improvements_cannot_supply_two_independent_improvement_votes() {
        let mut feedback = cases();
        feedback[1].input.source_key = feedback[0].input.source_key.clone();
        for f in &mut feedback[2..] {
            f.input.text = "Keep these words unchanged".into();
            f.desired = f.input.text.clone();
        }
        let mut neutral = feedback[2].clone();
        neutral.input = input("extra-independent", &neutral.desired);
        feedback.push(neutral);
        let score = compare(
            &feedback,
            Policy::baseline(),
            Policy {
                support_floor: 1,
                context_overlap: 1,
            },
        );
        assert_eq!(score.independent_cases, 4);
        assert_eq!(score.improved, 1);
        assert!(!passes(&score));
    }
    #[test]
    fn failures_at_any_position_including_duplicates_veto_promotion_and_rollback() {
        let active = Policy {
            support_floor: 1,
            context_overlap: 1,
        };
        for position in 0..8 {
            let mut feedback = cases();
            feedback.extend(feedback.clone());
            feedback[position].failure = Some("injected feedback storage failure".into());
            assert!(proposal(&feedback, Policy::baseline()).is_none());
            for f in &mut feedback {
                f.desired = "Discuss README.md in linguistics".into();
            }
            let mut controller = Controller {
                active,
                parent: Some(Policy::baseline()),
                decisions: vec![],
            };
            controller.finish("failure", None, &[], &feedback);
            assert_eq!(controller.active, active);
        }
    }
    #[test]
    fn isolated_users_forgetting_and_all_policy_combinations_preserve_guards() {
        let learner = learned();
        let other = Learner::new().unwrap();
        let s = learner.snapshot().unwrap();
        let serialized = serde_json::to_value(&s.profile).unwrap();
        for support_floor in 1..=3 {
            for context_overlap in 0..=1 {
                let policy = Policy {
                    support_floor,
                    context_overlap,
                };
                let query = input("query", "Open readme for onboarding");
                assert_eq!(
                    prediction(&other.snapshot().unwrap(), &query, policy).unwrap(),
                    query.text
                );
                for language in [
                    Language::English,
                    Language::Hindi,
                    Language::French,
                    Language::Auto,
                ] {
                    for text in [
                        "Keep 'readme' during onboarding",
                        "Keep \"readme\" during onboarding",
                        "Keep `readme` during onboarding",
                        "Do not send 15 files Friday",
                        "कृपया कल बैठक करें",
                    ] {
                        let mut query = input("guard", text);
                        query.language = language;
                        assert_eq!(prediction(&s, &query, policy).unwrap(), text);
                    }
                }
            }
        }
        assert_eq!(
            serialized,
            serde_json::to_value(&s.profile).unwrap(),
            "predictions do not mutate shared snapshots"
        );
        learner
            .store
            .forget_automatic_learning(s.profile.revision)
            .unwrap();
        let forgotten = learner.snapshot().unwrap();
        assert!(forgotten.profile.vocabulary.is_empty());
        let query = input("after-forget", "Open readme for onboarding");
        assert_eq!(
            prediction(&forgotten, &query, Policy::baseline()).unwrap(),
            query.text
        );
        assert!(other.snapshot().unwrap().evidence.candidates.is_empty());
    }
    #[test]
    fn manifest_budgets_accept_boundaries_and_reject_the_next_item() {
        let sample = fixture().final_test[0].clone();
        let make = |count: usize| -> Vec<Episode> {
            (0..count)
                .map(|n| {
                    let mut e = sample.clone();
                    e.input = input(&format!("limit-{n}"), "A bounded input");
                    e.input.at_ms = n as u64 + 1;
                    e
                })
                .collect()
        };
        let mut m = fixture();
        m.rounds.truncate(1);
        m.rounds[0].learn.clear();
        m.rounds[0].develop.clear();
        m.rounds[0].validate.clear();
        m.rounds[0].audit.clear();
        m.final_test = make(2048);
        assert!(m.validate().is_ok());
        m.final_test = make(2049);
        assert!(m.validate().is_err());
        m.final_test.clear();
        m.rounds[0].develop = make(64);
        assert!(m.validate().is_ok());
        m.rounds[0].develop = make(65);
        assert!(m.validate().is_err());
        m.rounds[0].develop.clear();
        let template = m.rounds[0].clone();
        m.rounds = (0..16)
            .map(|n| {
                let mut r = template.clone();
                r.id = n.to_string();
                r
            })
            .collect();
        assert!(m.validate().is_ok());
        m.rounds.push(template);
        assert!(m.validate().is_err());
    }
    #[test]
    fn disabled_entries_do_not_require_evicted_candidate_provenance() {
        let mut snapshot = learned().snapshot().unwrap();
        snapshot.profile.vocabulary[0].enabled = false;
        // Non-active candidates may be evicted at the retained-evidence bound.
        snapshot.evidence.candidates.clear();
        let query = input("disabled", "Open readme for onboarding");
        assert_eq!(
            prediction(&snapshot, &query, Policy::baseline()).unwrap(),
            query.text
        );
        snapshot.profile.vocabulary[0].enabled = true;
        assert!(
            prediction(&snapshot, &query, Policy::baseline()).is_err(),
            "enabled entries still need provenance"
        );
    }
}
