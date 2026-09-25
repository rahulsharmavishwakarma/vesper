use super::*;
use std::time::Instant;

fn context(root: &Path, model_root: PathBuf, speakers: bool) -> WorkContext {
    WorkContext {
        operation_id: uuid::Uuid::new_v4().to_string(),
        config: AppConfig {
            speaker_diarization: speakers,
            cleanup_enabled: false,
            compute_backend: transcribe_core::ComputeBackend::Cpu,
            ..AppConfig::default()
        },
        model_root,
        dataset_root: root.join("dataset"),
        runtime: Arc::new(Runtime::default()),
        inference: Arc::new(transcribe_inference::InferenceManager::new().unwrap()),
        streaming_result: Arc::new(Mutex::new(None)),
        personalization: Arc::default(),
    }
}

fn assert_no_recordings(root: &Path) {
    // Automatic text memory may create an empty audio directory even with
    // collection disabled. Assert the privacy property: no sessions or audio.
    let store = DatasetStore::open(root.to_path_buf()).unwrap();
    assert!(store.list_sessions().unwrap().is_empty());
    assert_eq!(std::fs::read_dir(root.join("audio")).unwrap().count(), 0);
}

#[test]
fn missing_diarizer_preserves_durable_raw_and_cancellation_does_not_erase_it() {
    let dir = tempfile::tempdir().unwrap();
    for cancel in [false, true] {
        let ctx = context(dir.path(), dir.path().join("missing-models"), true);
        let captured = transcribe_audio::CapturedAudio::new(vec![0.1; 16000], 16000, 1).unwrap();
        let mut session = ctx.archive_capture(&captured).unwrap();
        let audio = MonoPcm::new(vec![0.1; 16000], 16000).unwrap();
        ctx.runtime.cancelled.store(cancel, Ordering::Relaxed);
        let result = ctx.complete_raw(
            &audio,
            &mut session,
            "Preserve these original words.".into(),
        );
        assert_eq!(
            session.as_ref().unwrap().raw_transcript.as_deref(),
            Some("Preserve these original words.")
        );
        if cancel {
            assert!(result.is_err());
        } else {
            let result = result.unwrap();
            assert!(result.speaker_timeline.is_none());
            assert_eq!(result.raw_transcript, "Preserve these original words.");
            assert!(result
                .warnings
                .iter()
                .any(|s| s.contains("Speaker analysis did not finish")));
        }
    }
}

