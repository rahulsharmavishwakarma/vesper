//! Explicit native WebView/public-PCM qualification. Never opens a microphone.
//! Reuses production runtime sources without exporting a benchmark API in the app.
#[cfg(all(target_os = "macos", not(test)))]
#[allow(dead_code)]
#[path = "../src/diagnostics.rs"]
mod diagnostics;
#[cfg(all(target_os = "macos", not(test)))]
#[allow(dead_code)]
#[path = "../src/runtime.rs"]
mod runtime;

#[cfg(all(target_os = "macos", not(test)))]
mod probe {
    use super::runtime::{PipelineResult, Runtime, WorkContext};
    use serde::Deserialize;
    use serde_json::{json, Value};
    use sha2::{Digest, Sha256};
    use std::{
        fs::{self, File, OpenOptions},
        io::Write,
        path::PathBuf,
        sync::{atomic::AtomicBool, Arc, Mutex},
        time::Instant,
    };
    use vesper_core::{AppConfig, Language};
    use vesper_data::DatasetStore;

    const MANIFEST: &str =
        include_str!("../../../../benchmarks/asr/manifests/native-learning-2026-09-19.json");
    const URL: &str = "http://127.0.0.1:5198";
    static SUCCESS: AtomicBool = AtomicBool::new(false);

