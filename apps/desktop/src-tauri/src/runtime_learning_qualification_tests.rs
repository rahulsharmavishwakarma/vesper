//! Diagnostic qualification, not natural-user or microphone/WebView evidence.
use super::*;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, fs, io::Write, time::Instant};
use transcribe_core::{word_edits, AsrModel};
use transcribe_data::DictationPersonalizationSnapshot;

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
    inference: &Arc<transcribe_inference::InferenceManager>,
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
    let inference = Arc::new(transcribe_inference::InferenceManager::new().unwrap());
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
    let inference = Arc::new(transcribe_inference::InferenceManager::new().unwrap());
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
            let pcm =
                transcribe_audio::decode_file(audio_root.join(&sample.audio_path), 0).unwrap();
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
                    transcribe_audio::CapturedAudio::new(pcm.samples().to_vec(), 16000, 1).unwrap();
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
    let value = json!({"schemaVersion":1,"evidenceClass":"native post-capture public-audio diagnostic; simulated reference feedback, no natural user sessions","manifestSha256":format!("{:x}",Sha256::digest(PUBLIC)),"modelProvenance":transcribe_models::provenance(&AppConfig::default()),"modelVersions":transcribe_inference::model_versions(&AppConfig::default()),"totals":summary,"matchedResidentBackendCompletion":times,"failures":failures,"rows":rows,"stopToVisibleFinalMs":Value::Null,"humanEditingTimeMs":Value::Null,"limitations":["Reuses previously screened public corpus; two different recordings per language-background cohort cannot establish speaker identity or longitudinal benefit.","Backend time starts at process_capture entry and ends after runtime publication; excludes microphone stop, IPC, WebView rendering and insertion.","Cold first request retained separately; exact execution evidence records loads, providers and request ordinals.","Collection is disabled; no audio, session or reviewed training target is archived."]});
    report(&value);
    assert!(failures.is_empty(), "{failures:?}");
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
    let pcm = transcribe_audio::decode_file(path, 0).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("dataset");
    let store = DatasetStore::open(root.clone()).unwrap();
    let mut profile = store.dictation_personalization().unwrap();
    profile.active_project = "identifier-mechanism".into();
    store.save_dictation_personalization(profile).unwrap();
    let inference = Arc::new(transcribe_inference::InferenceManager::new().unwrap());
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
        let capture =
            transcribe_audio::CapturedAudio::new(pcm.samples().to_vec(), 16000, 1).unwrap();
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
