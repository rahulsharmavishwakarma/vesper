//! Shared streaming/completed audio processing. Frozen weights, CPU, advisory
//! VAD and optional level adjustment/enhancement. The caller retains original PCM
//! for completed fallback.
use crate::{artifact_identity::ArtifactIdentity, check_cancelled, native, InferenceError, Result};
use serde::Serialize;
use sherpa_onnx::{
    DenoisedAudio, OfflineSpeechDenoiserModelConfig, OnlineSpeechDenoiser,
    OnlineSpeechDenoiserConfig, SileroVadModelConfig, VadModelConfig, VoiceActivityDetector,
};
use std::{
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
    time::Instant,
};
use vesper_audio::{MonoPcm, StreamingResampler, ASR_SAMPLE_RATE};
use vesper_core::{AppConfig, AudioFrontendConfig};

mod level;
pub use level::LevelMetrics;

const VAD_FRAME: usize = 512;
const VAD_CONTEXT: usize = 64;
const VAD_BOOKKEEPING_FRAMES: usize = 320;
const DPDF_DELAY: usize = 640;
const BLOCK: usize = 320;
const MAX_INTERVALS: usize = 4096;

#[derive(Debug, PartialEq, Eq)]
struct Key {
    settings: AudioFrontendConfig,
    files: Vec<ArtifactIdentity>,
}
fn key(config: &AppConfig, root: &Path) -> Result<Key> {
    let files = vesper_models::selected(config)
        .map_err(|e| native("audio frontend identity", e))?
        .into_iter()
        .filter(|a| a.role == "audio_frontend")
        .map(|a| ArtifactIdentity::observe(root, a))
        .collect();
    Ok(Key {
        settings: config.audio_frontend.clone(),
        files,
    })
}

struct Models {
    key: Key,
    config: AppConfig,
    root: PathBuf,
    vad: Option<VoiceActivityDetector>,
    denoiser: Option<OnlineSpeechDenoiser>,
    verification_us: u64,
    load_us: u64,
}
impl Models {
    fn load(config: &AppConfig, root: &Path, cancelled: &AtomicBool) -> Result<Self> {
        let identity = key(config, root)?;
        let start = Instant::now();
        vesper_models::verify_frontend(config, root, cancelled).map_err(|e| match e {
            vesper_models::ModelError::Cancelled => InferenceError::Cancelled,
            e => native("audio frontend verification", e),
        })?;
        let verification_us = micros(start);
        let artifacts =
            vesper_models::selected(config).map_err(|e| native("audio frontend artifacts", e))?;
        let path = |id: &str| -> Result<String> {
            let artifact = artifacts
                .iter()
                .find(|a| a.role == "audio_frontend" && a.model_id.as_deref() == Some(id))
                .ok_or_else(|| native("audio frontend", "missing artifact declaration"))?;
            let path = root.join(&artifact.relative_path);
            path.to_str()
                .filter(|s| !s.contains('\0'))
                .map(str::to_owned)
                .ok_or(InferenceError::InvalidPath(path))
        };
        check_cancelled(cancelled)?;
        let start = Instant::now();
        let vad = if config.audio_frontend.vad_enabled || config.audio_frontend.auto_level_enabled {
            Some(
                VoiceActivityDetector::create(
                    &VadModelConfig {
                        silero_vad: SileroVadModelConfig {
                            model: Some(path("silero-v6.2")?),
                            threshold: 0.5,
                            min_silence_duration: 0.25,
                            min_speech_duration: 0.064,
                            window_size: VAD_FRAME as i32,
                            max_speech_duration: 30.0,
                        },
                        sample_rate: 16000,
                        num_threads: 1,
                        provider: Some("cpu".into()),
                        ..Default::default()
                    },
                    12.0,
                )
                .ok_or_else(|| native("audio frontend load", "Silero initialization failed"))?,
            )
        } else {
            None
        };
        check_cancelled(cancelled)?;
        let denoiser = if let Some(id) = config
            .audio_frontend
            .model_ids()
            .into_iter()
            .find(|id| id.starts_with("dpdfnet"))
        {
            let mut model = OfflineSpeechDenoiserModelConfig {
                num_threads: 1,
                provider: Some("cpu".into()),
                ..Default::default()
            };
            model.dpdfnet.model = Some(path(id)?);
            let denoiser = OnlineSpeechDenoiser::create(&OnlineSpeechDenoiserConfig { model })
                .ok_or_else(|| native("audio frontend load", "DPDFNet initialization failed"))?;
            if denoiser.sample_rate() != 16000 || denoiser.frame_shift_in_samples() != 160 {
                return Err(native(
                    "audio frontend load",
                    "unexpected DPDFNet sample rate or hop",
                ));
            }
            Some(denoiser)
        } else {
            None
        };
        check_cancelled(cancelled)?;
        if key(config, root)? != identity {
            return Err(native(
                "audio frontend identity",
                "artifacts changed while loading",
            ));
        }
        Ok(Self {
            key: identity,
            config: config.clone(),
            root: root.to_owned(),
            vad,
            denoiser,
            verification_us,
            load_us: micros(start),
        })
    }
    fn reset(&self) {
        if let Some(vad) = &self.vad {
            vad.reset();
            vad.clear();
        }
        if let Some(denoiser) = &self.denoiser {
            denoiser.reset();
        }
    }
}

