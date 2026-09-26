//! Resident built-in Sherpa encoder/decoder sessions, with verified load and fresh segment streams.
use crate::artifact_identity::ArtifactIdentity;
use crate::{check_cancelled, native, offline_asr, worker_threads, Result, TranscriptionProgress};
use serde::Serialize;
use sherpa_onnx::OfflineRecognizer;
use std::{
    cell::{Cell, RefCell},
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
    time::Instant,
};
use vesper_audio::MonoPcm;
use vesper_core::{AppConfig, AsrModel, ComputeBackend, Language};

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct RecognizerKey {
    root: PathBuf,
    model: AsrModel,
    artifacts: Vec<ArtifactIdentity>,
    runtime_revision: &'static str,
    runtime_build: &'static str,
    runtime_patch: &'static str,
    binding_version: &'static str,
    requested_backend: ComputeBackend,
    provider: &'static str,
    threads: i32,
    language: Language,
    precision: &'static str,
    decoding_method: &'static str,
    use_punct: bool,
    use_itn: bool,
    // Schema pins the fixed frontend/greedy/punctuation/ITN constructor defaults
    // in offline_asr::recognizer_config, alongside the exact binding/native build.
    constructor_schema: u32,
    whisper_tail_padding_frames: Option<i32>,
}

pub(crate) fn key(config: &AppConfig, root: &Path) -> Result<RecognizerKey> {
    config.validate()?;
    if (config.models.asr != AsrModel::CohereTranscribe && !config.models.asr.is_whisper())
        || config.models.custom_asr.is_some()
    {
        return Err(crate::InferenceError::UnsupportedAsr(
            config.models.asr.id(),
        ));
    }
    let root = root.canonicalize().unwrap_or_else(|_| root.to_owned());
    let artifacts = vesper_models::selected(config)
        .map_err(|e| native("resident ASR identity", e))?
        .into_iter()
        .filter(|a| a.role == "asr")
        .map(|a| ArtifactIdentity::observe(&root, a))
        .collect();
    Ok(RecognizerKey {
        root,
        model: config.models.asr,
        artifacts,
        runtime_revision: config.speech_adapter().runtime_revision,
        runtime_build: env!("TRANSCRIBE_SHERPA_BUILD_ID"),
        runtime_patch: env!("TRANSCRIBE_SHERPA_PATCH_SHA256"),
        binding_version: "1.13.8",
        requested_backend: config.compute_backend,
        provider: "cpu",
        threads: worker_threads(),
        language: config.language,
        precision: "pinned-int8-onnx",
        decoding_method: "greedy_search",
        use_punct: offline_asr::COHERE_USE_PUNCT,
        use_itn: offline_asr::COHERE_USE_ITN,
        constructor_schema: 1,
        whisper_tail_padding_frames: config
            .models
            .asr
            .is_whisper()
            .then_some(crate::whisper::TAIL_PADDING_FRAMES),
    })
}

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Metrics {
    pub request_ordinal: u64,
    pub total_us: u64,
    pub segments: Vec<offline_asr::SegmentObservation>,
}

pub(crate) struct Recognizer {
    model: OfflineRecognizer,
    identity: RecognizerKey,
    pub load_ordinal: u64,
    pub verification_us: u64,
    pub load_us: u64,
    requests: Cell<u64>,
    pub metrics: RefCell<Metrics>,
    pub healthy: Cell<bool>,
}

pub(crate) fn verification_error(error: vesper_models::ModelError) -> crate::InferenceError {
    match error {
        vesper_models::ModelError::Cancelled => crate::InferenceError::Cancelled,
        error => native("speech model verification", error),
    }
}

impl Recognizer {
    pub(crate) fn load(config: &AppConfig, root: &Path, cancelled: &AtomicBool) -> Result<Self> {
        check_cancelled(cancelled)?;
        let identity = key(config, root)?;
        let started = Instant::now();
        vesper_models::verify_asr(config, root, cancelled).map_err(verification_error)?;
        let verification_us = micros(started);
        check_cancelled(cancelled)?;
        let native_config = offline_asr::recognizer_config(config, root)?;
        let started = Instant::now();
        let model = OfflineRecognizer::create(&native_config)
            .ok_or_else(|| native("speech model load", "could not load the pinned ONNX export"))?;
        let load_us = micros(started);
        check_cancelled(cancelled)?;
        if identity != key(config, root)? {
            return Err(native(
                "resident ASR identity",
                "model files changed during verification/load",
            ));
        }
        Ok(Self {
            model,
            identity,
            load_ordinal: 0,
            verification_us,
            load_us,
            requests: Cell::new(0),
            metrics: RefCell::default(),
            healthy: Cell::new(true),
        })
    }

