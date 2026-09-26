//! Diagnostic qualification, not natural-user or microphone/WebView evidence.
use super::*;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, fs, io::Write, time::Instant};
use vesper_core::{word_edits, AsrModel};
use vesper_data::DictationPersonalizationSnapshot;

const AUTHORED: &str =
    include_str!("../../../../benchmarks/asr/fixtures/native-learning-sessions.json");
const PUBLIC: &str =
    include_str!("../../../../benchmarks/asr/manifests/native-learning-2026-09-19.json");

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Episode {
    id: String,
    session: u32,
    phase: String,
    kind: String,
    source: u16,
    project: String,
    language: Language,
    model: String,
    raw: String,
    reference: String,
    edited: Option<String>,
    #[serde(default)]
    undo: bool,
    #[serde(default)]
    forget: bool,
}

fn final_text(result: &PipelineResult) -> &str {
    result
        .draft_transcript
        .as_deref()
        .or(result.personalized_transcript.as_deref())
        .or(result.cleaned_transcript.as_deref())
        .unwrap_or(&result.raw_transcript)
}

fn context(
    root: &Path,
    runtime: &Arc<Runtime>,
    inference: &Arc<vesper_inference::InferenceManager>,
    operation: &str,
) -> WorkContext {
    WorkContext {
        operation_id: operation.into(),
        config: AppConfig {
            cleanup_enabled: false,
            collect_training_data: false,
            language: Language::English,
            ..Default::default()
        },
        model_root: root.join("no-models"),
        dataset_root: root.to_owned(),
        runtime: Arc::clone(runtime),
        inference: Arc::clone(inference),
        streaming_result: Arc::default(),
        personalization: Arc::default(),
    }
}

fn freeze(context: &WorkContext, snapshot: &DictationPersonalizationSnapshot, project: &str) {
    let mut frozen = snapshot.clone();
    frozen.profile.active_project = project.into();
    // Evaluation-only control: neither observation nor edits can add evidence.
    // This does not persist a production setting or alter the starting vocabulary.
    frozen.profile.automatic_learning = false;
    context.personalization.set(Ok(frozen)).unwrap();
}