/// Selected handles belong to one inference worker. A session's mutable borrow
/// prevents recordings from sharing recurrent state concurrently.
#[derive(Default)]
pub struct FrontendCache {
    models: Option<Models>,
    load_ordinal: u64,
}
impl FrontendCache {
    pub fn unload(&mut self) {
        self.models = None;
    }
    pub fn start<'a>(
        &'a mut self,
        config: &AppConfig,
        root: &Path,
        cancelled: &AtomicBool,
    ) -> Result<FrontendSession<'a>> {
        check_cancelled(cancelled)?;
        config.validate()?;
        if !config.audio_frontend.enabled() {
            self.models = None;
        } else {
            let identity = match key(config, root) {
                Ok(identity) => identity,
                Err(error) => {
                    self.models = None;
                    return Err(error);
                }
            };
            if self.models.as_ref().is_none_or(|m| m.key != identity) {
                self.models = None;
                self.models = Some(Models::load(config, root, cancelled)?);
                self.load_ordinal += 1;
            }
        }
        if let Some(models) = &self.models {
            models.reset();
            if let Some(vad) = &models.vad {
                vad.accept_waveform(&[0.0; VAD_CONTEXT]);
            }
        }
        let metrics = FrontendMetrics {
            settings: config.audio_frontend.clone(),
            load_ordinal: self.load_ordinal,
            verification_us: self.models.as_ref().map_or(0, |m| m.verification_us),
            model_load_us: self.models.as_ref().map_or(0, |m| m.load_us),
            ..Default::default()
        };
        Ok(FrontendSession {
            models: self.models.as_ref(),
            resampler: None,
            pending_vad: Vec::new(),
            pending_level: Vec::new(),
            level: config
                .audio_frontend
                .auto_level_enabled
                .then(level::LevelController::default),
            vad_frames: 0,
            native_output: 0,
            delay_remaining: if self.models.as_ref().is_some_and(|m| m.denoiser.is_some()) {
                DPDF_DELAY
            } else {
                0
            },
            finished: false,
            metrics,
        })
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SpeechInterval {
    pub start_sample: usize,
    pub end_sample: usize,
    pub bookkeeping_split: bool,
}
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FrontendMetrics {
    pub settings: AudioFrontendConfig,
    pub load_ordinal: u64,
    /// Cost at the resident handle's creation, not repeated request work.
    pub verification_us: u64,
    pub model_load_us: u64,
    pub input_samples_16k: usize,
    pub output_samples_16k: usize,
    pub resample_us: u64,
    pub native_us: u64,
    pub finalize_us: u64,
    pub dispatches: u64,
    pub max_dispatch_us: u64,
    /// Fixed 1 ms buckets, final bucket >=20 ms; bounded for long recordings.
    pub dispatch_histogram: [u64; 21],
    pub speech_intervals: Vec<SpeechInterval>,
    pub omitted_intervals: usize,
    pub finalized: bool,
    pub automatic_level: Option<LevelMetrics>,
    pub level_us: u64,
    pub enhanced_output_peak: f32,
}
impl FrontendMetrics {
    pub fn evidence(&self) -> serde_json::Value {
        serde_json::json!({"runtime":"sherpa-onnx", "runtimeVersion":"1.13.8",
            "runtimeBuildId":env!("TRANSCRIBE_SHERPA_BUILD_ID"), "runtimePatchSha256":env!("TRANSCRIBE_SHERPA_PATCH_SHA256"),
            "initializedProvider":"cpu", "nativeGraphThreads":1, "vadPolicy":vad_policy(&self.settings),
            "automaticLevelPolicy":if self.settings.auto_level_enabled {Some("bounded-level-v1")} else {None},
            "automaticLevelFrameSamples":level::FRAME,
            "denoiserApi":"online-in-both-modes", "sampleRate":16000, "dispatchHistogramBucketUs":1000, "metrics":self})
    }
}

