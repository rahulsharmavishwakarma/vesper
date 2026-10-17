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
        defer_speakers: Default::default(),
        pending_speakers: Default::default(),
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
    let models = PathBuf::from(
        vesper_core::environment::var_os("VESPER_DIAR_MODELS").expect("VESPER_DIAR_MODELS"),
    );
    let corpus = PathBuf::from(
        vesper_core::environment::var_os("VESPER_DIAR_CORPUS").expect("VESPER_DIAR_CORPUS"),
    );
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

/// #141: the desktop live path. A paced 48 kHz capture streams through the
/// continuous frontend; the final reuses its enhanced audio, the text is
/// available before speaker analysis, and the deferred timeline is persisted.
#[test]
#[ignore = "requires installed Cohere ASR, audio frontend, local diarization runtime and public meeting samples"]
fn live_capture_reuses_frontend_and_defers_speakers() {
    let models = PathBuf::from(
        vesper_core::environment::var_os("VESPER_DIAR_MODELS").expect("VESPER_DIAR_MODELS"),
    );
    let corpus = PathBuf::from(
        vesper_core::environment::var_os("VESPER_DIAR_CORPUS").expect("VESPER_DIAR_CORPUS"),
    );
    for sample in ["fleurs-single.wav", "ami-600-645.wav", "fleurs-single.wav"] {
        let dir = tempfile::tempdir().unwrap();
        let mut ctx = context(dir.path(), models.clone());
        ctx.config = crate::settings::managed_audio(AppConfig {
            compute_backend: vesper_core::ComputeBackend::Auto,
            language: vesper_core::Language::English,
            ..ctx.config.clone()
        });
        let source = vesper_audio::decode_file(corpus.join(sample), 0).unwrap();
        let samples = source.samples();
        let mic: Vec<f32> = (0..samples.len() * 3)
            .map(|i| {
                let (a, f) = (i / 3, (i % 3) as f32 / 3.0);
                let b = samples.get(a + 1).copied().unwrap_or(samples[a]);
                samples[a] * (1.0 - f) + b * f
            })
            .collect();
        ctx.runtime.begin("transcribing", "test").unwrap();
        ctx.runtime.set_operation_id(&ctx.operation_id);
        let stream = ctx
            .inference
            .start_stream(
                &ctx.config,
                &ctx.model_root,
                &ctx.operation_id,
                Arc::clone(&ctx.runtime.cancelled),
                Arc::new(|_| {}),
            )
            .unwrap();
        let start = Instant::now();
        for (index, chunk) in mic.chunks(960).enumerate() {
            stream.push(chunk.to_vec(), 48000).unwrap();
            std::thread::sleep(
                (start + std::time::Duration::from_millis((index as u64 + 1) * 20))
                    .saturating_duration_since(Instant::now()),
            );
        }
        let stopped = Instant::now();
        let (sender, receiver) = mpsc::sync_channel(1);
        sender
            .send(stream.finish_live().map_err(|e| e.to_string()))
            .unwrap();
        ctx.set_streaming_result(receiver);
        let captured = vesper_audio::CapturedAudio::new(mic, 48000, 1).unwrap();
        let mut result = ctx.process_capture_deferring_speakers(captured);
        let text_ms = stopped.elapsed().as_millis();
        let evidence = ctx.inference.execution_evidence(&ctx.operation_id);
        assert_eq!(evidence["audioFrontend"]["source"], "live-session");
        let before = result.as_ref().unwrap().clone();
        assert!(before.speaker_timeline.is_none());
        assert!(!before.raw_transcript.trim().is_empty());
        let speakers = Instant::now();
        ctx.complete_deferred_speakers(&mut result);
        let speaker_ms = speakers.elapsed().as_millis();
        let after = result.unwrap();
        assert_eq!(after.raw_transcript, before.raw_transcript);
        let timeline = after.speaker_timeline.as_ref().expect("deferred timeline");
        let store = DatasetStore::open(ctx.dataset_root.clone()).unwrap();
        let stored = store
            .list_sessions()
            .unwrap()
            .into_iter()
            .find(|s| Some(&s.id) == after.session.as_ref().map(|s| &s.id));
        if let Some(stored) = stored {
            assert!(stored.speaker_timeline.is_some());
        }
        println!(
            "live-desktop-v1 {}",
            serde_json::json!({"sample": sample, "audioSeconds": source.duration_seconds(),
                "stopToTextMs": text_ms, "deferredSpeakerMs": speaker_ms,
                "speakerSegments": timeline.segments.len(), "provider": timeline.provider, "warnings": after.warnings})
        );
    }
}
