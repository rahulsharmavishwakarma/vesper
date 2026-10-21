//! Shared completed-audio MLX ownership; architecture-specific native decoders remain upstream.
use crate::{
    artifact_identity::ArtifactIdentity, check_cancelled, native, Result, TranscriptionProgress,
};
use serde::Serialize;
use std::{
    cell::{Cell, RefCell, RefMut},
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
    time::Instant,
};
use vesper_audio::{MonoPcm, ASR_SAMPLE_RATE};
use vesper_core::{AppConfig, AsrModel, Language};

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct RecognizerKey {
    root: PathBuf,
    model: AsrModel,
    language: Language,
    artifacts: Vec<ArtifactIdentity>,
    runtime: Vec<ArtifactIdentity>,
}

pub(crate) fn key(config: &AppConfig, root: &Path) -> Result<RecognizerKey> {
    config.validate()?;
    if !config.uses_mlx_speech() {
        return Err(crate::InferenceError::UnsupportedAsr(
            config.models.asr.id(),
        ));
    }
    let root = root.canonicalize().unwrap_or_else(|_| root.to_owned());
    let artifacts = vesper_models::selected(config)
        .map_err(crate::resident_sherpa::verification_error)?
        .into_iter()
        .filter(|a| a.role == "asr")
        .map(|a| ArtifactIdentity::observe(&root, a))
        .collect();
    let runtime_root = vesper_models::mlx::runtime_directory();
    let runtime = vesper_models::mlx::runtime_artifacts()
        .into_iter()
        .map(|a| ArtifactIdentity::observe(&runtime_root, a))
        .collect();
    Ok(RecognizerKey {
        root,
        model: config.models.asr,
        language: config.language,
        artifacts,
        runtime,
    })
}

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Metrics {
    request_ordinal: u64,
    total_us: u64,
    input_samples: usize,
    processed_samples: usize,
    segments: Vec<serde_json::Value>,
}

pub(crate) struct Recognizer {
    model: RefCell<vesper_mlx::Model>,
    identity: RecognizerKey,
    pub load_ordinal: u64,
    verification_us: u64,
    load_us: u64,
    requests: Cell<u64>,
    pub healthy: Cell<bool>,
    metrics: RefCell<Metrics>,
}

pub(crate) fn maximum_samples(model: AsrModel) -> usize {
    (match model {
        AsrModel::WhisperSmall => 29,
        // Block attention is local to 1.28 s windows, so 30 s parts stay exact.
        AsrModel::ParakeetTdt06bV3
        | AsrModel::GraniteSpeech5TurboCtc
        | AsrModel::Phonon2
        | AsrModel::ParakeetRedux => 30,
        _ => 10,
    }) * ASR_SAMPLE_RATE as usize
}

fn map_error(error: vesper_mlx::Error) -> crate::InferenceError {
    match error {
        vesper_mlx::Error::Cancelled => crate::InferenceError::Cancelled,
        error => native("MLX speech", error),
    }
}

