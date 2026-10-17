//! Installed-model tests use public PCM selected by the contributor, never a microphone.
use super::*;
use std::time::{Duration, Instant};
use vesper_core::{AsrModel, ComputeBackend, Language};

#[test]
#[ignore = "requires installed pinned ASR artifacts and VESPER_LIVE_FIXTURE public 16 kHz WAV"]
fn installed_live_adapters_cancel_restart_and_final_parity() {
    let root = PathBuf::from(std::env::var("VESPER_MODEL_DIR").expect("VESPER_MODEL_DIR"));
    let audio = vesper_audio::decode_file(
        std::env::var("VESPER_LIVE_FIXTURE").expect("VESPER_LIVE_FIXTURE"),
        0,
    )
    .unwrap();
    assert_eq!(audio.sample_rate(), 16000);
    assert!(audio.samples().len() >= 4 * 16000);
    let mut paths = vec![
        (AsrModel::CohereTranscribe, ComputeBackend::Cpu),
        (AsrModel::Qwen3Asr06b, ComputeBackend::Cpu),
        (AsrModel::ParakeetTdt06bV3, ComputeBackend::Cpu),
    ];
    if cfg!(feature = "mlx") {
        paths.push((AsrModel::Qwen3Asr06b, ComputeBackend::Metal));
        paths.push((AsrModel::ParakeetTdt06bV3, ComputeBackend::Metal));
    }
    for (model, backend) in paths {
        let manager = InferenceManager::new().unwrap();
        let mut config = AppConfig {
            compute_backend: backend,
            language: Language::English,
            ..Default::default()
        };
        config.models.asr = model;
        let flag = || Arc::new(AtomicBool::new(false));
        let baseline = manager
            .transcribe(&audio, &config, &root, "baseline", flag())
            .unwrap();
        assert!(!baseline.is_empty());
        let silence_updates = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&silence_updates);
        let silent = manager
            .start_stream(
                &config,
                &root,
                "silence",
                flag(),
                Arc::new(move |event| sink.lock().unwrap().push(event)),
            )
            .unwrap();
        for _ in 0..150 {
            silent.push(vec![0.0; 320], 16000).unwrap();
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(
            silent.finish_live().unwrap(),
            StreamingFinal::CompletedAudioRequired
        );
        assert!(
            silence_updates
                .lock()
                .unwrap()
                .iter()
                .all(|e| e.provisional_text.trim().is_empty()),
            "{model:?}/{backend:?}: hallucinated digital silence"
        );
        for explicit_cancel in [false, true] {
            let cancelled = flag();
            let seen = Arc::new(Mutex::new(Vec::new()));
            let sink = Arc::clone(&seen);
            let session = manager
                .start_stream(
                    &config,
                    &root,
                    "cancel",
                    Arc::clone(&cancelled),
                    Arc::new(move |event| sink.lock().unwrap().push(event)),
                )
                .unwrap();
            // Cross the first decoder start, then cancel while further PCM is pending.
            let start = Instant::now();
            for (index, chunk) in audio.samples()[..33600].chunks(320).enumerate() {
                session.push(chunk.to_vec(), 16000).unwrap();
                std::thread::sleep(
                    (start + Duration::from_millis((index as u64 + 1) * 20))
                        .saturating_duration_since(Instant::now()),
                );
            }
            if explicit_cancel {
                session.cancel().unwrap();
            } else {
                drop(session);
            }
            // A following command is also a barrier after Drop's asynchronous cancellation.
            assert_eq!(
                manager
                    .transcribe(&audio, &config, &root, "after-cancel", flag())
                    .unwrap(),
                baseline
            );
            let count = seen.lock().unwrap().len();
            std::thread::sleep(Duration::from_millis(100));
            assert_eq!(seen.lock().unwrap().len(), count, "late cancelled draft");
            assert!(
                !cancelled.load(Ordering::Acquire),
                "live-only cancellation must permit final fallback"
            );
        }
        let seen = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&seen);
        let session = manager
            .start_stream(
                &config,
                &root,
                "restart",
                flag(),
                Arc::new(move |event| sink.lock().unwrap().push(event)),
            )
            .unwrap();
        let start = Instant::now();
        for (index, chunk) in audio.samples()[..64000].chunks(320).enumerate() {
            session.push(chunk.to_vec(), 16000).unwrap();
            std::thread::sleep(
                (start + Duration::from_millis((index as u64 + 1) * 20))
                    .saturating_duration_since(Instant::now()),
            );
        }
        assert!(
            seen.lock()
                .unwrap()
                .iter()
                .any(|e| !e.provisional_text.is_empty()),
            "{model:?}/{backend:?}: no draft before Stop"
        );
        assert_eq!(
            session.finish_live().unwrap(),
            StreamingFinal::CompletedAudioRequired
        );
        assert_eq!(
            manager
                .transcribe(&audio, &config, &root, "final", flag())
                .unwrap(),
            baseline
        );
        let events = seen.lock().unwrap();
        assert!(events
            .iter()
            .all(|e| e.operation_id == "restart" && !e.is_final && e.committed_text.is_empty()));
        assert!(events.windows(2).all(|w| w[0].sequence < w[1].sequence));
        eprintln!(
            "live lifecycle passed: {model:?}/{backend:?}; {} restart drafts",
            events.len()
        );
    }
}

