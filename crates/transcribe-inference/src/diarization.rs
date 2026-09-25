use crate::{check_cancelled, native, Result};
use std::{path::Path, sync::atomic::AtomicBool};
use transcribe_audio::MonoPcm;
use transcribe_core::{AppConfig, ComputeBackend, SpeakerTimeline};
use transcribe_models::diarization as models;

pub fn analyze(
    audio: &MonoPcm,
    config: &AppConfig,
    root: &Path,
    cancelled: &AtomicBool,
) -> Result<SpeakerTimeline> {
    check_cancelled(cancelled)?;
    audio.require_sample_rate(16000)?;
    models::verify(root, cancelled).map_err(crate::resident_sherpa::verification_error)?;
    let model_path = root.join(models::artifact().relative_path);
    let library = models::library_path(root);
    let (gpu, mut provider) = match config.compute_backend {
        ComputeBackend::Auto | ComputeBackend::Metal => (0, "metal"),
        ComputeBackend::Cpu => (-1, "cpu"),
        _ => {
            return Err(native(
                "speaker analysis",
                "this experimental runtime supports CPU and Metal",
            ))
        }
    };
    check_cancelled(cancelled)?;
    let loaded = transcribe_nemo::diarization::Diarizer::load(&library, &model_path, gpu);
    let mut fallback_reason = None;
    let mut model = match loaded {
        Ok(model) => model,
        Err(error) if config.compute_backend == ComputeBackend::Auto => {
            check_cancelled(cancelled)?;
            provider = "cpu";
            fallback_reason = Some(error.to_string());
            transcribe_nemo::diarization::Diarizer::load(&library, &model_path, -1).map_err(
                |e| {
                    native(
                        "speaker initialization",
                        format!("Metal failed ({error}); CPU retry failed ({e})"),
                    )
                },
            )?
        }
        Err(e) => return Err(native("speaker initialization", e)),
    };
    check_cancelled(cancelled)?;
    let result = model.diarize(audio.samples(), 16000, || {
        cancelled.load(std::sync::atomic::Ordering::Relaxed)
    });
    check_cancelled(cancelled)?;
    let rows = result
        .map_err(|e| native("speaker analysis", e))?
        .into_iter()
        .map(|r| (r.start, r.end, r.speaker))
        .collect::<Vec<_>>();
    let mut timeline = SpeakerTimeline::from_native(
        audio.duration_seconds(),
        &rows,
        models::MODEL_REVISION,
        models::RUNTIME_REVISION,
        provider,
    )
    .map_err(|e| native("speaker output validation", e))?;
    timeline.fallback_reason = fallback_reason;
    Ok(timeline)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{atomic::Ordering, Arc};

    #[test]
    fn cancellation_precedes_artifact_access() {
        let audio = MonoPcm::new(vec![0.0; 160], 16000).unwrap();
        assert!(matches!(
            analyze(
                &audio,
                &AppConfig::default(),
                Path::new("missing"),
                &AtomicBool::new(true)
            ),
            Err(crate::InferenceError::Cancelled)
        ));
    }

    #[test]
    #[ignore = "requires the pinned namespaced native bundle, GA weights, existing Nemotron ASR and public samples"]
    fn real_worker_speaker_samples_and_native_asr_coexistence() {
        let root = std::path::PathBuf::from(
            std::env::var_os("TRANSCRIBE_DIAR_MODELS").expect("TRANSCRIBE_DIAR_MODELS"),
        );
        let corpus = std::path::PathBuf::from(
            std::env::var_os("TRANSCRIBE_DIAR_CORPUS").expect("TRANSCRIBE_DIAR_CORPUS"),
        );
        let audio = transcribe_audio::decode_file(corpus.join("fleurs-single.wav"), 0).unwrap();
        let flag = Arc::new(AtomicBool::new(false));
        let mut config = AppConfig {
            compute_backend: ComputeBackend::Cpu,
            cleanup_enabled: false,
            speaker_diarization: true,
            ..AppConfig::default()
        };
        config.models.asr = transcribe_core::AsrModel::Nemotron35AsrStreaming06b;
        // Existing installed ASR bundle is the macOS Metal release; the separate
        // diarizer still exercises both CPU and Metal below.
        let mut asr_config = config.clone();
        asr_config.compute_backend = ComputeBackend::Metal;
        let reverse = std::env::var_os("TRANSCRIBE_DIAR_FIRST").is_some();
        // Run this test in two fresh processes, with and without DIAR_FIRST;
        // changing order inside one process would not prove loader isolation.
        {
            let manager = crate::InferenceManager::new().unwrap();
            if reverse {
                let t = manager
                    .analyze_speakers(&audio, &config, &root, "diar-first", Arc::clone(&flag))
                    .unwrap();
                assert_eq!(t.segments.iter().map(|s| s.speaker).max(), Some(1));
            }
            let before = manager
                .transcribe(&audio, &asr_config, &root, "asr-before", Arc::clone(&flag))
                .unwrap();
            assert!(!before.trim().is_empty());
            for backend in [ComputeBackend::Cpu, ComputeBackend::Metal] {
                config.compute_backend = backend;
                for (name, expected) in [
                    ("ami-600-645", 4),
                    ("ami-675-705", 4),
                    ("fleurs-single", 1),
                    ("silence", 0),
                ] {
                    let sample =
                        transcribe_audio::decode_file(corpus.join(format!("{name}.wav")), 0)
                            .unwrap();
                    let start = std::time::Instant::now();
                    let timeline = manager
                        .analyze_speakers(&sample, &config, &root, name, Arc::clone(&flag))
                        .unwrap();
                    timeline.validate().unwrap();
                    let speakers: std::collections::BTreeSet<_> =
                        timeline.segments.iter().map(|s| s.speaker).collect();
                    assert_eq!(speakers.len(), expected, "{name} {backend:?}");
                    if name == "ami-600-645" {
                        assert!(timeline.clipped_segments > 0);
                    }
                    println!(
                        "diar_sample={}",
                        serde_json::json!({"reverseLoadOrder":reverse,
                        "sample":name,"backend":backend,"seconds":start.elapsed().as_secs_f64(),
                        "speakers":speakers.len(),"segments":timeline.segments.len(),
                        "clipped":timeline.clipped_segments})
                    );
                }
            }
            let after = manager
                .transcribe(&audio, &asr_config, &root, "asr-after", Arc::clone(&flag))
                .unwrap();
            assert_eq!(
                before, after,
                "native runtime coexistence changed ASR output"
            );
            flag.store(true, Ordering::Relaxed);
            assert!(matches!(
                manager.analyze_speakers(&audio, &config, &root, "cancelled", Arc::clone(&flag)),
                Err(crate::InferenceError::Cancelled)
            ));
            flag.store(false, Ordering::Relaxed);
            let reset = manager
                .analyze_speakers(&audio, &config, &root, "after-cancel", Arc::clone(&flag))
                .unwrap();
            assert_eq!(reset.segments.iter().map(|s| s.speaker).max(), Some(1));
        }
        // Cancel inside actual packet delivery, then prove fresh stream isolation.
        models::verify(&root, &flag).unwrap();
        let mut model = transcribe_nemo::diarization::Diarizer::load(
            &models::library_path(&root),
            &root.join(models::artifact().relative_path),
            -1,
        )
        .unwrap();
        let calls = std::cell::Cell::new(0);
        let cancelled = model.diarize(audio.samples(), 16000, || {
            calls.set(calls.get() + 1);
            calls.get() >= 3
        });
        assert!(cancelled.is_err());
        let recovered = model.diarize(audio.samples(), 16000, || false).unwrap();
        assert_eq!(recovered.iter().map(|s| s.speaker).max(), Some(1));
    }
}