impl Recognizer {
    pub(crate) fn stream(&self) -> Result<Live<'_>> {
        if self.identity.model != AsrModel::Qwen3Asr06b
            || self.identity.language != Language::English
        {
            return Err(crate::InferenceError::UnsupportedAsr(
                self.identity.model.id(),
            ));
        }
        let mut model = self.model.borrow_mut();
        model.begin_live().map_err(map_error)?;
        Ok(Live {
            model,
            converter: None,
            rate: None,
            finished: false,
            leading_zero_samples: 0,
            dropped_samples: 0,
            has_nonzero_input: false,
        })
    }
    pub(crate) fn load(config: &AppConfig, root: &Path, cancelled: &AtomicBool) -> Result<Self> {
        check_cancelled(cancelled)?;
        let identity = key(config, root)?;
        let started = Instant::now();
        vesper_models::verify_asr(config, root, cancelled)
            .map_err(crate::resident_sherpa::verification_error)?;
        let verification_us = micros(started);
        check_cancelled(cancelled)?;
        let architecture = match config.models.asr {
            AsrModel::ParakeetTdt06bV3 => "parakeet",
            AsrModel::Qwen3Asr06b => "qwen3_asr",
            AsrModel::WhisperSmall => "whisper",
            AsrModel::GraniteSpeech5TurboCtc => "granite_speech5_ctc",
            AsrModel::Phonon2 => "phonon2",
            AsrModel::ParakeetRedux => "parakeet_redux",
            _ => unreachable!(),
        };
        let directory = root.join(
            vesper_models::mlx::model_subdirectory(config.models.asr)
                .map_err(crate::resident_sherpa::verification_error)?,
        );
        let started = Instant::now();
        let loaded = vesper_mlx::Model::load(
            &vesper_models::mlx::runtime_directory().join("libVesperMLX.dylib"),
            &directory,
            architecture,
        );
        let load_us = micros(started);
        check_cancelled(cancelled)?;
        if identity != key(config, root)? {
            return Err(native(
                "MLX speech identity",
                "files changed during model load",
            ));
        }
        let model = loaded.map_err(|e| match e {
            vesper_mlx::Error::Initialization(message) => {
                crate::InferenceError::AcceleratorInitialization(message)
            }
            other => map_error(other),
        })?;
        Ok(Self {
            model: RefCell::new(model),
            identity,
            load_ordinal: 0,
            verification_us,
            load_us,
            requests: Cell::new(0),
            healthy: Cell::new(true),
            metrics: RefCell::default(),
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
        check_cancelled(cancelled)?;
        config.validate()?;
        audio.require_sample_rate(ASR_SAMPLE_RATE)?;
        self.requests.set(self.requests.get() + 1);
        *self.metrics.borrow_mut() = Metrics {
            request_ordinal: self.requests.get(),
            input_samples: audio.samples().len(),
            ..Default::default()
        };
        let started = Instant::now();
        let result = crate::segmented_audio::decode_segments_with_limit(
            audio.samples(),
            cancelled,
            maximum_samples(config.models.asr),
            |samples| {
                let report = self
                    .model
                    .borrow_mut()
                    .transcribe(
                        samples,
                        Some(match config.models.asr {
                            AsrModel::Qwen3Asr06b => "English",
                            // Granite has no language prompt; record the selected mode.
                            AsrModel::GraniteSpeech5TurboCtc | AsrModel::ParakeetRedux => {
                                config.language.code().unwrap_or("auto")
                            }
                            _ => "en",
                        }),
                        512,
                        cancelled,
                    )
                    .map_err(map_error)?;
                self.metrics.borrow_mut().segments.push(serde_json::json!({"inputSamples":samples.len(), "stopReason":report.stop_reason, "generationTokens":report.generation_tokens, "promptTokens":report.prompt_tokens, "nativeSeconds":report.native_seconds, "processAllocatorPeakGB":report.peak_memory_gb, "whisperScores":report.whisper_scores,"resources":report.resources}));
                Ok(report.text)
            },
            &|p| {
                self.metrics.borrow_mut().processed_samples = p.processed_samples;
                progress(p);
            },
        );
        self.metrics.borrow_mut().total_us = micros(started);
        if self.identity != key(config, root)? {
            self.healthy.set(false);
            check_cancelled(cancelled)?;
            return Err(native(
                "MLX speech identity",
                "files changed during transcription",
            ));
        }
        if matches!(result, Err(crate::InferenceError::Native { .. })) {
            self.healthy.set(false);
        }
        result
    }

    pub(crate) fn evidence(&self, config: &AppConfig) -> serde_json::Value {
        serde_json::json!({"requestedBackend":config.compute_backend,"initializedProvider":"metal", "runtime":vesper_core::MLX_RUNTIME, "runtimeRevision":vesper_mlx::RUNTIME_REVISION, "runtimeArtifacts":vesper_models::mlx::runtime_artifacts(), "operatorPlacement":"MLX default GPU; individual kernels not attested", "residentWeights":true,"residentLoadOrdinal":self.load_ordinal,"verificationUs":self.verification_us,"modelLoadUs":self.load_us,"metrics":*self.metrics.borrow(),"contextualPhrasesSupported":false,"contextualPhraseCount":0,"evidence":"verified-application-MLX-fresh-segment-decoder-state"})
    }
}

pub(crate) struct Live<'a> {
    model: RefMut<'a, vesper_mlx::Model>,
    converter: Option<vesper_audio::StreamingResampler>,
    rate: Option<u32>,
    finished: bool,
    leading_zero_samples: u64,
    has_nonzero_input: bool,
    dropped_samples: u64,
}