pub struct FrontendSession<'a> {
    models: Option<&'a Models>,
    resampler: Option<StreamingResampler>,
    pending_vad: Vec<f32>,
    pending_level: Vec<f32>,
    level: Option<level::LevelController>,
    vad_frames: usize,
    native_output: usize,
    delay_remaining: usize,
    finished: bool,
    metrics: FrontendMetrics,
}
impl FrontendSession<'_> {
    pub fn metrics(&self) -> &FrontendMetrics {
        &self.metrics
    }
    pub fn push(&mut self, samples: &[f32], rate: u32, cancelled: &AtomicBool) -> Result<Vec<f32>> {
        let result = self.push_inner(samples, rate, cancelled);
        if result.is_err() {
            self.finished = true;
        }
        result
    }
    fn push_inner(
        &mut self,
        samples: &[f32],
        rate: u32,
        cancelled: &AtomicBool,
    ) -> Result<Vec<f32>> {
        self.check_open(cancelled)?;
        if self
            .resampler
            .as_ref()
            .is_some_and(|r| r.source_rate() != rate)
        {
            return Err(native(
                "audio frontend",
                "sample rate changed within a recording",
            ));
        }
        let started = Instant::now();
        if self.resampler.is_none() {
            self.resampler = Some(StreamingResampler::new(rate)?);
        }
        let pcm = self
            .resampler
            .as_mut()
            .expect("initialized resampler")
            .push(samples)?;
        self.metrics.resample_us += micros(started);
        let out = self.process(&pcm, cancelled)?;
        self.metrics.dispatches += 1;
        let elapsed = micros(started);
        self.metrics.max_dispatch_us = self.metrics.max_dispatch_us.max(elapsed);
        self.metrics.dispatch_histogram[(elapsed / 1000).min(20) as usize] += 1;
        Ok(out)
    }
    pub fn finish(&mut self, cancelled: &AtomicBool) -> Result<Vec<f32>> {
        self.check_open(cancelled)?;
        let started = Instant::now();
        // Finalize at most once, including when tail processing fails.
        self.finished = true;
        let resample_start = Instant::now();
        let tail = match &mut self.resampler {
            Some(r) => r.finish()?,
            None => Vec::new(),
        };
        self.metrics.resample_us += micros(resample_start);
        let mut out = self.process(&tail, cancelled)?;
        let native_start = Instant::now();
        if self.metrics.input_samples_16k > 0 {
            if !self.pending_level.is_empty() {
                let pending = std::mem::take(&mut self.pending_level);
                out.extend(self.level_frame(&pending, cancelled)?);
            }
            if self.models.is_some_and(|m| m.vad.is_some()) {
                // Advance past all real tail samples, including native context lag.
                self.pending_vad.resize(
                    (self.pending_vad.len() + VAD_CONTEXT).div_ceil(VAD_FRAME) * VAD_FRAME,
                    0.0,
                );
                let pending = std::mem::take(&mut self.pending_vad);
                for frame in pending.chunks(VAD_FRAME) {
                    self.vad_frame(frame, cancelled)?;
                }
                if let Some(vad) = self.models.and_then(|m| m.vad.as_ref()) {
                    vad.flush();
                }
                self.drain_vad(false);
            }
            if let Some(denoiser) = self.models.and_then(|m| m.denoiser.as_ref()) {
                check_cancelled(cancelled)?;
                out.extend(self.accept_denoised(denoiser.run(&[0.0; DPDF_DELAY], 16000))?);
                check_cancelled(cancelled)?;
                out.extend(self.accept_denoised(denoiser.flush())?);
                if self.native_output != self.metrics.input_samples_16k + DPDF_DELAY {
                    return Err(native("audio frontend", "incomplete denoiser tail"));
                }
            }
        }
        self.metrics.native_us += micros(native_start);
        if self.metrics.output_samples_16k != self.metrics.input_samples_16k {
            return Err(native("audio frontend", "processed audio length mismatch"));
        }
        if let Some(models) = self.models {
            if key(&models.config, &models.root)? != models.key {
                return Err(native(
                    "audio frontend identity",
                    "artifacts changed during recording",
                ));
            }
        }
        check_cancelled(cancelled)?;
        self.metrics.finalized = true;
        self.metrics.finalize_us = micros(started);
        Ok(out)
    }
    fn check_open(&self, cancelled: &AtomicBool) -> Result<()> {
        check_cancelled(cancelled)?;
        if self.finished {
            return Err(native("audio frontend", "recording is already finalized"));
        }
        Ok(())
    }
    fn process(&mut self, pcm: &[f32], cancelled: &AtomicBool) -> Result<Vec<f32>> {
        self.metrics.input_samples_16k += pcm.len();
        let start = Instant::now();
        let mut out = Vec::with_capacity(pcm.len());
        for block in pcm.chunks(BLOCK) {
            check_cancelled(cancelled)?;
            if self.level.is_some() {
                self.pending_level.extend_from_slice(block);
                while self.pending_level.len() >= level::FRAME {
                    let frame: Vec<_> = self.pending_level.drain(..level::FRAME).collect();
                    out.extend(self.level_frame(&frame, cancelled)?);
                }
                continue;
            }
            if self.models.is_some_and(|m| m.vad.is_some()) {
                self.pending_vad.extend_from_slice(block);
                while self.pending_vad.len() >= VAD_FRAME {
                    let frame: Vec<_> = self.pending_vad.drain(..VAD_FRAME).collect();
                    self.vad_frame(&frame, cancelled)?;
                }
            }
            if let Some(denoiser) = self.models.and_then(|m| m.denoiser.as_ref()) {
                out.extend(self.accept_denoised(denoiser.run(block, 16000))?);
            } else {
                out.extend_from_slice(block);
                self.metrics.output_samples_16k += block.len();
            }
        }
        self.metrics.native_us += micros(start);
        Ok(out)
    }
    fn level_frame(&mut self, frame: &[f32], cancelled: &AtomicBool) -> Result<Vec<f32>> {
        let start = Instant::now();
        let mut detection = self
            .level
            .as_mut()
            .expect("automatic level enabled")
            .detection_input(frame);
        // Only the detector sees padding; gain/noise statistics see real PCM.
        detection.resize(VAD_FRAME, 0.0);
        self.metrics.level_us += micros(start);
        let speech = self.vad_frame(&detection, cancelled)?;
        let start = Instant::now();
        let controller = self.level.as_mut().expect("automatic level enabled");
        let adjusted = controller.apply(frame, speech);
        self.metrics.automatic_level = Some(controller.metrics.clone());
        self.metrics.level_us += micros(start);
        if let Some(denoiser) = self.models.and_then(|m| m.denoiser.as_ref()) {
            self.accept_denoised(denoiser.run(&adjusted, 16000))
        } else {
            self.metrics.output_samples_16k += adjusted.len();
            Ok(adjusted)
        }
    }
    fn vad_frame(&mut self, frame: &[f32], cancelled: &AtomicBool) -> Result<bool> {
        check_cancelled(cancelled)?;
        let mut speech = false;
        if let Some(vad) = self.models.and_then(|m| m.vad.as_ref()) {
            vad.accept_waveform(frame);
            // Flush clears native segment state. Read live activity first.
            speech = vad.detected();
            self.vad_frames += 1;
            self.drain_vad(false);
            if self.vad_frames % VAD_BOOKKEEPING_FRAMES == 0 {
                vad.flush();
                self.drain_vad(true);
            }
        }
        Ok(speech)
    }
    fn drain_vad(&mut self, bookkeeping_split: bool) {
        let Some(vad) = self.models.and_then(|m| m.vad.as_ref()) else {
            return;
        };
        while !vad.is_empty() {
            let Some(segment) = vad.front() else {
                break;
            };
            vad.pop();
            if !self.metrics.settings.vad_enabled {
                continue;
            }
            let start = (segment.start().max(0) as usize)
                .saturating_sub(VAD_CONTEXT)
                .min(self.metrics.input_samples_16k);
            let end = (segment.start().max(0) as usize + segment.samples().len())
                .saturating_sub(VAD_CONTEXT)
                .min(self.metrics.input_samples_16k);
            if end > start {
                if self.metrics.speech_intervals.len() < MAX_INTERVALS {
                    self.metrics.speech_intervals.push(SpeechInterval {
                        start_sample: start,
                        end_sample: end,
                        bookkeeping_split,
                    });
                } else {
                    self.metrics.omitted_intervals += 1;
                }
            }
        }
    }
    fn accept_denoised(&mut self, mut audio: DenoisedAudio) -> Result<Vec<f32>> {
        if (!audio.samples.is_empty() && audio.sample_rate != 16000)
            || audio.samples.iter().any(|x| !x.is_finite())
        {
            return Err(native("audio frontend", "invalid denoiser output"));
        }
        self.native_output += audio.samples.len();
        let skip = self.delay_remaining.min(audio.samples.len());
        self.delay_remaining -= skip;
        audio.samples.drain(..skip);
        self.metrics.enhanced_output_peak = audio
            .samples
            .iter()
            .fold(self.metrics.enhanced_output_peak, |peak, sample| {
                peak.max(sample.abs())
            });
        self.metrics.output_samples_16k += audio.samples.len();
        if self.metrics.output_samples_16k > self.metrics.input_samples_16k {
            return Err(native("audio frontend", "denoiser emitted synthetic tail"));
        }
        Ok(audio.samples)
    }
}