/// #141: a buffered recording's final reuses its live frontend session. Input is
/// upsampled to 48 kHz and paced in real time like a microphone; the final gets
/// the same capture through the app's completed-audio resampler. Compares that
/// final with the previous path (frontend re-run on the completed audio).
/// Prints texts and timings; scoring happens outside the test.
#[test]
#[ignore = "requires installed ASR and audio-frontend artifacts and VESPER_FRONTEND_PARITY_FIXTURES of public WAVs"]
fn installed_live_frontend_final_parity() {
    let root = PathBuf::from(std::env::var("VESPER_MODEL_DIR").expect("VESPER_MODEL_DIR"));
    let fixtures = std::env::var("VESPER_FRONTEND_PARITY_FIXTURES").expect("fixtures");
    let models = std::env::var("VESPER_FRONTEND_PARITY_MODELS").unwrap_or_default();
    for model in [
        AsrModel::CohereTranscribe,
        AsrModel::Qwen3Asr06b,
        AsrModel::ParakeetTdt06bV3,
    ]
    .into_iter()
    .filter(|model| models.is_empty() || models.split(',').any(|m| m == model.id()))
    {
        let manager = InferenceManager::new().unwrap();
        let mut config = AppConfig {
            compute_backend: ComputeBackend::Cpu,
            language: Language::English,
            audio_frontend: vesper_core::AudioFrontendConfig {
                vad_enabled: true,
                auto_level_enabled: false,
                noise_reduction: vesper_core::NoiseReduction::Dpdfnet8,
            },
            ..Default::default()
        };
        config.models.asr = model;
        let flag = || Arc::new(AtomicBool::new(false));
        for (index, path) in fixtures.split(':').enumerate() {
            let source = vesper_audio::decode_file(path, 0).unwrap();
            // Linear 3x upsampling: a synthetic 48 kHz microphone capture.
            let samples = source.samples();
            let mic: Vec<f32> = (0..samples.len() * 3)
                .map(|i| {
                    let (a, f) = (i / 3, (i % 3) as f32 / 3.0);
                    let b = samples.get(a + 1).copied().unwrap_or(samples[a]);
                    samples[a] * (1.0 - f) + b * f
                })
                .collect();
            let audio = vesper_audio::CapturedAudio::new(mic.clone(), 48000, 1)
                .unwrap()
                .as_mono_16k()
                .unwrap();
            let id = format!("live-{index}");
            let session = manager
                .start_stream(&config, &root, &id, flag(), Arc::new(|_| {}))
                .unwrap();
            let start = Instant::now();
            for (chunk_index, chunk) in mic.chunks(960).enumerate() {
                session.push(chunk.to_vec(), 48000).unwrap();
                std::thread::sleep(
                    (start + Duration::from_millis((chunk_index as u64 + 1) * 20))
                        .saturating_duration_since(Instant::now()),
                );
            }
            let stopped = Instant::now();
            assert_eq!(
                session.finish_live().unwrap(),
                StreamingFinal::CompletedAudioRequired
            );
            let live = manager
                .transcribe(&audio, &config, &root, &id, flag())
                .unwrap();
            let live_ms = stopped.elapsed().as_millis();
            let source = manager.execution_evidence(&id)["audioFrontend"]["source"].clone();
            let started = Instant::now();
            let old = manager
                .transcribe(&audio, &config, &root, &format!("old-{index}"), flag())
                .unwrap();
            let old_ms = started.elapsed().as_millis();
            println!(
                "live-frontend-parity-v1 {}",
                serde_json::json!({"model": format!("{model:?}"), "path": path,
                    "audioSeconds": audio.duration_seconds(), "source": source,
                    "liveFinal": live, "liveStopToFinalMs": live_ms,
                    "oldFinal": old, "oldFinalMs": old_ms})
            );
        }
    }
}