impl Live<'_> {
    pub fn push(&mut self, samples: &[f32], rate: u32) -> Result<()> {
        if self.finished || self.rate.is_some_and(|previous| previous != rate) {
            return Err(native(
                "MLX live audio",
                "closed session or changed sample rate",
            ));
        }
        if self.converter.is_none() {
            self.converter = Some(vesper_audio::StreamingResampler::new(rate)?);
            self.rate = Some(rate);
        }
        let pcm = self
            .converter
            .as_mut()
            .expect("stream converter")
            .push(samples)?;
        for chunk in pcm.chunks(16000) {
            // The native session can hallucinate from digital silence. Avoid
            // starting its preview decoder until actual nonzero input arrives.
            // This is not VAD or an amplitude threshold; final audio is retained
            // independently by the recorder, including all leading zeros.
            if !self.has_nonzero_input && chunk.iter().all(|sample| *sample == 0.0) {
                self.leading_zero_samples += chunk.len() as u64;
                continue;
            }
            self.has_nonzero_input = true;
            self.dropped_samples += self.model.feed_live(chunk).map_err(map_error)?;
        }
        Ok(())
    }

    pub fn poll(&mut self) -> Result<Option<crate::adapters::LiveSnapshot>> {
        if self.finished {
            return Ok(None);
        }
        let update = self.model.poll_live().map_err(map_error)?;
        Ok(update.text.map(|text| crate::adapters::LiveSnapshot {
            text,
            audio_processed_seconds: update.input_seconds
                + self.leading_zero_samples as f32 / 16000.0,
            is_final: false,
        }))
    }

    /// 16 kHz input the native preview skipped under decoder backlog.
    pub fn dropped_seconds(&self) -> f32 {
        self.dropped_samples as f32 / 16000.0
    }

    pub fn finish(&mut self) {
        self.model.close_live();
        self.finished = true;
    }
}

impl Drop for Live<'_> {
    fn drop(&mut self) {
        self.finish();
    }
}