    #[derive(Clone, Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Sample {
        id: String,
        cohort: String,
        phase: String,
        audio_path: String,
        sha256: String,
        reference: String,
        duration_seconds: f64,
    }
    #[derive(Clone)]
    struct Case {
        work: WorkContext,
        sample: usize,
        arm: usize,
        row: Value,
    }
    struct Sequence {
        next: usize,
        current: Option<Case>,
    }
    struct Probe {
        _temporary: tempfile::TempDir,
        roots: Vec<[PathBuf; 2]>,
        runtimes: Mutex<Vec<[Arc<Runtime>; 2]>>,
        samples: Vec<Sample>,
        pcm: Vec<Vec<f32>>,
        config: AppConfig,
        models: PathBuf,
        inference: Arc<vesper_inference::InferenceManager>,
        sequence: Mutex<Sequence>,
        log: Mutex<File>,
    }
    impl Probe {
        fn write(&self, row: &Value) -> Result<(), String> {
            let mut file = self.log.lock().map_err(|e| e.to_string())?;
            serde_json::to_writer(&mut *file, row).map_err(|e| e.to_string())?;
            writeln!(file)
                .and_then(|()| file.flush())
                .map_err(|e| e.to_string())
        }
        fn current(&self) -> Result<Case, String> {
            self.sequence
                .lock()
                .map_err(|e| e.to_string())?
                .current
                .clone()
                .ok_or("No prepared replay request.".into())
        }
    }
    fn final_text(result: &PipelineResult) -> &str {
        result
            .draft_transcript
            .as_deref()
            .or(result.personalized_transcript.as_deref())
            .or(result.cleaned_transcript.as_deref())
            .unwrap_or(&result.raw_transcript)
    }

    #[tauri::command]
    fn prepare_case(state: tauri::State<'_, Arc<Probe>>) -> Result<Value, String> {
        let mut sequence = state.sequence.lock().map_err(|e| e.to_string())?;
        if sequence.current.is_some() {
            return Err("Previous request has not been reported.".into());
        }
        let index = sequence.next;
        if index == state.samples.len() * 2 {
            return Ok(Value::Null);
        }
        let sample_index = index / 2;
        let cohort = sample_index / 2;
        let arm = (index % 2) ^ ((cohort + sample_index % 2) % 2);
        let sample = &state.samples[sample_index];
        let root = state.roots[cohort][arm].clone();
        let work = WorkContext {
            operation_id: format!("webview-{index}"),
            config: state.config.clone(),
            model_root: state.models.clone(),
            dataset_root: root.clone(),
            runtime: Arc::clone(&state.runtimes.lock().map_err(|e| e.to_string())?[cohort][arm]),
            inference: Arc::clone(&state.inference),
            streaming_result: Arc::default(),
            personalization: Arc::default(),
        };
        if arm == 0 {
            let store = DatasetStore::open(root).map_err(|e| e.to_string())?;
            let mut snapshot = store
                .dictation_personalization_snapshot()
                .map_err(|e| e.to_string())?;
            if !snapshot.profile.vocabulary.is_empty() {
                return Err("Frozen vocabulary changed.".into());
            }
            snapshot.profile.automatic_learning = false;
            work.personalization
                .set(Ok(snapshot))
                .map_err(|_| "Snapshot already initialized")?;
        }
        sequence.current = Some(Case {
            work: work.clone(),
            sample: sample_index,
            arm,
            row: json!({"kind":"request", "request":index, "operationId":work.operation_id,
                "sampleId":sample.id,"cohort":sample.cohort,"phase":sample.phase,
                "arm":if arm == 0 {"frozen"} else {"learning"},"cold":index == 0,
                "audioSha256":sample.sha256,"audioSeconds":sample.duration_seconds}),
        });
        sequence.next += 1;
        Ok(
            json!({"operationId":work.operation_id,"sampleId":sample.id,"arm":arm,
            "config":state.config,"feedback":if arm == 1 && sample.phase == "feedback" {Some(&sample.reference)} else {None}}),
        )
    }

    #[tauri::command]
    fn start_recording(state: tauri::State<'_, Arc<Probe>>) -> Result<String, String> {
        let work = state.current()?.work;
        work.runtime
            .begin("recording", "Preloaded public PCM; microphone disabled")?;
        work.runtime.set_operation_id(&work.operation_id);
        work.runtime
            .diagnostics
            .begin(&work.operation_id, &work.config, true);
        work.runtime.set_recording(work.clone());
        Ok(work.operation_id)
    }
    #[tauri::command]
    async fn stop_recording(state: tauri::State<'_, Arc<Probe>>) -> Result<PipelineResult, String> {
        let probe = Arc::clone(&state);
        let current = probe.current()?;
        let work = current
            .work
            .runtime
            .take_recording()
            .ok_or("Replay already stopped")?;
        let pcm = probe.pcm[current.sample].clone();
        work.runtime
            .progress("transcribing", "Processing public recording…");
        tauri::async_runtime::spawn_blocking(move || {
            let capture =
                vesper_audio::CapturedAudio::new(pcm, 16000, 1).map_err(|e| e.to_string())?;
            let start = Instant::now();
            let result = work.process_capture(capture);
            let elapsed = start.elapsed().as_secs_f64() * 1000.0;
            work.runtime.finish(&result);
            let mut sequence = probe.sequence.lock().map_err(|e| e.to_string())?;
            let case = sequence.current.as_mut().ok_or("Missing replay request")?;
            if case.work.operation_id != work.operation_id {
                return Err("Stale completion".into());
            }
            case.row["backendCompletionMs"] = json!(elapsed);
            case.row["stageTimings"] = json!(work.runtime.diagnostics.snapshot());
            case.row["execution"] = probe.inference.execution_evidence(&work.operation_id);
            match &result {
                Ok(output) => {
                    case.row["resultId"] = json!(output.result_id);
                    case.row["output"] = json!(final_text(output));
                    case.row["raw"] = json!(output.raw_transcript);
                    case.row["wordEdits"] = json!(vesper_core::word_edits(
                        &probe.samples[case.sample].reference,
                        final_text(output)
                    ));
                    case.row["warnings"] = json!(output.warnings);
                }
                Err(error) => case.row["error"] = json!(error),
            }
            result
        })
        .await
        .map_err(|e| e.to_string())?
    }
    #[tauri::command]
    fn get_runtime_status(
        state: tauri::State<'_, Arc<Probe>>,
    ) -> Result<super::runtime::RuntimeStatus, String> {
        let work = state.current()?.work;
        work.runtime.status(
            vesper_models::inspect(&work.config, &work.model_root),
            0.0,
            0,
        )
    }
    #[tauri::command]
    fn get_latest_result(
        state: tauri::State<'_, Arc<Probe>>,
    ) -> Result<Option<PipelineResult>, String> {
        state.current()?.work.runtime.latest()
    }
    #[tauri::command]
    fn get_model_installation(state: tauri::State<'_, Arc<Probe>>) -> vesper_models::Installation {
        vesper_models::inspect(&state.config, &state.models)
    }
    #[tauri::command]
    fn list_model_installations() -> Value {
        json!([])
    }
    #[tauri::command]
    fn list_audio_devices() -> Value {
        json!([{"id":"public-pcm","name":"Public PCM replay (no microphone)","isDefault":true}])
    }
    #[tauri::command]
    fn get_audio_features() -> Value {
        json!({"level":0.0,"pitchHz":null})
    }
    #[tauri::command]
    async fn edit_latest_draft(
        state: tauri::State<'_, Arc<Probe>>,
        result_id: String,
        revision: u64,
        text: String,
    ) -> Result<PipelineResult, String> {
        let probe = Arc::clone(&state);
        let case = probe.current()?;
        if case.arm != 1
            || probe.samples[case.sample].phase != "feedback"
            || text != probe.samples[case.sample].reference
            || case.row["resultId"].as_str() != Some(result_id.as_str())
        {
            return Err("Only the declared calibration reference may supply feedback.".into());
        }
        tauri::async_runtime::spawn_blocking(move || {
            let work = &case.work;
            let started = Instant::now();
            let result = work.runtime.edit_latest_draft(
                work.dataset_root.clone(),
                &result_id,
                revision,
                text,
            );
            if result.is_ok() {
                work.runtime.schedule_semantic_shadow(
                    work.dataset_root.clone(),
                    work.config.clone(),
                    work.model_root.clone(),
                    &work.inference,
                );
            }
            let mut sequence = probe.sequence.lock().map_err(|e| e.to_string())?;
            let current = sequence.current.as_mut().ok_or("Missing request")?;
            if current.work.operation_id != case.work.operation_id {
                return Err("Stale edit".into());
            }
            current.row["editBackendMs"] = json!(started.elapsed().as_secs_f64() * 1000.0);
            match &result {
                Ok(updated) => current.row["learningOutcome"] = json!(updated.learning_outcome),
                Err(error) => current.row["editError"] = json!(error),
            }
            result
        })
        .await
        .map_err(|e| e.to_string())?
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct UiMetrics {
        operation_id: String,
        result_id: Option<String>,
        stop_to_dom_ms: Option<f64>,
        stop_to_render_opportunity_ms: Option<f64>,
        edit_submit_ms: Option<f64>,
        visible: bool,
        rendered_text: Option<String>,
        error: Option<String>,
    }
    #[tauri::command]
    fn finish_case(state: tauri::State<'_, Arc<Probe>>, metrics: UiMetrics) -> Result<(), String> {
        let mut sequence = state.sequence.lock().map_err(|e| e.to_string())?;
        let case = sequence.current.as_ref().ok_or("Missing request")?;
        if case.work.operation_id != metrics.operation_id {
            return Err("Stale UI acknowledgement".into());
        }
        for value in [
            metrics.stop_to_dom_ms,
            metrics.stop_to_render_opportunity_ms,
            metrics.edit_submit_ms,
        ]
        .into_iter()
        .flatten()
        {
            if !value.is_finite() || !(0.0..=1_200_000.0).contains(&value) {
                return Err("Invalid UI timing".into());
            }
        }
        let mut row = case.row.clone();
        let store =
            DatasetStore::open(case.work.dataset_root.clone()).map_err(|e| e.to_string())?;
        let profile = store.personalization().map_err(|e| e.to_string())?;
        let learning = store
            .automatic_learning_state()
            .map_err(|e| e.to_string())?;
        row["frozenUnchanged"] = json!(
            case.arm != 0
                || (profile.vocabulary.is_empty()
                    && learning.candidates.is_empty()
                    && learning.events.is_empty())
        );
        row["archiveEmpty"] = json!(
            store.list_sessions().map_err(|e| e.to_string())?.is_empty()
                && fs::read_dir(case.work.dataset_root.join("audio"))
                    .map_err(|e| e.to_string())?
                    .count()
                    == 0
        );
        row["activeMemories"] = json!(profile.vocabulary.iter().filter(|v| v.enabled).count());
        row["domMatches"] = json!(
            metrics
                .rendered_text
                .as_deref()
                .is_some_and(|text| row["output"].as_str() == Some(text))
                && metrics.result_id.as_deref() == row["resultId"].as_str()
        );
        row["stopToDomMs"] = json!(metrics.stop_to_dom_ms);
        row["stopToRenderOpportunityMs"] = json!(metrics.stop_to_render_opportunity_ms);
        row["editSubmitMs"] = json!(metrics.edit_submit_ms);
        row["visible"] = json!(metrics.visible);
        row["uiError"] = json!(metrics.error);
        state.write(&row)?;
        println!("completed replay {} {}", row["sampleId"], row["arm"]);
        sequence.current = None;
        Ok(())
    }
    #[tauri::command]
    fn finish_probe(
        state: tauri::State<'_, Arc<Probe>>,
        app: tauri::AppHandle,
        error: Option<String>,
    ) -> Result<(), String> {
        let sequence = state.sequence.lock().map_err(|e| e.to_string())?;
        let complete = error.is_none()
            && sequence.current.is_none()
            && sequence.next == state.samples.len() * 2;
        // Native handles can retain managed state beyond the event loop. Release
        // native weights and worker-owned stores before Metal's globals exit.
        let mut runtimes = state.runtimes.lock().map_err(|e| e.to_string())?;
        for pair in &*runtimes {
            for runtime in pair {
                runtime.prepare_exit()?;
            }
        }
        state.inference.unload().map_err(|e| e.to_string())?;
        runtimes.clear();
        fs::remove_dir_all(state._temporary.path()).map_err(|e| e.to_string())?;
        state.write(&json!({"kind":"complete","complete":complete,"error":error}))?;
        SUCCESS.store(complete, std::sync::atomic::Ordering::Relaxed);
        app.exit(if complete { 0 } else { 1 });
        Ok(())
    }

    pub fn run() -> Result<(), Box<dyn std::error::Error>> {
        let manifest: Value = serde_json::from_str(MANIFEST)?;
        let samples: Vec<Sample> = serde_json::from_value(manifest["samples"].clone())?;
        if samples.len() != 38 {
            return Err("Expected the fixed 38-sample manifest".into());
        }
        let audio = PathBuf::from(
            std::env::var_os("TRANSCRIBE_LEARNING_AUDIO_ROOT")
                .ok_or("Set TRANSCRIBE_LEARNING_AUDIO_ROOT")?,
        );
        let models = PathBuf::from(
            std::env::var_os("TRANSCRIBE_MODEL_DIR").ok_or("Set TRANSCRIBE_MODEL_DIR")?,
        );
        let output = PathBuf::from(
            std::env::var_os("TRANSCRIBE_WEBVIEW_REPORT")
                .ok_or("Set a new TRANSCRIBE_WEBVIEW_REPORT path")?,
        );
        let config = AppConfig {
            language: Language::English,
            collect_training_data: false,
            ..Default::default()
        };
        vesper_models::verify(&config, &models, &AtomicBool::new(false))?;
        let mut pcm = vec![];
        let mut hashes = std::collections::BTreeSet::new();
        for (index, sample) in samples.iter().enumerate() {
            if (index % 2 == 0 && sample.phase != "feedback")
                || (index % 2 == 1
                    && (sample.phase != "audit" || sample.cohort != samples[index - 1].cohort))
            {
                return Err("Invalid calibration/audit ordering".into());
            }
            let path = audio.join(&sample.audio_path);
            if !hashes.insert(&sample.sha256)
                || format!("{:x}", Sha256::digest(fs::read(&path)?)) != sample.sha256
            {
                return Err("Duplicate or mismatched public audio".into());
            }
            let decoded = vesper_audio::decode_file(path, 0)?;
            if (decoded.duration_seconds() - sample.duration_seconds).abs() >= 0.01 {
                return Err("Audio duration mismatch".into());
            }
            pcm.push(decoded.samples().to_vec());
        }
        let temporary = tempfile::tempdir()?;
        let mut roots = vec![];
        let mut runtimes = vec![];
        for index in 0..samples.len() / 2 {
            let pair = [
                temporary.path().join(format!("{index}-frozen")),
                temporary.path().join(format!("{index}-learning")),
            ];
            for root in &pair {
                DatasetStore::open(root.clone())?.dictation_personalization_snapshot()?;
            }
            roots.push(pair);
            runtimes.push([Arc::new(Runtime::default()), Arc::new(Runtime::default())]);
        }
        let state = Arc::new(Probe {
            _temporary: temporary,
            roots,
            runtimes: Mutex::new(runtimes),
            samples,
            pcm,
            config,
            models,
            inference: Arc::new(vesper_inference::InferenceManager::new()?),
            sequence: Mutex::new(Sequence {
                next: 0,
                current: None,
            }),
            log: Mutex::new(
                OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(output)?,
            ),
        });
        state.write(&json!({"kind":"header","schemaVersion":1,"expectedRequests":76,
            "learningPolicyVersion":vesper_core::LEARNING_POLICY_VERSION,
            "manifestSha256":format!("{:x}",Sha256::digest(MANIFEST)),"config":state.config,
            "modelProvenance":vesper_models::provenance(&state.config),"modelVersions":vesper_inference::model_versions(&state.config),
            "evidenceClass":"native WKWebView and production panel/runtime; prerecorded public PCM and scripted edits",
            "microphoneStopToVisibleMs":null,"humanEditingTimeMs":null}))?;
        let mut context = tauri::generate_context!();
        context.config_mut().identifier = "com.transcribe.learning-qualification".into();
        context.config_mut().build.dev_url = Some(URL.parse()?);
        context.config_mut().app.windows.clear();
        let started = Instant::now();
        let temporary_path = state._temporary.path().to_owned();
        let state_for_exit = Arc::clone(&state);
        let app = tauri::Builder::default()
            .manage(state)
            .invoke_handler(tauri::generate_handler![
                prepare_case,
                start_recording,
                stop_recording,
                get_runtime_status,
                get_latest_result,
                get_model_installation,
                list_model_installations,
                list_audio_devices,
                get_audio_features,
                edit_latest_draft,
                finish_case,
                finish_probe
            ])
            .setup(|app| {
                tauri::WebviewWindowBuilder::new(
                    app,
                    "main",
                    tauri::WebviewUrl::App("tests/native-learning.html".into()),
                )
                .title("Vesper — public audio qualification (microphone off)")
                .inner_size(1050.0, 1000.0)
                .build()?;
                Ok(())
            })
            .build(context)?;
        let watchdog_app = app.handle().clone();
        let (stop_watchdog, watchdog) = std::sync::mpsc::channel();
        let watchdog_thread = std::thread::spawn(move || {
            if matches!(
                watchdog.recv_timeout(std::time::Duration::from_secs(1200)),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout)
            ) {
                eprintln!("Native qualification timed out; retain the incomplete report.");
                watchdog_app.exit(1);
            }
        });
        let exit_code = app.run_return(move |_, event| {
            if matches!(event, tauri::RunEvent::Exit) {
                let _ = state_for_exit
                    .write(&json!({"kind":"exit","wallMs":started.elapsed().as_secs_f64()*1000.0}));
            }
        });
        let _ = stop_watchdog.send(());
        let _ = watchdog_thread.join();
        if temporary_path.exists() {
            return Err("Temporary replay stores were not cleaned up".into());
        }
        if exit_code != 0 || !SUCCESS.load(std::sync::atomic::Ordering::Relaxed) {
            return Err("Native replay incomplete; retain its report".into());
        }
        Ok(())
    }
}

#[cfg(all(target_os = "macos", not(test)))]
fn main() {
    if let Err(error) = probe::run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
#[cfg(any(not(target_os = "macos"), test))]
fn main() {}
