use super::*;
use std::time::Instant;

fn context(root: &Path, model_root: PathBuf) -> WorkContext {
    WorkContext {
        operation_id: uuid::Uuid::new_v4().to_string(),
        config: AppConfig {
            cleanup_enabled: false,
            compute_backend: vesper_core::ComputeBackend::Cpu,
            ..AppConfig::default()
        },
        model_root,
        dataset_root: root.join("dataset"),
        runtime: Arc::new(Runtime::default()),
        inference: Arc::new(vesper_inference::InferenceManager::new().unwrap()),
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
fn automatic_speakers_preserve_raw_when_unavailable_corrupt_or_cancelled() {
    let dir = tempfile::tempdir().unwrap();
    for state in ["missing", "corrupt", "unsupported"] {
        let models = dir.path().join(state).join("models");
        if state != "missing" {
            use vesper_models::diarization as diar;
            let weights = models.join(diar::artifact().relative_path);
            std::fs::create_dir_all(weights.parent().unwrap()).unwrap();
            std::fs::write(weights, b"corrupt weights must never be loaded").unwrap();
            let bundle = models.join(diar::runtime_relative());
            std::fs::create_dir_all(&bundle).unwrap();
            std::fs::write(bundle.join("manifest.json"), b"{}").unwrap();
            for part in ["api", "asr", "ggml", "base", "cpu", "blas", "metal"] {
                std::fs::write(
                    bundle.join(format!("{}{part}.dylib", diar::LIBRARY_PREFIX)),
                    b"fixture",
                )
                .unwrap();
            }
        }
        for cancel in [false, true] {
            let mut ctx = context(dir.path(), models.clone());
            if state == "unsupported" {
                ctx.config.models.asr = vesper_core::AsrModel::Nemotron35AsrStreaming06b;
                ctx.config.compute_backend = vesper_core::ComputeBackend::Vulkan;
            }
            let captured = vesper_audio::CapturedAudio::new(vec![0.1; 16000], 16000, 1).unwrap();
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
                let attempted = state == "corrupt" && vesper_models::diarization::supported();
                assert_eq!(
                    result
                        .warnings
                        .iter()
                        .any(|s| s.contains("Speaker analysis did not finish")),
                    attempted
                );
                assert_eq!(
                    ctx.inference.execution_evidence(&ctx.operation_id)["diarization"].is_null(),
                    !attempted
                );
            }
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
    let mut ctx = context(dir.path(), models);
    // Keep the observed Cohere meeting failure in the matrix. The incomplete
    // ASR guard must fire before speaker analysis for either retired preference.
    for legacy_setting in [false, true] {
        ctx.operation_id = uuid::Uuid::new_v4().to_string();
        let mut stored = serde_json::to_value(&ctx.config).unwrap();
        stored["speakerDiarization"] = legacy_setting.into();
        ctx.config = serde_json::from_value(stored).unwrap();
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
                "sample":"ami-675-705", "asr":"Cohere", "legacySpeakerSetting":legacy_setting,
                "error":error, "diarizationStarted":false
            })
        );
    }
    // Old false settings must not suppress automatic activity with installed files.
    let mut stored = serde_json::to_value(&ctx.config).unwrap();
    stored["speakerDiarization"] = false.into();
    ctx.config = serde_json::from_value(stored).unwrap();
    use vesper_core::{AsrModel, ComputeBackend};
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
        ctx.runtime
            .begin("transcribing", "paired baseline")
            .unwrap();
        let baseline_start = Instant::now();
        // Compare with the same ASR worker directly; there is no production
        // disable switch. This baseline excludes desktop persistence/analysis.
        let decoded = vesper_audio::decode_file(&audio, 0).unwrap();
        let baseline = ctx
            .inference
            .transcribe(
                &decoded,
                &ctx.config,
                &ctx.model_root,
                &ctx.operation_id,
                Arc::clone(&ctx.runtime.cancelled),
            )
            .unwrap();
        let baseline_seconds = baseline_start.elapsed().as_secs_f64();
        ctx.runtime.finish(&Ok::<(), String>(()));
        ctx.operation_id = uuid::Uuid::new_v4().to_string();
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
        assert_eq!(result.raw_transcript, baseline);
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
            "seconds":pipeline_seconds,"decodeAndAsrSeconds":baseline_seconds,"speakers":speakers.len(),
            "rawAsrUnchanged":true,"clipped":timeline.clipped_segments,
            "execution":session.model_versions["execution"]})
        );
    }
    // Collection-off still returns local speaker analysis, without an archive.
    let private = dir.path().join("no-collection");
    let mut unsaved = context(&private, ctx.model_root.clone());
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
