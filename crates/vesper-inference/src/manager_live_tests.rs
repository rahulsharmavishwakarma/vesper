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

/// #153: long completed recordings must keep every part on CPU Parakeet.
/// Prints final text per public recording; scoring happens outside the test.
#[test]
#[ignore = "requires installed Parakeet artifacts and VESPER_LONG_FINAL_FIXTURES of public 16 kHz WAVs"]
fn installed_parakeet_long_final() {
    let root = PathBuf::from(std::env::var("VESPER_MODEL_DIR").expect("VESPER_MODEL_DIR"));
    let manager = InferenceManager::new().unwrap();
    let mut config = AppConfig {
        compute_backend: ComputeBackend::Cpu,
        language: Language::English,
        ..Default::default()
    };
    config.models.asr = AsrModel::ParakeetTdt06bV3;
    for path in std::env::var("VESPER_LONG_FINAL_FIXTURES")
        .expect("VESPER_LONG_FINAL_FIXTURES")
        .split(':')
    {
        let audio = vesper_audio::decode_file(path, 0).unwrap();
        let started = Instant::now();
        let text = manager
            .transcribe(
                &audio,
                &config,
                &root,
                "long",
                Arc::new(AtomicBool::new(false)),
            )
            .unwrap();
        println!(
            "parakeet-long-final-v1 {}",
            serde_json::json!({"path": path, "audioSeconds": audio.duration_seconds(),
                "ms": started.elapsed().as_millis(), "words": text.split_whitespace().count(), "text": text})
        );
    }
}

/// Streams a whole public recording in real time and reports draft coverage,
/// preview compute and live evidence per adapter. Opt-in measurement only.
#[test]
#[ignore = "requires installed pinned ASR artifacts and VESPER_LIVE_LONG_FIXTURE public 16 kHz WAV"]
fn installed_live_long_recording_draft_coverage() {
    let root = PathBuf::from(std::env::var("VESPER_MODEL_DIR").expect("VESPER_MODEL_DIR"));
    let audio = vesper_audio::decode_file(
        std::env::var("VESPER_LIVE_LONG_FIXTURE").expect("VESPER_LIVE_LONG_FIXTURE"),
        0,
    )
    .unwrap();
    assert_eq!(audio.sample_rate(), 16000);
    let mut paths = vec![
        (AsrModel::CohereTranscribe, ComputeBackend::Cpu),
        (AsrModel::Qwen3Asr06b, ComputeBackend::Cpu),
        (AsrModel::ParakeetTdt06bV3, ComputeBackend::Cpu),
    ];
    if cfg!(feature = "mlx") {
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
        let seen = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&seen);
        let session = manager
            .start_stream(
                &config,
                &root,
                "long",
                flag(),
                Arc::new(move |event| sink.lock().unwrap().push(event)),
            )
            .unwrap();
        let start = Instant::now();
        for (index, chunk) in audio.samples().chunks(320).enumerate() {
            session.push(chunk.to_vec(), 16000).unwrap();
            std::thread::sleep(
                (start + Duration::from_millis((index as u64 + 1) * 20))
                    .saturating_duration_since(Instant::now()),
            );
        }
        let stopped = Instant::now();
        assert_eq!(
            session.finish_live().unwrap(),
            StreamingFinal::CompletedAudioRequired
        );
        let live = manager.execution_evidence("long")["live"].clone();
        let final_text = manager
            .transcribe(&audio, &config, &root, "long-final", flag())
            .unwrap();
        let stop_to_final_ms = stopped.elapsed().as_millis();
        let events = seen.lock().unwrap();
        assert!(events.windows(2).all(|w| w[0].sequence < w[1].sequence));
        // Committed text only ever grows by appending.
        assert!(events
            .windows(2)
            .all(|w| w[1].committed_text.starts_with(&w[0].committed_text)));
        let last = events.last().expect("drafts before Stop");
        let draft = format!("{}{}", last.committed_text, last.provisional_text);
        let normalize = |text: &str| {
            text.split_whitespace()
                .map(|w| {
                    w.trim_matches(|c: char| !c.is_alphanumeric())
                        .to_lowercase()
                })
                .filter(|w| !w.is_empty())
                .collect::<Vec<_>>()
        };
        let (draft_words, final_words) = (normalize(&draft), normalize(&final_text));
        eprintln!(
            "{}",
            serde_json::json!({
                "model": format!("{model:?}"), "backend": format!("{backend:?}"),
                "audioSeconds": audio.samples().len() as f64 / 16000.0,
                "drafts": events.len(), "lastDraftWords": draft_words.len(),
                "finalWords": final_words.len(), "stopToFinalMs": stop_to_final_ms,
                "lastDraft": draft, "final": final_text, "live": live,
            })
        );
    }
}
