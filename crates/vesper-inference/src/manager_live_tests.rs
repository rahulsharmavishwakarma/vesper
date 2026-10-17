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

/// Phase B gate (#139): decodes each causal committed segment plus the
/// remaining tail separately and compares it with today's whole-recording
/// final. Reports texts and timings only; scoring happens outside the test.
#[test]
#[ignore = "requires installed pinned ASR artifacts and VESPER_SEGMENTED_PARITY_MANIFEST of public 16 kHz WAVs"]
fn installed_segmented_final_parity() {
    use crate::live_preview::Mailbox;
    let root = PathBuf::from(std::env::var("VESPER_MODEL_DIR").expect("VESPER_MODEL_DIR"));
    let manifest: Vec<serde_json::Value> = serde_json::from_slice(
        &std::fs::read(
            std::env::var("VESPER_SEGMENTED_PARITY_MANIFEST")
                .expect("VESPER_SEGMENTED_PARITY_MANIFEST"),
        )
        .unwrap(),
    )
    .unwrap();
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
        for (index, recording) in manifest.iter().enumerate() {
            let mut samples = Vec::new();
            for file in recording["files"].as_array().unwrap() {
                let audio = vesper_audio::decode_file(file.as_str().unwrap(), 0).unwrap();
                assert_eq!(audio.sample_rate(), 16000);
                samples.extend_from_slice(audio.samples());
            }
            // Same causal segmentation as the live mailbox, drained after
            // every 20 ms push so no segment is dropped.
            let mailbox = Mailbox::default();
            let mut boundaries = vec![0_usize];
            for chunk in samples.chunks(320) {
                mailbox.push(chunk, 16000).unwrap();
                while let Some(snapshot) = mailbox.snapshot(u64::MAX).unwrap() {
                    boundaries.push(snapshot.end_sample as usize);
                }
            }
            boundaries.push(samples.len());
            let decode = |piece: &[f32], label: String| {
                let audio = vesper_audio::MonoPcm::new(piece.to_vec(), 16000).unwrap();
                let started = Instant::now();
                let text = manager.transcribe(&audio, &config, &root, &label, flag());
                (
                    text.map_err(|e| e.to_string()),
                    started.elapsed().as_secs_f64() * 1000.0,
                )
            };
            // Warm the resident owner so neither arm pays model load.
            let _ = decode(
                &samples[..16000.min(samples.len())],
                format!("warm-{index}"),
            );
            let (full, full_ms) = decode(&samples, format!("full-{index}"));
            let mut pieces = Vec::new();
            for (piece, pair) in boundaries.windows(2).enumerate() {
                if pair[1] > pair[0] {
                    pieces.push(decode(
                        &samples[pair[0]..pair[1]],
                        format!("piece-{index}-{piece}"),
                    ));
                }
            }
            // Candidate final policy: causal 10 s boundaries (quietest 100 ms in
            // the last second) while more than 20 s remain, then today's
            // balanced split. Everything before the last 20 s is decodable live.
            let maximum = 10 * 16000;
            let mut greedy = vec![0_usize];
            let mut start = 0;
            while start < samples.len() {
                let end = if samples.len() - start > 2 * maximum {
                    (0..=16000)
                        .step_by(160)
                        .map(|offset| start + maximum - offset)
                        .min_by(|a, b| {
                            let energy = |c: usize| -> f64 {
                                samples[c - 800..c + 800]
                                    .iter()
                                    .map(|x| f64::from(*x).powi(2))
                                    .sum()
                            };
                            energy(*a).total_cmp(&energy(*b))
                        })
                        .unwrap()
                } else {
                    vesper_audio::completed_segment_end(&samples, start, maximum)
                };
                greedy.push(end);
                start = end;
            }
            let live_cut = greedy
                .iter()
                .copied()
                .filter(|end| samples.len() - end >= 2 * maximum)
                .max()
                .unwrap_or(0);
            let mut greedy_texts = Vec::new();
            let mut stop_ms = 0.0;
            for (piece, pair) in greedy.windows(2).enumerate() {
                let decoded = decode(
                    &samples[pair[0]..pair[1]],
                    format!("greedy-{index}-{piece}"),
                );
                if pair[0] >= live_cut {
                    stop_ms += decoded.1;
                }
                greedy_texts.push(decoded.0.unwrap_or_default());
            }
            let greedy_text = greedy_texts
                .iter()
                .map(|text| text.trim())
                .filter(|text| !text.is_empty())
                .collect::<Vec<_>>()
                .join(" ");
            // Pause-first causal policy: quietest 300 ms between 4 and 10 s,
            // fixed once 10 s have arrived; the remainder decodes at Stop.
            let mut pause = vec![0_usize];
            let mut start = 0;
            while samples.len() - start > maximum {
                let energy = |c: usize| -> f64 {
                    samples[c - 2400..c + 2400]
                        .iter()
                        .map(|x| f64::from(*x).powi(2))
                        .sum()
                };
                let end = (4 * 16000..=maximum - 2400)
                    .step_by(160)
                    .map(|offset| start + offset)
                    .min_by(|a, b| energy(*a).total_cmp(&energy(*b)))
                    .unwrap();
                pause.push(end);
                start = end;
            }
            pause.push(samples.len());
            let pause_text = pause
                .windows(2)
                .enumerate()
                .filter_map(|(piece, pair)| {
                    decode(&samples[pair[0]..pair[1]], format!("pause-{index}-{piece}"))
                        .0
                        .ok()
                })
                .map(|text| text.trim().to_owned())
                .filter(|text| !text.is_empty())
                .collect::<Vec<_>>()
                .join(" ");
            let tail_ms = pieces.last().map_or(0.0, |piece| piece.1);
            let segmented = pieces
                .iter()
                .filter_map(|piece| piece.0.as_ref().ok())
                .map(|text| text.trim())
                .filter(|text| !text.is_empty())
                .collect::<Vec<_>>()
                .join(" ");
            println!(
                "segmented-final-parity-v1 {}",
                serde_json::json!({
                    "model": format!("{model:?}"), "backend": format!("{backend:?}"),
                    "id": recording["id"], "audioSeconds": samples.len() as f64 / 16000.0,
                    "boundaries": boundaries, "full": full, "fullMs": full_ms,
                    "segmented": segmented, "tailMs": tail_ms,
                    "greedyBoundaries": greedy, "greedy": greedy_text,
                    "greedyStopMs": stop_ms, "greedyLiveCut": live_cut,
                    "pauseBoundaries": pause, "pause": pause_text,
                    "pieceFailures": pieces.iter().filter(|piece| piece.0.is_err()).count(),
                    "piecesMs": pieces.iter().map(|piece| piece.1).collect::<Vec<_>>(),
                })
            );
        }
    }
}