    pub(crate) fn transcribe(
        &self,
        audio: &MonoPcm,
        config: &AppConfig,
        root: &Path,
        cancelled: &AtomicBool,
        progress: &dyn Fn(TranscriptionProgress),
    ) -> Result<String> {
        *self.metrics.borrow_mut() = Metrics::default();
        check_cancelled(cancelled)?;
        let started = Instant::now();
        self.requests.set(self.requests.get() + 1);
        let result = offline_asr::decode_with_recognizer(
            &self.model,
            audio,
            config,
            cancelled,
            progress,
            Some(&self.healthy),
            &|observation| self.metrics.borrow_mut().segments.push(observation),
        );
        {
            let mut metrics = self.metrics.borrow_mut();
            metrics.request_ordinal = self.requests.get();
            metrics.total_us = micros(started);
        }
        // Stamps detect ordinary replacement/edit/removal, not adversarial mutation
        // between observations. Full hashes are always checked on the next load.
        if self.identity != key(config, root)? {
            self.healthy.set(false);
            check_cancelled(cancelled)?;
            return Err(native(
                "resident ASR identity",
                "model files changed during transcription",
            ));
        }
        result
    }
}

fn micros(start: Instant) -> u64 {
    start.elapsed().as_micros().min(u64::MAX as u128) as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{InferenceError, InferenceManager};
    use std::fs;
    use std::sync::{atomic::Ordering, Arc};

    fn config() -> AppConfig {
        let mut config = AppConfig::default();
        config.models.asr = AsrModel::CohereTranscribe;
        config.compute_backend = ComputeBackend::Cpu;
        config.language = Language::English;
        config
    }

    #[test]
    fn resident_whisper_key_distinguishes_variants_and_language_not_writer() {
        let root = tempfile::tempdir().unwrap();
        let mut config = config();
        config.models.asr = AsrModel::WhisperSmall;
        let small = key(&config, root.path()).unwrap();
        config.models.asr = AsrModel::WhisperTurbo;
        assert_ne!(small, key(&config, root.path()).unwrap());
        config.models.asr = AsrModel::WhisperSmall;
        config.language = Language::Auto;
        assert_ne!(small, key(&config, root.path()).unwrap());
        config.language = Language::English;
        config.cleanup_enabled = false;
        config.rewrite_quantization = vesper_core::RewriteQuantization::Q8_0;
        assert_eq!(small, key(&config, root.path()).unwrap());
        fs::create_dir(root.path().join("whisper-small")).unwrap();
        fs::write(root.path().join("whisper-small/tokens.txt"), "changed").unwrap();
        assert_ne!(small, key(&config, root.path()).unwrap());
    }

    #[test]
    fn key_tracks_files_root_and_asr_options_but_not_writer_selection() {
        let root = tempfile::tempdir().unwrap();
        let folder = root.path().join("cohere-transcribe");
        fs::create_dir(&folder).unwrap();
        let tokens = folder.join("tokens.txt");
        let config = config();
        let missing = key(&config, root.path()).unwrap();
        fs::write(&tokens, "fixture").unwrap();
        let present = key(&config, root.path()).unwrap();
        assert_ne!(missing, present);
        assert_eq!(present, key(&config, root.path()).unwrap());
        let mut changed = config.clone();
        changed.language = Language::French;
        assert_ne!(present, key(&changed, root.path()).unwrap());
        changed = config.clone();
        changed.compute_backend = ComputeBackend::Auto;
        assert_ne!(present, key(&changed, root.path()).unwrap());
        changed = config.clone();
        changed.cleanup_enabled = false;
        assert_eq!(present, key(&changed, root.path()).unwrap());
        changed.rewrite_quantization = vesper_core::RewriteQuantization::Q8_0;
        assert_eq!(present, key(&changed, root.path()).unwrap());
        assert_ne!(
            present,
            key(&config, tempfile::tempdir().unwrap().path()).unwrap()
        );
        fs::remove_file(tokens).unwrap();
        assert_eq!(missing, key(&config, root.path()).unwrap());
    }

    #[cfg(unix)]
    #[test]
    fn same_size_and_mtime_replacement_changes_inode_identity() {
        let root = tempfile::tempdir().unwrap();
        let folder = root.path().join("cohere-transcribe");
        fs::create_dir(&folder).unwrap();
        let path = folder.join("tokens.txt");
        fs::write(&path, "before").unwrap();
        let initial = key(&config(), root.path()).unwrap();
        let modified = fs::metadata(&path).unwrap().modified().unwrap();
        let replacement = folder.join("replacement");
        fs::write(&replacement, "after!").unwrap();
        fs::File::options()
            .write(true)
            .open(&replacement)
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(modified))
            .unwrap();
        fs::rename(replacement, &path).unwrap();
        assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), modified);
        assert_ne!(initial, key(&config(), root.path()).unwrap());
    }

    #[test]
    fn cancelled_corrupt_and_incompatible_loads_never_construct_a_model() {
        assert!(matches!(
            verification_error(vesper_models::ModelError::Cancelled),
            InferenceError::Cancelled
        ));
        let root = tempfile::tempdir().unwrap();
        assert!(matches!(
            Recognizer::load(&config(), root.path(), &AtomicBool::new(true)),
            Err(InferenceError::Cancelled)
        ));
        assert!(Recognizer::load(&config(), root.path(), &AtomicBool::new(false)).is_err());
        let mut incompatible = config();
        incompatible.language = Language::Auto;
        assert!(key(&incompatible, root.path()).is_err());
        incompatible = config();
        incompatible.models.asr = AsrModel::Qwen3Asr06b;
        assert!(key(&incompatible, root.path()).is_err());
    }

    fn rss_kib() -> Option<u64> {
        let out = std::process::Command::new("ps")
            .args(["-o", "rss=", "-p", &std::process::id().to_string()])
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        String::from_utf8(out.stdout).ok()?.trim().parse().ok()
    }

    #[test]
    #[ignore = "100 real Cohere lifecycle operations, Qwen and public fixtures; no downloads"]
    fn installed_cohere_residency_isolation_and_invalidation() {
        qualify_residency(AsrModel::CohereTranscribe);
    }

    #[test]
    #[ignore = "100 real Whisper Small lifecycle operations, Qwen and public fixtures; no downloads"]
    fn installed_whisper_small_residency_isolation_and_invalidation() {
        qualify_residency(AsrModel::WhisperSmall);
    }

    #[test]
    #[ignore = "100 real Whisper Turbo lifecycle operations, Qwen and public fixtures; no downloads"]
    fn installed_whisper_turbo_residency_isolation_and_invalidation() {
        qualify_residency(AsrModel::WhisperTurbo);
    }

    fn qualify_residency(asr: AsrModel) {
        let path = |name| PathBuf::from(std::env::var_os(name).expect(name));
        let root = path("TRANSCRIBE_SMOKE_MODELS");
        let read = |name| vesper_audio::decode_file(path(name), 0).unwrap();
        let a = read("TRANSCRIBE_SMOKE_AUDIO");
        let b = read("TRANSCRIBE_COHERE_AUDIO_B");
        let long = read("TRANSCRIBE_COHERE_LONG_AUDIO");
        let french = read("TRANSCRIBE_COHERE_FRENCH_AUDIO");
        assert!(long.duration_seconds() > 10.0);
        let mut config = config();
        config.models.asr = asr;
        let mut fr = config.clone();
        fr.language = Language::French;
        let fresh = AtomicBool::new(false);
        vesper_models::verify_asr(&config, &root, &fresh).unwrap();
        let cold =
            |audio, cfg: &AppConfig| offline_asr::transcribe(audio, cfg, &root, &fresh).unwrap();
        let expected_a = cold(&a, &config);
        let expected_b = cold(&b, &config);
        let expected_long = cold(&long, &config);
        let expected_fr = cold(&french, &fr);
        assert!(!expected_a.is_empty() && expected_a != expected_b);
        // The native early-error contract resets the report; a fresh stream then
        // returns A exactly. Arbitrary ORT execution exceptions remain unqualified.
        {
            let model = Recognizer::load(&config, &root, &fresh).unwrap();
            let bad = model.model.create_stream();
            if asr.is_whisper() {
                // Bypass application segmentation to exercise native oversized
                // input rejection. The following A must start with fresh state.
                let oversized = a
                    .samples()
                    .repeat(1 + (30.0 / a.duration_seconds()) as usize);
                bad.accept_waveform(16000, &oversized);
            } else {
                bad.set_option("language", "unsupported-language-fixture");
                bad.accept_waveform(16000, a.samples());
            }
            model.model.decode(&bad);
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(
                    &bad.get_option("vesper.decode_report.v1")
                )
                .unwrap()["stopReason"],
                if asr.is_whisper() {
                    "context_limit"
                } else {
                    "error"
                }
            );
            assert_eq!(
                model
                    .transcribe(&a, &config, &root, &fresh, &|_| {})
                    .unwrap(),
                expected_a
            );
        }
        let manager = InferenceManager::new().unwrap();
        let flag = || Arc::new(AtomicBool::new(false));
        let run = |audio, cfg: &AppConfig, id: &str| {
            manager.transcribe(audio, cfg, &root, id, flag()).unwrap()
        };
        let ordinal = |id: &str| {
            manager.execution_evidence(id)["asr"]["residentLoadOrdinal"]
                .as_u64()
                .unwrap()
        };
        let silence = MonoPcm::new(vec![0.0; 16000], 16000).unwrap();
        assert!(run(&silence, &config, "cold-silence").is_empty());
        assert_eq!(
            manager.execution_evidence("cold-silence")["asr"]["residentWeights"],
            false
        );
        let cancelled = flag();
        let signal = Arc::clone(&cancelled);
        let cold_started = Instant::now();
        let thread = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(20));
            signal.store(true, Ordering::Release);
        });
        let cold_cancel = manager.transcribe(&a, &config, &root, "cold-cancel", cancelled);
        thread.join().unwrap();
        assert!(matches!(cold_cancel, Err(InferenceError::Cancelled)));
        println!(
            "{}",
            serde_json::json!({"case":"resident-asr-cold-preflight", "model":asr.id(), "coldSilenceConstructedModel":false,
            "coldCancelReturnMs":cold_started.elapsed().as_secs_f64() * 1000.0})
        );
        assert_eq!(run(&a, &config, "warm"), expected_a);
        let resident_rss = rss_kib();
        let mut checkpoints = Vec::new();
        let mut cancellations = Vec::new();
        for index in 0..100 {
            let id = format!("cycle-{index}");
            match index % 10 {
                0 | 2 => assert_eq!(run(&a, &config, &id), expected_a),
                1 => assert_eq!(run(&b, &config, &id), expected_b),
                3 => assert!(run(&silence, &config, &id).is_empty()),
                4 => assert_eq!(run(&long, &config, &id), expected_long),
                5 => {
                    let mut other = config.clone();
                    other.models.asr = AsrModel::ParakeetTdt06bV3;
                    assert!(matches!(
                        manager.transcribe(&a, &other, &root, &id, Arc::new(AtomicBool::new(true))),
                        Err(InferenceError::Cancelled)
                    ));
                    continue;
                }
                6 | 7 => {
                    let cancelled = flag();
                    let signal = Arc::clone(&cancelled);
                    let audio = if index % 10 == 6 { &long } else { &a };
                    assert!(matches!(
                        manager.transcribe_with_progress(
                            audio,
                            &config,
                            &root,
                            &id,
                            cancelled,
                            Arc::new(move |_| signal.store(true, Ordering::Release))
                        ),
                        Err(InferenceError::Cancelled)
                    ));
                }
                8 => {
                    let cancelled = flag();
                    let signal = Arc::clone(&cancelled);
                    let started = Instant::now();
                    let thread = std::thread::spawn(move || {
                        std::thread::sleep(std::time::Duration::from_millis(20));
                        signal.store(true, Ordering::Release);
                    });
                    let result = manager.transcribe(&long, &config, &root, &id, cancelled);
                    thread.join().unwrap();
                    assert!(matches!(result, Err(InferenceError::Cancelled)));
                    cancellations.push(started.elapsed().as_secs_f64() * 1000.0);
                }
                9 => {
                    assert_eq!(
                        manager
                            .rewrite(&expected_a, &config, &root, flag())
                            .unwrap(),
                        expected_a
                    );
                    assert_eq!(run(&a, &config, &id), expected_a);
                    checkpoints.push(serde_json::json!({"cycles":index + 1,"rssKiB":rss_kib()}));
                }
                _ => unreachable!(),
            }
            assert_eq!(
                ordinal(&id),
                1,
                "weights must persist across successful/cancelled jobs and writing"
            );
        }
        let resident_peak = checkpoints
            .iter()
            .filter_map(|v| v["rssKiB"].as_u64())
            .max();
        if let (Some(first), Some(maximum)) = (checkpoints[0]["rssKiB"].as_u64(), resident_peak) {
            assert!(
                maximum.saturating_sub(first) <= 128 * 1024,
                "resident growth exceeded the 128 MiB qualification ceiling"
            );
        }
        assert_eq!(run(&french, &fr, "french"), expected_fr);
        assert_eq!(ordinal("french"), 2);
        assert_eq!(run(&a, &config, "english"), expected_a);
        assert_eq!(ordinal("english"), 3);
        let mut other = config.clone();
        other.models.asr = AsrModel::ParakeetTdt06bV3;
        assert!(!run(&a, &other, "parakeet").is_empty());
        assert_eq!(run(&a, &config, "after-model-switch"), expected_a);
        assert_eq!(ordinal("after-model-switch"), 4);
        manager.unload().unwrap();
        let unloaded_rss = rss_kib();
        assert_eq!(run(&a, &config, "after-unload"), expected_a);
        assert_eq!(ordinal("after-unload"), 5);
        drop(manager);
        let shutdown_rss = rss_kib();

        // Copy fixture artifacts; never alter the installed originals.
        let scratch = tempfile::tempdir().unwrap();
        for artifact in vesper_models::selected(&config)
            .unwrap()
            .iter()
            .filter(|a| a.role == "asr")
        {
            let destination = scratch.path().join(&artifact.relative_path);
            fs::create_dir_all(destination.parent().unwrap()).unwrap();
            fs::copy(root.join(&artifact.relative_path), destination).unwrap();
        }
        let tokens = scratch
            .path()
            .join(if asr.is_whisper() {
                crate::whisper::export(asr).0
            } else {
                "cohere-transcribe"
            })
            .join("tokens.txt");
        let original = fs::read(&tokens).unwrap();
        let manager = InferenceManager::new().unwrap();
        let run = |id: &str| manager.transcribe(&a, &config, scratch.path(), id, flag());
        let ordinal = |id: &str| {
            manager.execution_evidence(id)["asr"]["residentLoadOrdinal"]
                .as_u64()
                .unwrap()
        };
        assert_eq!(run("initial-copy").unwrap(), expected_a);
        fs::write(&tokens, "corrupt").unwrap();
        assert!(run("corrupt").is_err());
        fs::write(&tokens, &original).unwrap();
        assert_eq!(run("after-corrupt").unwrap(), expected_a);
        assert_eq!(ordinal("after-corrupt"), 2);
        fs::remove_file(&tokens).unwrap();
        assert!(run("missing").is_err());
        fs::write(&tokens, &original).unwrap();
        assert_eq!(run("after-missing").unwrap(), expected_a);
        assert_eq!(ordinal("after-missing"), 3);
        let replacement = tokens.with_extension("replacement");
        fs::write(&replacement, &original).unwrap();
        fs::rename(replacement, &tokens).unwrap();
        assert_eq!(run("replaced").unwrap(), expected_a);
        assert_eq!(ordinal("replaced"), 4);
        let mutate = tokens.clone();
        assert!(manager
            .transcribe_with_progress(
                &a,
                &config,
                scratch.path(),
                "changed-during-request",
                flag(),
                Arc::new(move |_| fs::write(&mutate, "changed during decode").unwrap())
            )
            .is_err());
        fs::write(&tokens, original).unwrap();
        assert_eq!(run("after-race").unwrap(), expected_a);
        assert_eq!(ordinal("after-race"), 5);
        drop(manager);
        println!(
            "{}",
            serde_json::json!({"case":"resident-asr-residency-lifecycle","model":asr.id(),"passed":true,"mixedCycles":100,
            "residentAsrOnlyRssKiB":resident_rss,"checkpoints":checkpoints,"afterUnloadRssKiB":unloaded_rss,
            "afterShutdownRssKiB":shutdown_rss,"cancellationSignalDelayMs":20,"cancelReturnFromRequestStartMs":cancellations,
            "coldComparisons":["English A","English B","long English","French"],"artifactInvalidation":true})
        );
    }
}