pub(crate) fn vad_policy(settings: &AudioFrontendConfig) -> &'static str {
    if settings.auto_level_enabled {
        "level-conditioned-analysis-no-audio-deletion"
    } else {
        "advisory-original-pcm"
    }
}
impl Drop for FrontendSession<'_> {
    fn drop(&mut self) {
        if let Some(models) = self.models {
            models.reset();
        }
    }
}

/// Bounded calls over the same state machine as live processing. Does not pace.
pub fn process_completed(
    cache: &mut FrontendCache,
    audio: &MonoPcm,
    config: &AppConfig,
    root: &Path,
    cancelled: &AtomicBool,
) -> Result<(MonoPcm, FrontendMetrics)> {
    let mut session = cache.start(config, root, cancelled)?;
    let mut samples = Vec::new();
    for block in audio.samples().chunks(BLOCK) {
        samples.extend(session.push(block, audio.sample_rate(), cancelled)?);
    }
    samples.extend(session.finish(cancelled)?);
    Ok((
        MonoPcm::new(samples, ASR_SAMPLE_RATE)?,
        session.metrics.clone(),
    ))
}
fn micros(start: Instant) -> u64 {
    start.elapsed().as_micros().min(u128::from(u64::MAX)) as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;
    use vesper_core::NoiseReduction;

    #[test]
    fn bypass_is_exact_and_rejects_invalid_session_transitions() {
        let mut cache = FrontendCache::default();
        let config = AppConfig::default();
        let flag = AtomicBool::new(false);
        let mut session = cache.start(&config, Path::new("/unused"), &flag).unwrap();
        assert_eq!(
            session.push(&[0.1, -0.3], 16000, &flag).unwrap(),
            [0.1, -0.3]
        );
        assert!(session.finish(&flag).unwrap().is_empty());
        assert!(session.finish(&flag).is_err());
        assert!(session.push(&[0.1], 16000, &flag).is_err());
        drop(session);
        let mut session = cache.start(&config, Path::new("/unused"), &flag).unwrap();
        session.push(&[0.1], 48000, &flag).unwrap();
        assert!(session.push(&[0.1], 44100, &flag).is_err());
        assert!(session.finish(&flag).is_err());
        drop(session);
        let mut session = cache.start(&config, Path::new("/unused"), &flag).unwrap();
        flag.store(true, Ordering::Release);
        assert!(matches!(
            session.push(&[0.1], 16000, &flag),
            Err(InferenceError::Cancelled)
        ));
    }

    #[test]
    fn frontend_keys_ignore_asr_and_track_replaced_artifacts() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = AppConfig::default();
        config.audio_frontend.vad_enabled = true;
        let absent = key(&config, dir.path()).unwrap();
        std::fs::create_dir(dir.path().join("audio-frontends")).unwrap();
        std::fs::write(
            dir.path().join("audio-frontends/silero_v6.2_16k_op15.onnx"),
            "invalid fixture",
        )
        .unwrap();
        let present = key(&config, dir.path()).unwrap();
        assert_ne!(absent, present);
        config.models.asr = vesper_core::AsrModel::WhisperSmall;
        config.cleanup_enabled = false;
        assert_eq!(present, key(&config, dir.path()).unwrap());
        assert!(
            FrontendCache::default()
                .start(&config, dir.path(), &AtomicBool::new(false))
                .is_err(),
            "bad bytes never enter native load"
        );
    }

    #[test]
    #[ignore = "requires pinned frontend models and a public speech fixture"]
    fn automatic_level_native_packet_prefix_tail_and_reset_contracts() {
        let root = PathBuf::from(std::env::var_os("TRANSCRIBE_SMOKE_MODELS").expect("models"));
        let audio = vesper_audio::decode_file(
            Path::new(&std::env::var_os("TRANSCRIBE_SMOKE_AUDIO").expect("public fixture")),
            60,
        )
        .unwrap();
        let flag = AtomicBool::new(false);
        let mut config = AppConfig::default();
        config.audio_frontend.auto_level_enabled = true;
        let mut cache = FrontendCache::default();
        // No annotation opt-in is needed to use speech evidence for level control.
        assert!(!config.audio_frontend.vad_enabled);
        for denoiser in [
            NoiseReduction::Off,
            NoiseReduction::Dpdfnet4,
            NoiseReduction::Dpdfnet8,
        ] {
            config.audio_frontend.noise_reduction = denoiser;
            for length in [0, 1, 447, 511, 512, 513, 1023, audio.samples().len()] {
                let input = MonoPcm::new(
                    audio.samples()[..length].iter().map(|x| x * 0.03).collect(),
                    16000,
                )
                .unwrap();
                let (expected, stats) =
                    process_completed(&mut cache, &input, &config, &root, &flag).unwrap();
                let mut session = cache.start(&config, &root, &flag).unwrap();
                let mut actual = Vec::new();
                let mut offset = 0;
                for size in [1, 159, 319, 17, 1600, 511].into_iter().cycle() {
                    if offset == length {
                        break;
                    }
                    let end = (offset + size).min(length);
                    actual.extend(
                        session
                            .push(&input.samples()[offset..end], 16000, &flag)
                            .unwrap(),
                    );
                    offset = end;
                    assert!(session.pending_level.len() < level::FRAME);
                    assert_eq!(
                        &actual[..],
                        &expected.samples()[..actual.len()],
                        "causal prefix changed"
                    );
                }
                actual.extend(session.finish(&flag).unwrap());
                assert_eq!(actual, expected.samples());
                assert_eq!(actual.len(), length);
                assert!(session.metrics().speech_intervals.is_empty());
                assert_eq!(
                    serde_json::to_value(&session.metrics().automatic_level).unwrap(),
                    serde_json::to_value(&stats.automatic_level).unwrap()
                );
                if let Some(level) = &stats.automatic_level {
                    assert_eq!(level.samples, length);
                    assert!(level.max_gain_db <= 30.0001);
                    assert!(level.output_peak <= 0.950001);
                }
                assert!(session.finish(&flag).is_err());
                assert!(session.push(&[0.0], 16000, &flag).is_err());
            }
            let (baseline, _) =
                process_completed(&mut cache, &audio, &config, &root, &flag).unwrap();
            for cycle in 0..6 {
                {
                    let mut b = cache.start(&config, &root, &flag).unwrap();
                    b.push(&vec![0.7; 4000 + cycle], 48000, &flag).unwrap();
                    if cycle % 3 == 0 {
                        b.finish(&flag).unwrap();
                    }
                    if cycle % 3 == 1 {
                        flag.store(true, Ordering::Release);
                        assert!(matches!(b.finish(&flag), Err(InferenceError::Cancelled)));
                        flag.store(false, Ordering::Release);
                    }
                }
                let (after, _) =
                    process_completed(&mut cache, &audio, &config, &root, &flag).unwrap();
                assert_eq!(
                    after.samples(),
                    baseline.samples(),
                    "state crossed recordings"
                );
            }
            // Unconfirmed stationary noise and digital silence must not gain.
            for amplitude in [0.0, 0.00001, 0.01] {
                let mut state = 42_u32;
                let noise = (0..32000)
                    .map(|_| {
                        state ^= state << 13;
                        state ^= state >> 17;
                        state ^= state << 5;
                        amplitude * (state as f64 / u32::MAX as f64 * 2.0 - 1.0) as f32
                    })
                    .collect();
                let input = MonoPcm::new(noise, 16000).unwrap();
                let (_, stats) =
                    process_completed(&mut cache, &input, &config, &root, &flag).unwrap();
                assert_eq!(stats.automatic_level.unwrap().max_gain_db, 0.0);
            }
        }
        eprintln!("automatic level: 24 packet/tail schedules, 18 mixed reset cycles, 9 noise/silence cases passed");
    }

    #[test]
    #[ignore = "requires pinned frontend models and a public speech fixture"]
    fn installed_frontends_packetization_tails_and_100_recording_resets() {
        let root = PathBuf::from(std::env::var_os("TRANSCRIBE_SMOKE_MODELS").expect("models"));
        let audio = vesper_audio::decode_file(
            Path::new(&std::env::var_os("TRANSCRIBE_SMOKE_AUDIO").expect("public fixture")),
            60,
        )
        .unwrap();
        let flag = AtomicBool::new(false);
        let mut cache = FrontendCache::default();
        let mut config = AppConfig::default();
        config.audio_frontend.vad_enabled = true;
        let mut cases = 0;
        for denoiser in [
            NoiseReduction::Off,
            NoiseReduction::Dpdfnet4,
            NoiseReduction::Dpdfnet8,
        ] {
            config.audio_frontend.noise_reduction = denoiser;
            for length in [
                0,
                1,
                159,
                160,
                161,
                447,
                448,
                449,
                511,
                512,
                513,
                1023,
                1024,
                1025,
                audio.samples().len(),
            ] {
                let input = MonoPcm::new(audio.samples()[..length].to_vec(), 16000).unwrap();
                let (expected, stats) =
                    process_completed(&mut cache, &input, &config, &root, &flag).unwrap();
                let mut session = cache.start(&config, &root, &flag).unwrap();
                let mut actual = Vec::new();
                let mut offset = 0;
                for size in [1, 319, 17, 1600, 511].into_iter().cycle() {
                    if offset == length {
                        break;
                    }
                    let end = (offset + size).min(length);
                    actual.extend(
                        session
                            .push(&input.samples()[offset..end], 16000, &flag)
                            .unwrap(),
                    );
                    offset = end;
                }
                actual.extend(session.finish(&flag).unwrap());
                assert_eq!(actual.len(), length);
                let error = actual
                    .iter()
                    .zip(expected.samples())
                    .map(|(a, b)| (a - b).abs())
                    .fold(0.0_f32, f32::max);
                assert!(error <= 1e-6, "packetization error {error}");
                assert_eq!(session.metrics().speech_intervals, stats.speech_intervals);
                assert!(session.finish(&flag).is_err());
                assert!(session.push(&[0.0], 16000, &flag).is_err());
                cases += 1;
            }
            let (baseline, _) =
                process_completed(&mut cache, &audio, &config, &root, &flag).unwrap();
            // Different content/rates/early exits must not contaminate the next A.
            for cycle in 0..100 {
                {
                    let mut b = cache.start(&config, &root, &flag).unwrap();
                    b.push(
                        &vec![0.03; 1031 + cycle],
                        if cycle % 2 == 0 { 48000 } else { 44100 },
                        &flag,
                    )
                    .unwrap();
                    if cycle % 3 == 0 {
                        b.finish(&flag).unwrap();
                    } else if cycle % 3 == 1 {
                        flag.store(true, Ordering::Release);
                        assert!(matches!(b.finish(&flag), Err(InferenceError::Cancelled)));
                        flag.store(false, Ordering::Release);
                    } // Remaining cycles drop without Finish.
                }
                let (a, stats) =
                    process_completed(&mut cache, &audio, &config, &root, &flag).unwrap();
                assert_eq!(
                    a.samples(),
                    baseline.samples(),
                    "cross-recording contamination {cycle}"
                );
                assert_eq!(stats.output_samples_16k, audio.samples().len());
            }
            // Natural pause endpoints close to each bookkeeping boundary.
            for remainder in 0..8 {
                let length = VAD_BOOKKEEPING_FRAMES * VAD_FRAME + remainder * VAD_FRAME;
                let mut speech = Vec::with_capacity(length + 16000);
                speech.extend(audio.samples().iter().copied().cycle().take(length));
                speech.extend_from_slice(&[0.0; 8000]);
                speech.extend_from_slice(audio.samples());
                let input = MonoPcm::new(speech, 16000).unwrap();
                let (_, stats) =
                    process_completed(&mut cache, &input, &config, &root, &flag).unwrap();
                assert!(stats.speech_intervals.iter().all(
                    |s| s.start_sample < s.end_sample && s.end_sample <= input.samples().len()
                ));
                assert!(
                    stats
                        .speech_intervals
                        .iter()
                        .any(|s| s.end_sample > length + 8000),
                    "later speech must survive bookkeeping"
                );
            }
            cache.unload();
        }
        eprintln!("frontend contract: {cases} boundary schedules; 300 mixed reset cycles; 24 bookkeeping-pause recordings passed");
    }
}