#[test]
#[ignore = "requires the pinned local diarization runtime, Cohere ASR and public meeting samples"]
fn real_asr_speaker_pipeline_persists_and_exports_without_changing_words() {
    let models =
        PathBuf::from(std::env::var_os("TRANSCRIBE_DIAR_MODELS").expect("TRANSCRIBE_DIAR_MODELS"));
    let corpus =
        PathBuf::from(std::env::var_os("TRANSCRIBE_DIAR_CORPUS").expect("TRANSCRIBE_DIAR_CORPUS"));
    let dir = tempfile::tempdir().unwrap();
    let mut ctx = context(dir.path(), models, false);
    // Keep the observed Cohere meeting failure in the matrix. The incomplete
    // ASR guard must fire equally with speakers off/on, before speaker analysis.
    for enabled in [false, true] {
        ctx.operation_id = uuid::Uuid::new_v4().to_string();
        ctx.config.speaker_diarization = enabled;
        ctx.runtime
            .begin("transcribing", "Cohere meeting control")
            .unwrap();
        let error = ctx
            .process_file(&corpus.join("ami-675-705.wav"))
            .unwrap_err();
        ctx.runtime.finish(&Err::<(), String>(error.clone()));
        assert!(error.contains("duration_token_limit"), "{error}");
        assert!(ctx.inference.execution_evidence(&ctx.operation_id)["diarization"].is_null());
        println!(
            "asr_control_failure={}",
            serde_json::json!({
                "sample":"ami-675-705", "asr":"Cohere", "speakersEnabled":enabled,
                "error":error, "diarizationStarted":false
            })
        );
    }
    use transcribe_core::{AsrModel, ComputeBackend};
    for (name, expected, asr) in [
        ("fleurs-single", 1, AsrModel::CohereTranscribe),
        ("fleurs-single", 1, AsrModel::Nemotron35AsrStreaming06b),
        ("ami-675-705", 4, AsrModel::Nemotron35AsrStreaming06b),
        ("ami-600-645", 4, AsrModel::Nemotron35AsrStreaming06b),
    ] {
        ctx.config.models.asr = asr;
        ctx.config.compute_backend = if asr == AsrModel::CohereTranscribe {
            ComputeBackend::Cpu
        } else {
            ComputeBackend::Metal
        };
        let audio = corpus.join(format!("{name}.wav"));
        ctx.operation_id = uuid::Uuid::new_v4().to_string();
        ctx.config.speaker_diarization = false;
        ctx.runtime
            .begin("transcribing", "paired baseline")
            .unwrap();
        let baseline_start = Instant::now();
        let baseline = ctx.process_file(&audio).unwrap();
        let baseline_seconds = baseline_start.elapsed().as_secs_f64();
        ctx.runtime.finish(&Ok::<(), String>(()));
        ctx.operation_id = uuid::Uuid::new_v4().to_string();
        ctx.config.speaker_diarization = true;
        ctx.runtime
            .begin("transcribing", "speaker pipeline")
            .unwrap();
        ctx.runtime
            .diagnostics
            .begin(&ctx.operation_id, &ctx.config, false);
        let start = Instant::now();
        let result = ctx.process_file(&audio).unwrap();
        let pipeline_seconds = start.elapsed().as_secs_f64();
        ctx.runtime.finish(&Ok::<(), String>(()));
        assert_eq!(result.raw_transcript, baseline.raw_transcript);
        assert!(result.warnings.is_empty(), "{:?}", result.warnings);
        let timeline = result.speaker_timeline.as_ref().unwrap();
        timeline.validate().unwrap();
        let speakers: std::collections::BTreeSet<_> =
            timeline.segments.iter().map(|s| s.speaker).collect();
        assert_eq!(speakers.len(), expected);
        let session = result.session.as_ref().unwrap();
        assert_eq!(session.speaker_timeline, result.speaker_timeline);
        assert_eq!(
            session.raw_transcript.as_deref(),
            Some(result.raw_transcript.as_str())
        );
        let store = DatasetStore::open(ctx.dataset_root.clone()).unwrap();
        let restored = store
            .list_sessions()
            .unwrap()
            .into_iter()
            .find(|s| s.id == session.id)
            .unwrap();
        assert_eq!(restored.speaker_timeline, result.speaker_timeline);
        let export = dir.path().join(format!("export-{}", session.id));
        store.export_dataset(&export, false).unwrap();
        let entries = std::fs::read_to_string(export.join("manifest.jsonl")).unwrap();
        let row: serde_json::Value = entries
            .lines()
            .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap())
            .find(|row| row["id"] == session.id)
            .unwrap();
        assert_eq!(
            row["speakerTimeline"],
            serde_json::to_value(timeline).unwrap()
        );
        assert!(row["asrTarget"].is_null());
        println!(
            "wired_speaker_pipeline={}",
            serde_json::json!({"sample":name,"asr":asr.id(),
            "seconds":pipeline_seconds,"baselineSeconds":baseline_seconds,"speakers":speakers.len(),
            "rawAsrUnchanged":true,"clipped":timeline.clipped_segments,
            "execution":session.model_versions["execution"]})
        );
    }
    // Collection-off still returns local speaker analysis, without an archive.
    let private = dir.path().join("no-collection");
    let mut unsaved = context(&private, ctx.model_root.clone(), true);
    unsaved.config.collect_training_data = false;
    unsaved
        .runtime
        .begin("transcribing", "collection off")
        .unwrap();
    let result = unsaved
        .process_file(&corpus.join("fleurs-single.wav"))
        .unwrap();
    unsaved.runtime.finish(&Ok::<(), String>(()));
    assert!(result.speaker_timeline.is_some());
    assert!(result.session.is_none());
    assert_no_recordings(&unsaved.dataset_root);
    unsaved.config.cleanup_enabled = true;
    unsaved
        .runtime
        .begin("transcribing", "speaker and cleanup")
        .unwrap();
    let cleaned = unsaved
        .process_file(&corpus.join("fleurs-single.wav"))
        .unwrap();
    unsaved.runtime.finish(&Ok::<(), String>(()));
    assert_eq!(result.raw_transcript, cleaned.raw_transcript);
    assert!(cleaned.speaker_timeline.is_some());
    assert!(cleaned
        .cleaned_transcript
        .as_ref()
        .is_some_and(|s| !s.trim().is_empty()));
    assert!(cleaned.warnings.is_empty(), "{:?}", cleaned.warnings);
    assert!(cleaned.session.is_none());
    assert_no_recordings(&unsaved.dataset_root);
}