fn micros(start: Instant) -> u64 {
    start.elapsed().as_micros().min(u64::MAX as u128) as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{InferenceError, InferenceManager};
    use std::sync::{atomic::Ordering, Arc, Mutex};

    #[test]
    #[ignore = "requires pinned CPU/MLX Qwen models, a Metal device and public audio"]
    fn automatic_migration_agrees_with_download_readiness_and_execution() {
        use vesper_core::ComputeBackend;
        let source =
            PathBuf::from(vesper_core::environment::var_os("VESPER_SMOKE_MODELS").expect("models"));
        let audio = vesper_audio::decode_file(
            PathBuf::from(vesper_core::environment::var_os("VESPER_SMOKE_AUDIO").expect("audio")),
            0,
        )
        .unwrap();
        let root = tempfile::tempdir().unwrap();
        let mut config = AppConfig::default();
        config.models.asr = AsrModel::Qwen3Asr06b;
        config.cleanup_enabled = false;
        let saved = config.clone();
        let link_export = |backend| {
            let mut explicit = config.clone();
            explicit.compute_backend = backend;
            for a in vesper_models::selected(&explicit).unwrap() {
                let target = root.path().join(&a.relative_path);
                std::fs::create_dir_all(target.parent().unwrap()).unwrap();
                std::fs::hard_link(source.join(a.relative_path).canonicalize().unwrap(), target)
                    .unwrap();
            }
        };
        assert!(!vesper_models::inspect(&config, root.path()).ready);
        link_export(ComputeBackend::Cpu);
        assert!(vesper_models::inspect(&config, root.path()).ready);
        let card = vesper_models::list_model_installations(&config, root.path())
            .into_iter()
            .find(|m| m.selected && m.role == vesper_core::ModelRole::SpeechRecognition)
            .unwrap();
        assert!(card.ready);
        assert!(!card.artifacts_installed);
        let manager = InferenceManager::new().unwrap();
        let run = |config: &AppConfig, id| {
            manager
                .transcribe(
                    &audio,
                    config,
                    root.path(),
                    id,
                    Arc::new(AtomicBool::new(false)),
                )
                .unwrap()
        };
        let cpu = run(&config, "cpu-existing");
        assert!(!cpu.is_empty());
        assert_eq!(
            manager.execution_evidence("cpu-existing")["asr"]["initializedProvider"],
            "cpu"
        );
        let mut explicit = config.clone();
        explicit.compute_backend = ComputeBackend::Cpu;
        assert_eq!(run(&explicit, "cpu-reference"), cpu);
        manager.unload().unwrap();
        // Local verified cache supplies the ordinary installer without a download.
        link_export(ComputeBackend::Metal);
        assert!(
            vesper_models::install(&config, root.path(), &AtomicBool::new(false), |_| {})
                .unwrap()
                .ready
        );
        let gpu = run(&config, "automatic-gpu");
        let observed = manager.execution_evidence("automatic-gpu");
        assert_eq!(observed["asr"]["requestedBackend"], "auto");
        assert_eq!(observed["asr"]["initializedProvider"], "metal");
        assert_eq!(observed["asr"]["selection"]["preferredBackend"], "metal");
        assert_eq!(observed["asr"]["selection"]["resolvedBackend"], "metal");
        explicit.compute_backend = ComputeBackend::Metal;
        assert_eq!(run(&explicit, "metal-reference"), gpu);
        assert_eq!(
            observed["asr"]["residentLoadOrdinal"],
            manager.execution_evidence("metal-reference")["asr"]["residentLoadOrdinal"]
        );
        assert_eq!(config, saved);
    }

    #[test]
    fn identity_distinguishes_artifacts_and_selection_not_writer() {
        let root = tempfile::tempdir().unwrap();
        let mut config = AppConfig::default();
        config.models.asr = AsrModel::Qwen3Asr06b;
        config.compute_backend = vesper_core::ComputeBackend::Metal;
        let original = key(&config, root.path()).unwrap();
        config.cleanup_enabled = false;
        assert_eq!(original, key(&config, root.path()).unwrap());
        std::fs::create_dir_all(root.path().join("mlx/qwen")).unwrap();
        std::fs::write(root.path().join("mlx/qwen/tokenizer.json"), "corrupt").unwrap();
        assert_ne!(original, key(&config, root.path()).unwrap());
        config.language = Language::Auto;
        assert!(key(&config, root.path()).is_err());
    }

    #[test]
    #[ignore = "installed pinned MLX models and actual Metal for before/after load evidence"]
    fn installed_exact_zero_recordings_do_not_load_weights() {
        let root =
            PathBuf::from(vesper_core::environment::var_os("VESPER_SMOKE_MODELS").expect("models"));
        let silence = MonoPcm::new(vec![0.0; 16000], 16000).unwrap();
        let manager = InferenceManager::new().unwrap();
        let flag = || Arc::new(AtomicBool::new(false));
        let mut observed = Vec::new();
        for model in [
            AsrModel::ParakeetTdt06bV3,
            AsrModel::Qwen3Asr06b,
            AsrModel::WhisperSmall,
        ] {
            manager.unload().unwrap();
            let mut config = AppConfig::default();
            config.models.asr = model;
            config.compute_backend = vesper_core::ComputeBackend::Metal;
            config.cleanup_enabled = false;
            let progress = Arc::new(Mutex::new(Vec::new()));
            let updates = Arc::clone(&progress);
            let started = Instant::now();
            assert!(manager
                .transcribe_with_progress(
                    &silence,
                    &config,
                    &root,
                    "cold-zero",
                    flag(),
                    Arc::new(move |p| updates.lock().unwrap().push(p))
                )
                .unwrap()
                .is_empty());
            let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
            let evidence = manager.execution_evidence("cold-zero");
            let updates = progress.lock().unwrap();
            assert_eq!(
                updates.last().unwrap().processed_samples,
                silence.samples().len()
            );
            assert_eq!(
                updates.last().unwrap().total_samples,
                silence.samples().len()
            );
            observed.push(evidence["asr"]["residentWeights"] == false);
            println!(
                "{}",
                serde_json::json!({"model":model.id(),"coldZeroMs":elapsed_ms,"evidence":evidence})
            );
            let cancelled = flag();
            let signal = Arc::clone(&cancelled);
            assert!(matches!(
                manager.transcribe_with_progress(
                    &silence,
                    &config,
                    &root,
                    "cancel-zero",
                    cancelled,
                    Arc::new(move |_| signal.store(true, Ordering::Release))
                ),
                Err(InferenceError::Cancelled)
            ));
            assert!(manager
                .transcribe(&silence, &config, &root, "after-cancel-zero", flag())
                .unwrap()
                .is_empty());
            let missing = tempfile::tempdir().unwrap();
            assert!(manager
                .transcribe(
                    &silence,
                    &config,
                    missing.path(),
                    "missing-model-zero",
                    flag()
                )
                .is_err());
        }
        assert!(
            observed.into_iter().all(|skipped| skipped),
            "exact zeros must verify without loading native weights"
        );
    }

    #[test]
    #[ignore = "real Metal, installed pinned MLX variants and public speech fixtures"]
    fn installed_application_lifecycle_and_full_coverage() {
        let root =
            PathBuf::from(vesper_core::environment::var_os("VESPER_SMOKE_MODELS").expect("models"));
        let read = |name| {
            vesper_audio::decode_file(
                PathBuf::from(vesper_core::environment::var_os(name).expect(name)),
                0,
            )
            .unwrap()
        };
        let a = read("VESPER_SMOKE_AUDIO");
        let b = read("VESPER_SMOKE_AUDIO_B");
        let manager = InferenceManager::new().unwrap();
        let flag = || Arc::new(AtomicBool::new(false));
        for model in [
            AsrModel::ParakeetTdt06bV3,
            AsrModel::Qwen3Asr06b,
            AsrModel::WhisperSmall,
        ] {
            let mut config = AppConfig::default();
            config.models.asr = model;
            config.compute_backend = vesper_core::ComputeBackend::Metal;
            config.cleanup_enabled = false;
            assert!(vesper_models::inspect(&config, &root).ready);
            let expected = manager
                .transcribe(&a, &config, &root, "first", flag())
                .unwrap();
            assert!(!expected.is_empty());
            let ordinal = manager.execution_evidence("first")["asr"]["residentLoadOrdinal"].clone();
            let mut rss = Vec::new();
            for index in 0..100 {
                let input = if index % 3 == 1 { &b } else { &a };
                let actual = manager
                    .transcribe(input, &config, &root, "repeat", flag())
                    .unwrap();
                if index % 3 != 1 {
                    assert_eq!(actual, expected);
                }
                assert_eq!(
                    manager.execution_evidence("repeat")["asr"]["residentLoadOrdinal"],
                    ordinal
                );
                if index % 10 == 9 {
                    rss.push(crate::qualification::rss_kib());
                }
            }
            crate::qualification::assert_bounded_growth(&rss);
            // > 60 seconds includes multiple architecture windows and a tiny source tail.
            let long = MonoPcm::new(
                a.samples().repeat(1 + 61 * 16000 / a.samples().len()),
                16000,
            )
            .unwrap();
            let progress = Arc::new(Mutex::new(Vec::new()));
            let updates = Arc::clone(&progress);
            assert!(!manager
                .transcribe_with_progress(
                    &long,
                    &config,
                    &root,
                    "long",
                    flag(),
                    Arc::new(move |p| updates.lock().unwrap().push(p))
                )
                .unwrap()
                .is_empty());
            let updates = progress.lock().unwrap();
            assert!(updates.len() >= 3);
            assert_eq!(
                updates.last().unwrap().processed_samples,
                long.samples().len()
            );
            assert!(updates
                .windows(2)
                .all(|p| p[0].processed_samples < p[1].processed_samples));
            let evidence = manager.execution_evidence("long");
            assert_eq!(evidence["asr"]["initializedProvider"], "metal");
            assert_eq!(
                evidence["asr"]["metrics"]["processedSamples"],
                long.samples().len()
            );
            let cancelled = flag();
            let signal = Arc::clone(&cancelled);
            assert!(matches!(
                manager.transcribe_with_progress(
                    &long,
                    &config,
                    &root,
                    "cancel-progress",
                    cancelled,
                    Arc::new(move |_| signal.store(true, Ordering::Release))
                ),
                Err(InferenceError::Cancelled)
            ));
            let cancelled = flag();
            let signal = Arc::clone(&cancelled);
            let thread = std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(5));
                signal.store(true, Ordering::Release);
            });
            let result = manager.transcribe(&long, &config, &root, "cancel-native", cancelled);
            thread.join().unwrap();
            assert!(matches!(result, Err(InferenceError::Cancelled)));
            assert_eq!(
                manager
                    .transcribe(&a, &config, &root, "recovery", flag())
                    .unwrap(),
                expected
            );
            assert_eq!(
                manager.execution_evidence("recovery")["asr"]["residentLoadOrdinal"],
                ordinal
            );
            assert!(manager
                .start_stream(&config, &root, "unsupported-live", flag(), Arc::new(|_| {}))
                .is_err());
            let silence = MonoPcm::new(vec![0.0; 16000], 16000).unwrap();
            assert!(manager
                .transcribe(&silence, &config, &root, "silence", flag())
                .unwrap()
                .is_empty());
            manager.unload().unwrap();
            assert_eq!(
                manager
                    .transcribe(&a, &config, &root, "reload", flag())
                    .unwrap(),
                expected
            );
            assert_ne!(
                manager.execution_evidence("reload")["asr"]["residentLoadOrdinal"],
                ordinal
            );
            println!(
                "{}",
                serde_json::json!({"model":model.id(),"passed":true,"repeatRequests":100,"longSamples":long.samples().len(),"longEvidence":evidence["asr"]})
            );
        }
    }
}