fn timing(values: &[f64]) -> Value {
    if values.is_empty() {
        return Value::Null;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let percentile = |p: f64| sorted[(sorted.len() as f64 * p).ceil() as usize - 1];
    json!({"count":sorted.len(),"meanMs":sorted.iter().sum::<f64>() / sorted.len() as f64,"p50Ms":percentile(0.5),"p95Ms":percentile(0.95)})
}

fn recurring_timing_gate(timings: &Value, expected_pairs: usize) -> Value {
    let valid = expected_pairs > 0
        && ["frozen", "learning"].into_iter().all(|arm| {
            timings[arm]["count"].as_u64() == Some(expected_pairs as u64)
                && ["meanMs", "p95Ms"].into_iter().all(|metric| {
                    timings[arm][metric]
                        .as_f64()
                        .is_some_and(|v| v.is_finite() && v > 0.0)
                })
        });
    let margins = if valid {
        json!({
            "meanPercent": (timings["learning"]["meanMs"].as_f64().unwrap()
                / timings["frozen"]["meanMs"].as_f64().unwrap() - 1.0) * 100.0,
            "p95Percent": (timings["learning"]["p95Ms"].as_f64().unwrap()
                / timings["frozen"]["p95Ms"].as_f64().unwrap() - 1.0) * 100.0,
        })
    } else {
        Value::Null
    };
    let passed = valid
        && ["meanMs", "p95Ms"].into_iter().all(|metric| {
            timings["learning"][metric].as_f64().unwrap()
                <= timings["frozen"][metric].as_f64().unwrap() * 1.10
        });
    json!({"passed":passed,"expectedPairs":expected_pairs,"completeTiming":valid,
        "maximumIncreasePercent":10,"margins":margins})
}

#[test]
fn recurring_qualification_enforces_latency_and_complete_timing() {
    let good = json!({"frozen":{"count":3,"meanMs":100.0,"p95Ms":200.0},
        "learning":{"count":3,"meanMs":110.0,"p95Ms":220.0}});
    assert_eq!(recurring_timing_gate(&good, 3)["passed"], true);
    for metric in ["meanMs", "p95Ms"] {
        let mut slow = good.clone();
        slow["learning"][metric] = json!(good["learning"][metric].as_f64().unwrap() + 1.0);
        assert_eq!(recurring_timing_gate(&slow, 3)["passed"], false);
    }
    assert_eq!(recurring_timing_gate(&good, 4)["passed"], false);
    assert_eq!(recurring_timing_gate(&good, 0)["passed"], false);
    let mut missing = good;
    missing["learning"]["meanMs"] = Value::Null;
    assert_eq!(recurring_timing_gate(&missing, 3)["passed"], false);
}

fn report(value: &Value) {
    if let Some(path) = std::env::var_os("TRANSCRIBE_LEARNING_REPORT") {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .expect("new report path");
        serde_json::to_writer_pretty(&mut file, value).unwrap();
        writeln!(file).unwrap();
    }
    println!("qualification_totals={}", value["totals"]);
}

fn totals(rows: &[Value]) -> Value {
    let mut audited = 0;
    let mut improved = 0;
    let mut harmed = 0;
    let mut frozen = 0;
    let mut learning = 0;
    let mut frozen_needs_edit = 0;
    let mut learning_needs_edit = 0;
    for row in rows.iter().filter(|r| r["phase"] == "audit") {
        let Some(a) = row["frozenWordEdits"].as_u64() else {
            continue;
        };
        let Some(b) = row["learningWordEdits"].as_u64() else {
            continue;
        };
        audited += 1;
        frozen += a;
        learning += b;
        improved += u64::from(b < a);
        harmed += u64::from(b > a);
        frozen_needs_edit += u64::from(a > 0);
        learning_needs_edit += u64::from(b > 0);
    }
    json!({"auditRowsTotal":rows.iter().filter(|r| r["phase"] == "audit").count(),"auditRowsScored":audited,"frozenWordEdits":frozen,"learningWordEdits":learning,"improved":improved,"harmed":harmed,"frozenOutputsNeedingCorrection":frozen_needs_edit,"learningOutputsNeedingCorrection":learning_needs_edit})
}

#[test]
fn chronological_desktop_learning_against_frozen_memory() {
    let manifest: Value = serde_json::from_str(AUTHORED).unwrap();
    let episodes: Vec<Episode> = serde_json::from_value(manifest["episodes"].clone()).unwrap();
    assert!(episodes.windows(2).all(|w| w[0].session <= w[1].session));
    assert_eq!(
        episodes
            .iter()
            .map(|e| &e.id)
            .collect::<BTreeSet<_>>()
            .len(),
        episodes.len()
    );
    assert!(episodes
        .iter()
        .all(|e| matches!(e.phase.as_str(), "feedback" | "audit")
            && (e.phase != "audit" || e.edited.is_none())));
    let dir = tempfile::tempdir().unwrap();
    let roots = [dir.path().join("frozen"), dir.path().join("learning")];
    let initial = DatasetStore::open(roots[0].clone())
        .unwrap()
        .dictation_personalization_snapshot()
        .unwrap();
    let inference = Arc::new(vesper_inference::InferenceManager::new().unwrap());
    let mut runtimes = [Arc::new(Runtime::default()), Arc::new(Runtime::default())];
    let mut session = 0;
    let mut rows = vec![];
    let mut violations = vec![];
    for e in &episodes {
        if session != e.session {
            runtimes = [Arc::new(Runtime::default()), Arc::new(Runtime::default())];
            session = e.session;
        }
        let mut row = json!({"id":e.id,"session":e.session,"phase":e.phase,"kind":e.kind,"raw":e.raw,"reference":e.reference,"source":e.source});
        for arm in 0..2 {
            let store = DatasetStore::open(roots[arm].clone()).unwrap();
            let mut profile = store.dictation_personalization().unwrap();
            if profile.active_project != e.project {
                profile.active_project = e.project.clone();
                store.save_dictation_personalization(profile).unwrap();
            }
            let mut work = context(&roots[arm], &runtimes[arm], &inference, &e.id);
            work.config.language = e.language;
            work.config.models.asr = match e.model.as_str() {
                "cohere" => AsrModel::CohereTranscribe,
                "nemotron" => AsrModel::Nemotron35AsrStreaming06b,
                _ => panic!("unknown model"),
            };
            if arm == 0 {
                freeze(&work, &initial, &e.project);
            }
            let pcm = MonoPcm::new(vec![f32::from(e.source) / 1000.0; 1600], 16000).unwrap();
            let started = Instant::now();
            let mut result = work.complete_raw(&pcm, &mut None, e.raw.clone()).unwrap();
            work.observe_result(&mut result, true);
            work.runtime.publish(result.clone());
            let elapsed = started.elapsed().as_secs_f64() * 1000.0;
            let name = if arm == 0 { "frozen" } else { "learning" };
            row[name] = json!(final_text(&result));
            row[format!("{name}WordEdits")] =
                json!(word_edits(&e.reference, final_text(&result)).unwrap());
            row[format!("{name}CompletionMs")] = json!(elapsed);
            if !result.warnings.is_empty() {
                violations.push(format!("{} {name}: {:?}", e.id, result.warnings));
            }
            if arm == 0 {
                assert_eq!(final_text(&result), e.raw);
            }
            if arm == 1 {
                // Independent audit labels above are never submitted as edits.
                if let Some(edit) = &e.edited {
                    let started = Instant::now();
                    let edited = work
                        .runtime
                        .edit_latest_draft(roots[arm].clone(), &result.result_id, 0, edit.clone())
                        .unwrap();
                    row["editToMemoryReadyMs"] = json!(started.elapsed().as_secs_f64() * 1000.0);
                    row["learningOutcome"] = json!(edited.learning_outcome);
                }
                if e.undo {
                    work.runtime
                        .undo_unsaved_with_learning(&result.result_id, roots[arm].clone())
                        .unwrap();
                }
                if e.forget {
                    let revision = store.personalization().unwrap().revision;
                    store.forget_automatic_learning(revision).unwrap();
                    work.runtime.clear_learning_context();
                }
                if matches!(
                    e.kind.as_str(),
                    "wrong_context"
                        | "quoted_alias"
                        | "quoted_context"
                        | "wrong_project"
                        | "wrong_language"
                        | "unchanged"
                        | "intent_not_global"
                        | "number_not_global"
                        | "suspended_after_reversal"
                        | "suspended_after_undo"
                        | "forget_old_memory"
                        | "forget_new_memory"
                        | "forgotten_context"
                ) && final_text(&result) != e.raw
                {
                    violations.push(format!("{} protected control changed", e.id));
                }
            }
            let state = store.automatic_learning_state().unwrap();
            row[format!("{name}ActiveMemories")] = json!(store
                .personalization()
                .unwrap()
                .vocabulary
                .iter()
                .filter(|v| v.enabled)
                .count());
            row[format!("{name}IndependentSupport")] = json!(state
                .candidates
                .iter()
                .map(|c| c.support.len())
                .collect::<Vec<_>>());
            assert!(store.list_sessions().unwrap().is_empty());
            assert_eq!(fs::read_dir(roots[arm].join("audio")).unwrap().count(), 0);
            if arm == 0 {
                assert!(store.personalization().unwrap().vocabulary.is_empty());
                assert!(state.candidates.is_empty());
            }
        }
        rows.push(row);
    }
    let summary = totals(&rows);
    let value = json!({"schemaVersion":1,"evidenceClass":manifest["evidenceClass"],"manifestSha256":format!("{:x}",Sha256::digest(AUTHORED)),"totals":summary,"violations":violations,"rows":rows,"limitations":["No ASR, microphone, native WebView, natural feedback or human editing time.","Completion timings include constructed raw completion/observation/publication, not stop-to-visible latency.","Expected adverse unflagged memory use remains in audit totals."]});
    report(&value);
    assert!(violations.is_empty(), "{violations:?}");
    // The negative audit must stay visible; do not remove it to advertise no regressions.
    assert!(value["rows"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["kind"] == "unflagged_reversal"));
}

/// Authored controls for ordinary edits, later use and correction removal.
#[test]
fn ordinary_correction_learning_reuses_names_acronyms_and_formatted_edits() {
    let directory = tempfile::tempdir().unwrap();
    let inference = Arc::new(vesper_inference::InferenceManager::new().unwrap());
    let mut rows = vec![];
    for (alias, canonical, cue) in [
        ("BrooksidePark", "Brookside Park", "station"),
        ("QTX", "QTY", "driver"),
        ("postgress", "postgres", "cluster"),
    ] {
        for undo in [false, true] {
            let root = directory.path().join(format!("{alias}-{undo}"));
            let complete = |source: u16, raw: &str, model_switch: bool| {
                // Authored session/source boundaries; not independent real recordings.
                let runtime = Arc::new(Runtime::default());
                let mut work = context(&root, &runtime, &inference, &format!("{source}"));
                if model_switch {
                    work.config.models.asr = AsrModel::Nemotron35AsrStreaming06b;
                }
                let pcm = MonoPcm::new(vec![f32::from(source) / 100.0; 1600], 16000).unwrap();
                let mut result = work.complete_raw(&pcm, &mut None, raw.into()).unwrap();
                work.observe_result(&mut result, true);
                runtime.publish(result.clone());
                (runtime, result)
            };
            for (attempt, source) in [1, 1, 2].into_iter().enumerate() {
                let raw = if attempt < 2 {
                    format!("inspect {alias}, {cue}")
                } else {
                    format!("Maintain {alias} {cue}")
                };
                let target = if attempt < 2 {
                    format!("Inspect {canonical} {cue}.")
                } else {
                    format!("Maintain {canonical} {cue}")
                };
                let (runtime, result) = complete(source, &raw, false);
                assert_eq!(final_text(&result), raw);
                let edited = runtime
                    .edit_latest_draft(root.clone(), &result.result_id, 0, target)
                    .unwrap();
                let outcome = edited.learning_outcome.unwrap();
                if attempt < 2 {
                    assert!(outcome.activated.is_empty());
                    assert_eq!(outcome.provisional, 1);
                } else {
                    assert_eq!(outcome.activated, [canonical]);
                }
                let state = DatasetStore::open(root.clone())
                    .unwrap()
                    .automatic_learning_state()
                    .unwrap();
                let candidate = state.candidates.iter().find(|c| c.alias == alias).unwrap();
                assert_eq!(candidate.support.len(), if attempt < 2 { 1 } else { 2 });
            }
            for raw in [
                format!("Describe {alias} sculpture"),
                format!("Inspect '{alias}' {cue}"),
            ] {
                let (_, result) = complete(3, &raw, false);
                assert_eq!(final_text(&result), raw);
            }
            let raw = format!("Inspect {alias} {cue} again");
            let target = format!("Inspect {canonical} {cue} again");
            let (runtime, result) = complete(4, &raw, true);
            assert_eq!(final_text(&result), target);
            assert_eq!(word_edits(&target, final_text(&result)), Some(0));
            assert!(word_edits(&target, &raw).unwrap() > 0);
            if undo {
                runtime
                    .undo_unsaved_with_learning(&result.result_id, root.clone())
                    .unwrap();
            } else {
                let reversed = runtime
                    .edit_latest_draft(root.clone(), &result.result_id, 0, raw.clone())
                    .unwrap();
                assert_eq!(reversed.learning_outcome.unwrap().suspended, 1);
            }
            let (_, paused) = complete(5, &raw, false);
            assert_eq!(final_text(&paused), raw);
            let store = DatasetStore::open(root.clone()).unwrap();
            store
                .forget_automatic_learning(store.personalization().unwrap().revision)
                .unwrap();
            assert!(store
                .automatic_learning_state()
                .unwrap()
                .candidates
                .is_empty());
            assert!(store.personalization().unwrap().vocabulary.is_empty());
            assert!(store.list_sessions().unwrap().is_empty());
            assert_eq!(fs::read_dir(root.join("audio")).unwrap().count(), 0);
            rows.push(json!({"alias":alias,"canonical":canonical,"undo":undo,
                "distinctSupport":2,"duplicateSupportAdded":false,
                "rawWordEdits":word_edits(&target,&raw),"learningWordEdits":0,
                "wrongContextAndQuoteUnchanged":true,"suspensionPassed":true,"forgetPassed":true}));
        }
    }
    report(
        &json!({"schemaVersion":1,"policyVersion":vesper_core::LEARNING_POLICY_VERSION,
        "evidenceClass":"authored desktop/SQLite lifecycle controls; raw text and PCM identities are simulated, no ASR or natural feedback",
        "totals":{"families":3,"lifecycles":rows.len()},"rows":rows}),
    );
}

/// Replay recorded public text through the edit transaction without re-running ASR.
/// A successful diagnostic does not imply that any correction is learnable.
#[test]
fn public_correction_admission_diagnostic() {
    const HYPOTHESES: &str =
        include_str!("../../../../benchmarks/cleanup/native-learning-safety.json");
    let manifest: Value = serde_json::from_str(PUBLIC).unwrap();
    let hypotheses: Value = serde_json::from_str(HYPOTHESES).unwrap();
    let samples: Vec<PublicSample> = serde_json::from_value(manifest["samples"].clone()).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let inference = Arc::new(vesper_inference::InferenceManager::new().unwrap());
    let mut rows = vec![];
    let mut reasons = std::collections::BTreeMap::new();
    let mut persisted_edits = 0;
    let mut eligible_edits = 0;
    for sample in samples.iter().filter(|s| s.phase == "feedback") {
        let hypothesis = hypotheses["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["id"] == sample.id)
            .unwrap();
        let before = hypothesis["raw"].as_str().unwrap();
        let after = &sample.reference;
        let pair = vesper_core::automatic_spelling_pair(before, after);
        let proposed = vesper_core::correction_suggestions(before, after).pop();
        let reason = if before == after {
            "unchanged"
        } else if pair.is_some() {
            "eligible"
        } else if word_edits(before, after) == Some(0) {
            "no-token-change"
        } else if let Some(span) = &proposed {
            if span.heard.split_whitespace().count() > 3
                || span.intended.split_whitespace().count() != 1
            {
                "span-token-limit"
            } else if vesper_core::automatic_spelling_pair(&span.heard, &span.intended).is_none() {
                "isolated-pair-rejected"
            } else {
                "whole-edit-replay-rejected"
            }
        } else {
            "no-safe-contiguous-span"
        };
        *reasons.entry(reason).or_insert(0_usize) += 1;
        let root = directory.path().join(&sample.id);
        let runtime = Arc::new(Runtime::default());
        let work = context(&root, &runtime, &inference, &sample.id);
        let pcm = MonoPcm::new(vec![0.2; 1600], 16000).unwrap();
        let mut result = work.complete_raw(&pcm, &mut None, before.into()).unwrap();
        work.observe_result(&mut result, true);
        runtime.publish(result.clone());
        assert_eq!(final_text(&result), before);
        let outcome = if before != after {
            let edited = runtime
                .edit_latest_draft(root.clone(), &result.result_id, 0, after.clone())
                .unwrap();
            assert_eq!(edited.draft_transcript.as_deref(), Some(after.as_str()));
            assert_eq!(edited.draft_revision, 1);
            edited.learning_outcome
        } else {
            None
        };
        let store = DatasetStore::open(root.clone()).unwrap();
        let state = store.automatic_learning_state().unwrap();
        let events: Vec<_> = state
            .events
            .iter()
            .filter(|event| matches!(event.kind, vesper_data::LearningEventKind::UserEdit))
            .collect();
        assert_eq!(events.len(), usize::from(before != after));
        for event in &events {
            assert_eq!(event.context.result_id, result.result_id);
            assert_eq!(event.pair, pair);
        }
        persisted_edits += events.len();
        eligible_edits += usize::from(pair.is_some());
        assert!(store.list_sessions().unwrap().is_empty());
        assert_eq!(fs::read_dir(root.join("audio")).unwrap().count(), 0);
        rows.push(json!({"sampleId":sample.id,"cohort":sample.cohort,
            "reason":reason,"proposedSpan":proposed,"admittedPair":pair,
            "persistedEdits":events.len(),"learningOutcome":outcome}));
    }
    report(
        &json!({"schemaVersion":1,"policyVersion":vesper_core::LEARNING_POLICY_VERSION,
        "evidenceClass":"post-hoc recorded-public-text admission diagnostic; no new audio inference or natural feedback",
        "manifestSha256":format!("{:x}",Sha256::digest(PUBLIC)),
        "hypothesesSha256":format!("{:x}",Sha256::digest(HYPOTHESES)),
        "totals":{"calibrationRows":rows.len(),"persistedEdits":persisted_edits,
            "eligibleEdits":eligible_edits,"reasons":reasons},"rows":rows}),
    );
    assert_eq!(rows.len(), 19);
    assert_eq!(persisted_edits, 18);
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PublicSample {
    id: String,
    cohort: String,
    phase: String,
    audio_path: String,
    sha256: String,
    reference: String,
    duration_seconds: f64,
}

#[test]
#[ignore = "explicit cached public Svarah audio and installed Cohere/Qwen; no microphone or downloads"]
fn native_public_capture_learning_against_frozen_memory() {
    let model_root =
        PathBuf::from(std::env::var_os("TRANSCRIBE_MODEL_DIR").expect("TRANSCRIBE_MODEL_DIR"));
    let audio_root = PathBuf::from(
        std::env::var_os("TRANSCRIBE_LEARNING_AUDIO_ROOT").expect("TRANSCRIBE_LEARNING_AUDIO_ROOT"),
    );
    let manifest: Value = serde_json::from_str(PUBLIC).unwrap();
    let samples: Vec<PublicSample> = serde_json::from_value(manifest["samples"].clone()).unwrap();
    // Verify the complete selection before starting model work.
    assert_eq!(samples.len(), 38);
    let mut hashes = BTreeSet::new();
    for pair in samples.chunks_exact(2) {
        assert_eq!(pair[0].cohort, pair[1].cohort);
        assert_eq!(pair[0].phase, "feedback");
        assert_eq!(pair[1].phase, "audit");
    }
    for sample in &samples {
        assert!(hashes.insert(&sample.sha256), "duplicate audio");
        assert_eq!(
            format!(
                "{:x}",
                Sha256::digest(fs::read(audio_root.join(&sample.audio_path)).unwrap())
            ),
            sample.sha256
        );
    }
    let inference = Arc::new(vesper_inference::InferenceManager::new().unwrap());
    let dir = tempfile::tempdir().unwrap();
    let mut rows = vec![];
    let mut failures = vec![];
    let mut request = 0;
    for (cohort_index, pair) in samples.chunks_exact(2).enumerate() {
        assert_eq!(pair[0].phase, "feedback");
        assert_eq!(pair[1].phase, "audit");
        assert_eq!(pair[0].cohort, pair[1].cohort);
        let roots = [
            dir.path().join(format!("{cohort_index}-frozen")),
            dir.path().join(format!("{cohort_index}-learning")),
        ];
        let initial = DatasetStore::open(roots[0].clone())
            .unwrap()
            .dictation_personalization_snapshot()
            .unwrap();
        let runtimes = [Arc::new(Runtime::default()), Arc::new(Runtime::default())];
        for (sample_index, sample) in pair.iter().enumerate() {
            let pcm = vesper_audio::decode_file(audio_root.join(&sample.audio_path), 0).unwrap();
            assert!((pcm.duration_seconds() - sample.duration_seconds).abs() < 0.01);
            let mut row = json!({"id":sample.id,"cohort":sample.cohort,"phase":sample.phase,"reference":sample.reference,"audioSha256":sample.sha256,"audioSeconds":pcm.duration_seconds()});
            let order = if (cohort_index + sample_index) % 2 == 0 {
                [0, 1]
            } else {
                [1, 0]
            };
            row["armOrder"] = json!(order);
            for arm in order {
                let name = if arm == 0 { "frozen" } else { "learning" };
                let operation = format!("qualification-{request}");
                let mut work = context(&roots[arm], &runtimes[arm], &inference, &operation);
                work.config.cleanup_enabled = true;
                work.model_root = model_root.clone();
                if arm == 0 {
                    freeze(&work, &initial, "");
                }
                work.runtime
                    .begin("transcribing", "public capture replay")
                    .unwrap();
                work.runtime
                    .diagnostics
                    .begin(&operation, &work.config, false);
                let capture =
                    vesper_audio::CapturedAudio::new(pcm.samples().to_vec(), 16000, 1).unwrap();
                let start = Instant::now();
                let result = work.process_capture(capture);
                let elapsed = start.elapsed().as_secs_f64() * 1000.0;
                work.runtime.finish(&result);
                row[format!("{name}BackendCompletionMs")] = json!(elapsed);
                row[format!("{name}ColdRequest")] = json!(request == 0);
                row[format!("{name}StageTimings")] = json!(work.runtime.diagnostics.snapshot());
                row[format!("{name}Execution")] = inference.execution_evidence(&operation);
                request += 1;
                match result {
                    Err(error) => {
                        failures.push(format!("{} {name}: {error}", sample.id));
                        row[format!("{name}Error")] = json!(error);
                    }
                    Ok(result) => {
                        row[name] = json!(final_text(&result));
                        row[format!("{name}Raw")] = json!(result.raw_transcript);
                        row[format!("{name}WordEdits")] =
                            json!(word_edits(&sample.reference, final_text(&result)));
                        row[format!("{name}Warnings")] = json!(result.warnings);
                        if !result.warnings.is_empty() {
                            failures.push(format!("{} {name}: warnings", sample.id));
                        }
                        assert!(result.session.is_none());
                        if arm == 1 && sample.phase == "feedback" {
                            let start = Instant::now();
                            match work.runtime.edit_latest_draft(
                                roots[arm].clone(),
                                &result.result_id,
                                0,
                                sample.reference.clone(),
                            ) {
                                Ok(edited) => {
                                    row["simulatedFeedbackOutcome"] =
                                        json!(edited.learning_outcome);
                                }
                                Err(error) => {
                                    failures.push(format!("{} feedback: {error}", sample.id));
                                    row["feedbackError"] = json!(error);
                                }
                            }
                            row["editToMemoryReadyMs"] =
                                json!(start.elapsed().as_secs_f64() * 1000.0);
                        }
                    }
                }
                let store = DatasetStore::open(roots[arm].clone()).unwrap();
                row[format!("{name}ActiveMemories")] = json!(store
                    .personalization()
                    .unwrap()
                    .vocabulary
                    .iter()
                    .filter(|v| v.enabled)
                    .count());
                assert!(store.list_sessions().unwrap().is_empty());
                assert_eq!(fs::read_dir(roots[arm].join("audio")).unwrap().count(), 0);
                if arm == 0 {
                    assert!(store.personalization().unwrap().vocabulary.is_empty());
                    assert!(store
                        .automatic_learning_state()
                        .unwrap()
                        .candidates
                        .is_empty());
                }
            }
            println!("completed_public_pair={} phase={}", sample.id, sample.phase);
            rows.push(row);
        }
    }
    let mut times = json!({});
    for name in ["frozen", "learning"] {
        let values = rows
            .iter()
            .filter(|r| r["frozenColdRequest"] == false && r["learningColdRequest"] == false)
            .filter_map(|r| r[format!("{name}BackendCompletionMs")].as_f64())
            .collect::<Vec<_>>();
        times[name] = timing(&values);
    }
    let mut summary = totals(&rows);
    summary["failures"] = json!(failures.len());
    let value = json!({"schemaVersion":1,"evidenceClass":"native post-capture public-audio diagnostic; simulated reference feedback, no natural user sessions","manifestSha256":format!("{:x}",Sha256::digest(PUBLIC)),"modelProvenance":vesper_models::provenance(&AppConfig::default()),"modelVersions":vesper_inference::model_versions(&AppConfig::default()),"totals":summary,"matchedResidentBackendCompletion":times,"failures":failures,"rows":rows,"stopToVisibleFinalMs":Value::Null,"humanEditingTimeMs":Value::Null,"limitations":["Reuses previously screened public corpus; two different recordings per language-background cohort cannot establish speaker identity or longitudinal benefit.","Backend time starts at process_capture entry and ends after runtime publication; excludes microphone stop, IPC, WebView rendering and insertion.","Cold first request retained separately; exact execution evidence records loads, providers and request ordinals.","Collection is disabled; no audio, session or reviewed training target is archived."]});
    report(&value);
    assert!(failures.is_empty(), "{failures:?}");
}

#[test]
fn revised_learning_lifecycles_use_ordinary_drafts_with_and_without_history() {
    let directory = tempfile::tempdir().unwrap();
    let inference = Arc::new(vesper_inference::InferenceManager::new().unwrap());
    for collect in [false, true] {
        let root = directory.path().join(format!("revised-{collect}"));
        let complete = |source: u16, raw: &str| {
            let runtime = Arc::new(Runtime::default());
            let mut work = context(&root, &runtime, &inference, &format!("{source}"));
            work.config.collect_training_data = collect;
            let pcm = MonoPcm::new(vec![f32::from(source) / 100.0; 1600], 16000).unwrap();
            let capture =
                vesper_audio::CapturedAudio::new(pcm.samples().to_vec(), 16000, 1).unwrap();
            let mut session = work.archive_capture(&capture).unwrap();
            let mut result = work.complete_raw(&pcm, &mut session, raw.into()).unwrap();
            work.observe_result(&mut result, true);
            runtime.publish(result.clone());
            assert_eq!(result.session.is_some(), collect);
            (runtime, result)
        };
        let edit = |runtime: &Runtime, result: &PipelineResult, text: &str| {
            let edited = runtime
                .edit_latest_draft(
                    root.clone(),
                    &result.result_id,
                    result.draft_revision,
                    text.into(),
                )
                .unwrap();
            assert_eq!(edited.draft_transcript.as_deref(), Some(text));
            if let Some(session) = &edited.session {
                assert_eq!(session.draft_transcript.as_deref(), Some(text));
                assert_eq!(session.draft_revision, edited.draft_revision);
            }
            edited
        };
        let (runtime, first) = complete(1, "Visit Jonathon museum");
        let first = edit(&runtime, &first, "Visit Jonathan museum");
        assert_eq!(first.learning_outcome.as_ref().unwrap().provisional, 1);
        edit(&runtime, &first, "Visit Jonathon museum");
        let (runtime, second) = complete(2, "Visit Jonathon museum");
        assert!(edit(&runtime, &second, "Visit Jonathan museum")
            .learning_outcome
            .unwrap()
            .activated
            .is_empty());
        let (runtime, third) = complete(3, "Meet Jonathon station");
        assert_eq!(
            edit(&runtime, &third, "Meet Jonathan station")
                .learning_outcome
                .unwrap()
                .activated,
            ["Jonathan"]
        );
        let (runtime, later) = complete(4, "Find Jonathon station");
        assert_eq!(final_text(&later), "Find Jonathan station");
        let revised = edit(&runtime, &later, "Find Jonatan station");
        assert_eq!(revised.learning_outcome.unwrap().suspended, 1);
        for (source, raw) in [
            (5, "Inspect Jonathon station"),
            (6, "Locate Jonathon station"),
        ] {
            let (runtime, result) = complete(source, raw);
            assert_eq!(final_text(&result), raw);
            let edited = edit(&runtime, &result, &raw.replace("Jonathon", "Jonatan"));
            assert_eq!(
                !edited.learning_outcome.unwrap().activated.is_empty(),
                source == 6
            );
        }
        let (_, corrected) = complete(7, "Find Jonathon station");
        assert_eq!(final_text(&corrected), "Find Jonatan station");
        for (source, raw) in [
            (8, "Sketch Jonathon sculpture"),
            (9, "Find 'Jonathon' station"),
        ] {
            let (_, result) = complete(source, raw);
            assert_eq!(final_text(&result), raw);
        }
        let store = DatasetStore::open(root.clone()).unwrap();
        assert_eq!(store.list_sessions().unwrap().is_empty(), !collect);
        assert_eq!(
            fs::read_dir(root.join("audio")).unwrap().count() == 0,
            !collect
        );
        store
            .forget_automatic_learning(store.personalization().unwrap().revision)
            .unwrap();
        let (_, forgotten) = complete(10, "Find Jonathon station");
        assert_eq!(final_text(&forgotten), "Find Jonathon station");
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RecurringSample {
    id: String,
    cohort: String,
    kind: String,
    phase: String,
    spoken: String,
    reference: String,
    audio_path: String,
    sha256: String,
}

#[test]
#[ignore = "installed models and explicitly prepared synthetic recurring-term corpus"]
fn native_recurring_spelling_learning_against_frozen_memory() {
    let model_root = PathBuf::from(std::env::var_os("TRANSCRIBE_MODEL_DIR").unwrap());
    let audio_root = PathBuf::from(std::env::var_os("TRANSCRIBE_LEARNING_AUDIO_ROOT").unwrap());
    let manifest_bytes = fs::read(audio_root.join("manifest.json")).unwrap();
    let manifest: Value = serde_json::from_slice(&manifest_bytes).unwrap();
    let samples: Vec<RecurringSample> =
        serde_json::from_value(manifest["samples"].clone()).unwrap();
    assert_eq!(samples.len(), 72);
    let kinds = [
        "teach-one",
        "duplicate",
        "teach-two",
        "reuse-new-context",
        "wrong-context",
        "no-alias",
        "revise-target",
        "fresh-one",
        "fresh-two",
        "reuse-revised",
        "wrong-context-revised",
        "forget",
    ];
    let mut hashes = BTreeSet::new();
    for cohort in samples.chunks_exact(12) {
        for (sample, kind) in cohort.iter().zip(kinds) {
            assert_eq!(sample.kind, kind);
            assert_eq!(sample.cohort, cohort[0].cohort);
            assert_eq!(
                sample.phase,
                if matches!(
                    kind,
                    "teach-one"
                        | "duplicate"
                        | "teach-two"
                        | "revise-target"
                        | "fresh-one"
                        | "fresh-two"
                ) {
                    "feedback"
                } else {
                    "audit"
                }
            );
            assert!(Path::new(&sample.audio_path)
                .components()
                .all(|c| matches!(c, std::path::Component::Normal(_))));
            assert_eq!(
                format!(
                    "{:x}",
                    Sha256::digest(fs::read(audio_root.join(&sample.audio_path)).unwrap())
                ),
                sample.sha256
            );
            if kind == "duplicate" {
                assert_eq!(sample.sha256, cohort[0].sha256);
            } else if kind != "no-alias" {
                assert!(hashes.insert(&sample.sha256), "unexpected duplicate speech");
            }
        }
    }
    let directory = tempfile::tempdir().unwrap();
    let inference = Arc::new(vesper_inference::InferenceManager::new().unwrap());
    let mut rows = vec![];
    let mut failures = vec![];
    let mut request = 0;
    for (cohort_index, cohort) in samples.chunks_exact(12).enumerate() {
        let roots = [
            directory.path().join(format!("frozen-{cohort_index}")),
            directory.path().join(format!("learning-{cohort_index}")),
        ];
        let initial = DatasetStore::open(roots[0].clone())
            .unwrap()
            .dictation_personalization_snapshot()
            .unwrap();
        let mut duplicate_support = None;
        for (index, sample) in cohort.iter().enumerate() {
            let pcm = vesper_audio::decode_file(audio_root.join(&sample.audio_path), 0).unwrap();
            let mut row = json!({"id":sample.id,"cohort":sample.cohort,"kind":sample.kind,"phase":sample.phase,
                "reference":sample.reference,"audioSha256":sample.sha256,"audioSeconds":pcm.duration_seconds()});
            let order = if (cohort_index + index) % 2 == 0 {
                [0, 1]
            } else {
                [1, 0]
            };
            row["armOrder"] = json!(order);
            for arm in order {
                let name = if arm == 0 { "frozen" } else { "learning" };
                let store = DatasetStore::open(roots[arm].clone()).unwrap();
                if arm == 1 && sample.kind == "forget" {
                    store
                        .forget_automatic_learning(store.personalization().unwrap().revision)
                        .unwrap();
                }
                // Synthetic episode boundary, not evidence of independent humans.
                let runtime = Arc::new(Runtime::default());
                let operation = format!("recurring-{request}");
                let mut work = context(&roots[arm], &runtime, &inference, &operation);
                work.config.cleanup_enabled = true;
                work.model_root = model_root.clone();
                if arm == 0 {
                    freeze(&work, &initial, "");
                }
                runtime
                    .begin("transcribing", "synthetic recurring-term replay")
                    .unwrap();
                runtime.diagnostics.begin(&operation, &work.config, false);
                let capture =
                    vesper_audio::CapturedAudio::new(pcm.samples().to_vec(), 16000, 1).unwrap();
                let start = Instant::now();
                let result = work.process_capture(capture);
                let elapsed = start.elapsed().as_secs_f64() * 1000.0;
                runtime.finish(&result);
                row[format!("{name}BackendCompletionMs")] = json!(elapsed);
                row[format!("{name}ColdRequest")] = json!(request == 0);
                row[format!("{name}Execution")] = inference.execution_evidence(&operation);
                request += 1;
                match result {
                    Err(error) => {
                        failures.push(format!("{} {name}: {error}", sample.id));
                        row[format!("{name}Error")] = json!(error);
                    }
                    Ok(result) => {
                        row[name] = json!(final_text(&result));
                        row[format!("{name}Raw")] = json!(result.raw_transcript);
                        row[format!("{name}WordEdits")] =
                            json!(word_edits(&sample.reference, final_text(&result)));
                        row[format!("{name}Warnings")] = json!(result.warnings);
                        if !result.warnings.is_empty() {
                            failures.push(format!("{} {name}: warnings", sample.id));
                        }
                        let selected = work
                            .personalization_snapshot()
                            .unwrap()
                            .post_asr_profile(&result.raw_transcript, Language::English);
                        row[format!("{name}SelectedMemories")] = json!(selected
                            .vocabulary
                            .iter()
                            .filter(|v| v.enabled && v.automatic_source.is_some())
                            .count());
                        row[format!("{name}AppliedMemories")] = json!(result
                            .personalization_evidence
                            .as_ref()
                            .map_or(0, |e| e.applied.len()));
                        if arm == 0 && sample.phase == "audit" && sample.kind.starts_with("reuse-")
                        {
                            row["reuseOpportunity"] = json!(
                                vesper_core::memory_phrase_occurs(
                                    &result.raw_transcript,
                                    &sample.spoken
                                ) && word_edits(&sample.reference, final_text(&result))
                                    .is_some_and(|n| n > 0)
                            );
                        }
                        if arm == 1 && sample.phase == "feedback" {
                            row["eligibleDisplayedPair"] =
                                json!(vesper_core::automatic_spelling_pair(
                                    final_text(&result),
                                    &sample.reference
                                ));
                            let start = Instant::now();
                            match runtime.edit_latest_draft(
                                roots[arm].clone(),
                                &result.result_id,
                                0,
                                sample.reference.clone(),
                            ) {
                                Ok(edited) => {
                                    row["learningOutcome"] = json!(edited.learning_outcome)
                                }
                                Err(error) => {
                                    failures.push(format!("{} feedback: {error}", sample.id))
                                }
                            }
                            row["editToMemoryReadyMs"] =
                                json!(start.elapsed().as_secs_f64() * 1000.0);
                        }
                        assert!(result.session.is_none());
                    }
                }
                let state = store.automatic_learning_state().unwrap();
                let support = state
                    .candidates
                    .iter()
                    .map(|c| c.support.len())
                    .sum::<usize>();
                row[format!("{name}Support")] = json!(support);
                row[format!("{name}Candidates")] = json!(state.candidates);
                row[format!("{name}ActiveMemories")] = json!(store
                    .personalization()
                    .unwrap()
                    .vocabulary
                    .iter()
                    .filter(|v| v.enabled)
                    .count());
                if arm == 1 && sample.kind == "teach-one" {
                    duplicate_support = Some(support);
                }
                if arm == 1 && sample.kind == "duplicate" && duplicate_support != Some(support) {
                    failures.push(format!(
                        "{}: duplicated audio changed independent support",
                        sample.id
                    ));
                }
                assert!(store.list_sessions().unwrap().is_empty());
                assert_eq!(fs::read_dir(roots[arm].join("audio")).unwrap().count(), 0);
                if arm == 0 {
                    assert!(state.candidates.is_empty());
                    assert!(state.events.is_empty());
                    assert!(store.personalization().unwrap().vocabulary.is_empty());
                }
            }
            println!(
                "completed_recurring_pair={} phase={}",
                sample.id, sample.phase
            );
            rows.push(row);
        }
    }
    let mut summary = totals(&rows);
    summary["requests"] = json!(request);
    summary["failures"] = json!(failures.len());
    summary["reuseOpportunities"] = json!(rows
        .iter()
        .filter(|r| r["reuseOpportunity"] == true)
        .count());
    summary["feedbackRows"] = json!(rows.iter().filter(|r| r["phase"] == "feedback").count());
    summary["eligibleFeedbackPairs"] = json!(rows
        .iter()
        .filter(|r| !r["eligibleDisplayedPair"].is_null())
        .count());
    summary["activations"] = json!(rows
        .iter()
        .filter_map(|r| r["learningOutcome"]["activated"].as_array())
        .map(Vec::len)
        .sum::<usize>());
    summary["auditRowsWithSelectedMemory"] =
        json!(rows
            .iter()
            .filter(|r| r["phase"] == "audit"
                && r["learningSelectedMemories"].as_u64().unwrap_or(0) > 0)
            .count());
    summary["auditRowsWithDeterministicApplication"] = json!(rows
        .iter()
        .filter(|r| r["phase"] == "audit" && r["learningAppliedMemories"].as_u64().unwrap_or(0) > 0)
        .count());
    let mut timings = json!({});
    for name in ["frozen", "learning"] {
        timings[name] = timing(
            &rows
                .iter()
                .filter(|r| r["frozenColdRequest"] == false && r["learningColdRequest"] == false)
                .filter_map(|r| r[format!("{name}BackendCompletionMs")].as_f64())
                .collect::<Vec<_>>(),
        );
    }
    let timing_gate = recurring_timing_gate(&timings, rows.len().saturating_sub(1));
    let passed = failures.is_empty()
        && timing_gate["passed"] == true
        && summary["harmed"] == 0
        && summary["improved"].as_u64().unwrap() > 0
        && summary["reuseOpportunities"].as_u64().unwrap() > 0;
    report(
        &json!({"schemaVersion":1,"policyVersion":vesper_core::LEARNING_POLICY_VERSION,
        "evidenceClass":manifest["evidenceClass"],"manifestSha256":format!("{:x}",Sha256::digest(manifest_bytes)),
        "corpusProvenance":manifest["provenance"],"modelProvenance":vesper_models::provenance(&AppConfig::default()),
        "modelVersions":vesper_inference::model_versions(&AppConfig::default()),"totals":summary,"gatesPassed":passed,
        "matchedResidentBackendCompletion":timings,"exploratoryTimingGate":timing_gate,"failures":failures,"rows":rows,
        "stopToVisibleFinalMs":Value::Null,"humanEditingTimeMs":Value::Null,"limitations":manifest["limitations"]}),
    );
    assert!(
        passed,
        "recurring learning diagnostic gates failed: {summary}"
    );
}

/// Post-hoc wiring diagnostic: one public source and an authored identifier preference.
/// Replays must not be counted as independent support or new acoustic accuracy evidence.
#[test]
#[ignore = "explicit cached public audio and installed Cohere/Qwen; authored edit, no microphone"]
fn native_public_identifier_memory_roundtrip() {
    let model_root =
        PathBuf::from(std::env::var_os("TRANSCRIBE_MODEL_DIR").expect("TRANSCRIBE_MODEL_DIR"));
    let audio_root = PathBuf::from(
        std::env::var_os("TRANSCRIBE_LEARNING_AUDIO_ROOT").expect("TRANSCRIBE_LEARNING_AUDIO_ROOT"),
    );
    let manifest: Value = serde_json::from_str(PUBLIC).unwrap();
    let sample: PublicSample = serde_json::from_value(manifest["samples"][0].clone()).unwrap();
    assert_eq!(sample.id, "svarah-281474976906756_f3021_chunk_1");
    let path = audio_root.join(&sample.audio_path);
    assert_eq!(
        format!("{:x}", Sha256::digest(fs::read(&path).unwrap())),
        sample.sha256
    );
    let pcm = vesper_audio::decode_file(path, 0).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("dataset");
    let store = DatasetStore::open(root.clone()).unwrap();
    let mut profile = store.dictation_personalization().unwrap();
    profile.active_project = "identifier-mechanism".into();
    store.save_dictation_personalization(profile).unwrap();
    let inference = Arc::new(vesper_inference::InferenceManager::new().unwrap());
    let mut rows = vec![];
    let mut violations = vec![];
    for step in 0..4 {
        // Recreate the desktop Runtime; model ownership remains unchanged/resident.
        let runtime = Arc::new(Runtime::default());
        let operation = format!("identifier-roundtrip-{step}");
        let mut work = context(&root, &runtime, &inference, &operation);
        work.model_root = model_root.clone();
        work.config.cleanup_enabled = true;
        runtime
            .begin("transcribing", "public identifier mechanism")
            .unwrap();
        runtime.diagnostics.begin(&operation, &work.config, false);
        let capture = vesper_audio::CapturedAudio::new(pcm.samples().to_vec(), 16000, 1).unwrap();
        let started = Instant::now();
        let result = work.process_capture(capture);
        let elapsed = started.elapsed().as_secs_f64() * 1000.0;
        runtime.finish(&result);
        let mut row = json!({"step":step,"backendCompletionMs":elapsed,"execution":inference.execution_evidence(&operation),"stageTimings":runtime.diagnostics.snapshot()});
        match result {
            Err(error) => {
                violations.push(format!("step {step}: {error}"));
                row["error"] = json!(error);
            }
            Ok(result) => {
                let text = final_text(&result).to_owned();
                row["raw"] = json!(result.raw_transcript);
                row["final"] = json!(text);
                row["warnings"] = json!(result.warnings);
                row["sourceKey"] = json!(result.learning_context.source_key);
                if !result.warnings.is_empty() {
                    violations.push(format!("step {step}: warnings"));
                }
                let expect_identifier = step == 1;
                if text.contains("application_id") != expect_identifier {
                    violations.push(format!("step {step}: identifier presence mismatch"));
                }
                match step {
                    0 => {
                        // An authored project identifier, not the corpus's verbatim reference.
                        if !text.contains("application ID") {
                            violations
                                .push("source phrase absent from initial model output".into());
                        }
                        let target = text.replace("application ID", "application_id");
                        let edited = runtime
                            .edit_latest_draft(root.clone(), &result.result_id, 0, target)
                            .unwrap();
                        row["editOutcome"] = json!(edited.learning_outcome);
                        if !store
                            .personalization()
                            .unwrap()
                            .vocabulary
                            .iter()
                            .any(|v| v.enabled && v.phrase == "application_id")
                        {
                            violations.push("identifier did not activate".into());
                        }
                    }
                    1 => {
                        let edited = runtime
                            .edit_latest_draft(
                                root.clone(),
                                &result.result_id,
                                0,
                                text.replace("application_id", "application ID"),
                            )
                            .unwrap();
                        row["reversalOutcome"] = json!(edited.learning_outcome);
                    }
                    2 => {
                        let revision = store.personalization().unwrap().revision;
                        store.forget_automatic_learning(revision).unwrap();
                        runtime.clear_learning_context();
                        let edited = runtime
                            .edit_latest_draft(
                                root.clone(),
                                &result.result_id,
                                0,
                                text.replace("application ID", "application_id"),
                            )
                            .unwrap();
                        row["oldEpochEditOutcome"] = json!(edited.learning_outcome);
                        if !store.automatic_learning_state().unwrap().events.is_empty() {
                            violations.push("stale edit recreated forgotten evidence".into());
                        }
                    }
                    _ => {}
                }
                assert!(result.session.is_none());
            }
        }
        let state = store.automatic_learning_state().unwrap();
        if state.candidates.iter().any(|c| c.support.len() > 1) {
            violations.push("repeated source manufactured independent support".into());
        }
        row["activeMemories"] = json!(store
            .personalization()
            .unwrap()
            .vocabulary
            .iter()
            .filter(|v| v.enabled)
            .count());
        rows.push(row);
    }
    assert!(store.list_sessions().unwrap().is_empty());
    assert_eq!(fs::read_dir(root.join("audio")).unwrap().count(), 0);
    let value = json!({"schemaVersion":1,"evidenceClass":"post-hoc native wiring test; scripted identifier preference and repeated public source, not independent feedback or accuracy evidence","sampleId":sample.id,"audioSha256":sample.sha256,"totals":{"requests":rows.len(),"violations":violations.len()},"violations":violations,"rows":rows,"limitations":["Restarts Runtime objects, not the OS application process or native WebView.","All four requests use the same recording and resident Cohere/Qwen; no acoustic model switch or microphone is claimed."]});
    report(&value);
    assert!(violations.is_empty(), "{violations:?}");
}
