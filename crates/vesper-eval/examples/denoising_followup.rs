//! Development-only observation-adding and gain diagnostics. No product defaults.
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::VecDeque,
    fs,
    path::Path,
    sync::{atomic::AtomicBool, Arc, Mutex},
    thread,
    time::{Duration, Instant},
};
use vesper_audio::CapturedAudio;
use vesper_core::{AppConfig, AsrModel, ComputeBackend, Language, NoiseReduction};
use vesper_eval::{score, BenchmarkManifest, ReplayKind};
use vesper_inference::{audio_frontend::FrontendCache, InferenceManager};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const GAIN: f32 = 0.03;

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn read(path: &Path) -> Result<Value> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}
fn write(path: &Path, value: &Value) -> Result<()> {
    fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}
fn us(start: Instant) -> u64 {
    start.elapsed().as_micros() as u64
}
fn ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

fn config(name: &str) -> AppConfig {
    let mut config = AppConfig {
        language: Language::English,
        compute_backend: ComputeBackend::Cpu,
        cleanup_enabled: false,
        ..Default::default()
    };
    config.audio_frontend.vad_enabled = name != "raw";
    config.audio_frontend.noise_reduction = if name.starts_with("dpdfnet4") {
        NoiseReduction::Dpdfnet4
    } else if name.starts_with("dpdfnet8") {
        NoiseReduction::Dpdfnet8
    } else {
        NoiseReduction::Off
    };
    config
}

/// Canonical 16 kHz only. Never mix future or synthetic-tail original samples.
struct AlignedMix {
    original: VecDeque<f32>,
    weight: f32,
    max_pending: usize,
    mix_us: u64,
}
impl AlignedMix {
    fn new(weight: f32) -> Result<Self> {
        if !weight.is_finite() || !(0.0..=1.0).contains(&weight) {
            return Err("invalid original weight".into());
        }
        Ok(Self {
            original: VecDeque::new(),
            weight,
            max_pending: 0,
            mix_us: 0,
        })
    }
    fn push_original(&mut self, input: &[f32]) -> Result<()> {
        if input.iter().any(|x| !x.is_finite()) {
            return Err("nonfinite original".into());
        }
        self.original.extend(input);
        self.max_pending = self.max_pending.max(self.original.len());
        Ok(())
    }
    fn accept(&mut self, enhanced: &[f32]) -> Result<Vec<f32>> {
        let started = Instant::now();
        if enhanced.len() > self.original.len() || enhanced.iter().any(|x| !x.is_finite()) {
            return Err("unmatched/nonfinite enhancement".into());
        }
        let out = enhanced
            .iter()
            .map(|&sample| {
                let original = self.original.pop_front().expect("length checked");
                if self.weight == 0.0 {
                    sample
                } else if self.weight == 1.0 {
                    original
                } else {
                    (1.0 - self.weight) * sample + self.weight * original
                }
            })
            .collect();
        self.mix_us += us(started);
        Ok(out)
    }
    fn finish(&self) -> Result<()> {
        if !self.original.is_empty() {
            return Err("unconsumed original tail".into());
        }
        Ok(())
    }
}

fn render(
    cache: &mut FrontendCache,
    input: &[f32],
    name: &str,
    root: &Path,
    weight: f32,
    schedule: &[usize],
) -> Result<(Vec<f32>, Value)> {
    if schedule.is_empty() || schedule.contains(&0) {
        return Err("empty packet schedule".into());
    }
    let started = Instant::now();
    let flag = AtomicBool::new(false);
    let config = config(name);
    let mut session = cache.start(&config, root, &flag)?;
    let mut mixer = AlignedMix::new(weight)?;
    let mut output = Vec::with_capacity(input.len());
    let mut offset = 0;
    for &size in schedule.iter().cycle() {
        if offset == input.len() {
            break;
        }
        let end = (offset + size).min(input.len());
        mixer.push_original(&input[offset..end])?;
        output.extend(mixer.accept(&session.push(&input[offset..end], 16000, &flag)?)?);
        offset = end;
    }
    output.extend(mixer.accept(&session.finish(&flag)?)?);
    mixer.finish()?;
    if output.len() != input.len() {
        return Err("output length mismatch".into());
    }
    let metrics = json!({"wallUs":us(started),"mixUs":mixer.mix_us,
        "maxOriginalPendingSamples":mixer.max_pending,"frontend":session.metrics()});
    Ok((output, metrics))
}

fn level(input: &[f32]) -> Value {
    let rms = (input.iter().map(|&x| f64::from(x).powi(2)).sum::<f64>()
        / input.len().max(1) as f64)
        .sqrt();
    json!({"samples":input.len(),"rms":rms,"rmsDbfs":if rms>0.0 {Some(20.0*rms.log10())} else {None},
        "peak":input.iter().map(|x| x.abs()).fold(0.0_f32, f32::max),
        "overFullScale":input.iter().filter(|x| x.abs()>1.0).count()})
}
fn save_audio(root: &Path, profile: &str, row: &Value, audio: &[f32]) -> Result<Value> {
    let name = format!("{profile}/{}.wav", row["id"].as_str().ok_or("sample id")?);
    let path = root.join("corpus").join(&name);
    fs::create_dir_all(path.parent().ok_or("parent")?)?;
    CapturedAudio::new(audio.to_vec(), 16000, 1)?.write_wav(&path)?;
    let mut sample = row.clone();
    sample["audioPath"] = name.into();
    sample["sha256"] = hash(&fs::read(path)?).into();
    Ok(sample)
}
fn input(root: &Path, row: &Value) -> Result<Vec<f32>> {
    let path = root
        .join("input")
        .join(row["audioPath"].as_str().ok_or("audio path")?);
    if hash(&fs::read(&path)?) != row["sha256"].as_str().ok_or("sha256")? {
        return Err("input integrity mismatch".into());
    }
    Ok(vesper_audio::decode_file(path, 0)?.samples().to_vec())
}

fn prepare(root: &Path, models: &Path) -> Result<()> {
    fs::create_dir(root.join("corpus"))?;
    fs::create_dir(root.join("manifests"))?;
    let manifest = read(&root.join("quality.json"))?;
    let samples = manifest["samples"].as_array().ok_or("samples")?;
    let mut records = Vec::new();
    let mut cache = FrontendCache::default();
    for name in ["raw", "dpdfnet4", "dpdfnet8"] {
        let profiles = if name == "raw" {
            vec!["raw".to_owned()]
        } else {
            vec![
                format!("{name}-full"),
                format!("{name}-mix50"),
                format!("{name}-pregain-restore"),
                format!("{name}-postgain"),
            ]
        };
        let mut outputs = vec![Vec::new(); profiles.len()];
        for (index, row) in samples.iter().enumerate() {
            let diagnostic =
                row["condition"] == "quiet30" && row["evaluationRole"] == "observed-regression";
            let attempt = (|| -> Result<()> {
                let original = input(root, row)?;
                if name == "raw" {
                    outputs[0].push(save_audio(root, "raw", row, &original)?);
                    return Ok(());
                }
                let (full, metrics) = render(&mut cache, &original, name, models, 0.0, &[320])?;
                let mut mixer = AlignedMix::new(0.5)?;
                mixer.push_original(&original)?;
                let mixed = mixer.accept(&full)?;
                mixer.finish()?;
                outputs[0].push(save_audio(root, &profiles[0], row, &full)?);
                outputs[1].push(save_audio(root, &profiles[1], row, &mixed)?);
                let mut record = json!({"sampleId":row["id"],"denoiser":name,"metrics":metrics,"mix50Us":mixer.mix_us,
                    "originalLevel":level(&original),"fullLevel":level(&full),"mixLevel":level(&mixed),
                    "originalEnhancedDotProduct":original.iter().zip(&full).map(|(&x,&y)| f64::from(x)*f64::from(y)).sum::<f64>()});
                if diagnostic {
                    let boosted = original.iter().map(|x| x / GAIN).collect::<Vec<_>>();
                    let (pregain, gain_metrics) =
                        render(&mut cache, &boosted, name, models, 0.0, &[320])?;
                    let restored = pregain.iter().map(|x| x * GAIN).collect::<Vec<_>>();
                    let postgain = full.iter().map(|x| x / GAIN).collect::<Vec<_>>();
                    outputs[2].push(save_audio(root, &profiles[2], row, &restored)?);
                    outputs[3].push(save_audio(root, &profiles[3], row, &postgain)?);
                    let energy = full.iter().map(|&x| f64::from(x).powi(2)).sum::<f64>();
                    record["gainDiagnostic"] = json!({"metrics":gain_metrics,"boostedInput":level(&boosted),
                        "pregainRestored":level(&restored),"postgain":level(&postgain),
                        "relativeWaveformDifference":(full.iter().zip(&restored).map(|(&x,&y)| (f64::from(x)-f64::from(y)).powi(2)).sum::<f64>()/energy.max(1e-30)).sqrt()});
                }
                records.push(record);
                Ok(())
            })();
            if let Err(error) = attempt {
                records
                    .push(json!({"sampleId":row["id"],"denoiser":name,"error":error.to_string()}));
                // Preserve an expected row with a missing waveform; the evaluator records failure.
                for (profile, rows) in profiles.iter().zip(&mut outputs) {
                    if (profile.contains("gain") && !diagnostic)
                        || rows.last().is_some_and(|r| r["id"] == row["id"])
                    {
                        continue;
                    }
                    let mut failed = row.clone();
                    failed["audioPath"] =
                        format!("{profile}/{}.wav", row["id"].as_str().unwrap_or("invalid")).into();
                    failed["sha256"] = "0".repeat(64).into();
                    rows.push(failed);
                }
            }
            write(&root.join("preprocessing.json"), &json!({"rows":records}))?;
            eprintln!("prepared {name} {}/{}", index + 1, samples.len());
        }
        for (profile, rows) in profiles.iter().zip(outputs) {
            let mut value = manifest.clone();
            value["samples"] = rows.into();
            write(
                &root.join("manifests").join(format!("{profile}.json")),
                &value,
            )?;
        }
    }
    Ok(())
}

#[derive(Default)]
struct Observation {
    first_ms: Option<f64>,
    updates: usize,
    changed: usize,
    latest: String,
}

fn stream(root: &Path, models: &Path, profile: &str, output: &Path) -> Result<()> {
    if ![
        "raw",
        "dpdfnet4-full",
        "dpdfnet4-mix50",
        "dpdfnet8-full",
        "dpdfnet8-mix50",
    ]
    .contains(&profile)
    {
        return Err("unknown streaming profile".into());
    }
    let manifest_path = root.join("streaming.json");
    let manifest = BenchmarkManifest::from_path(&manifest_path)?;
    let mut cache = FrontendCache::default();
    let manager = InferenceManager::new()?;
    let mut asr = config("raw");
    asr.models.asr = AsrModel::Nemotron35AsrStreaming06b;
    let frontend_config = config(profile);
    let mut rows = Vec::new();
    for sample in &manifest.samples {
        let row = serde_json::to_value(sample)?;
        let input_result = input(root, &row);
        let observations = Arc::new(Mutex::new(Observation::default()));
        let observed = Arc::clone(&observations);
        let started = Instant::now();
        let mut timings = json!({});
        let attempt = (|| -> Result<String> {
            let audio = input_result?;
            let flag = AtomicBool::new(false);
            let mut frontend = if profile == "raw" {
                None
            } else {
                Some(cache.start(&frontend_config, models, &flag)?)
            };
            let mut mix = AlignedMix::new(if profile.ends_with("mix50") { 0.5 } else { 0.0 })?;
            let session = manager.start_stream(
                &asr,
                models,
                &sample.id,
                Arc::new(AtomicBool::new(false)),
                Arc::new(move |update| {
                    if let Ok(mut observation) = observed.lock() {
                        observation.updates += 1;
                        if !update.is_final
                            && !update.provisional_text.trim().is_empty()
                            && observation.first_ms.is_none()
                        {
                            observation.first_ms = Some(ms(started));
                        }
                        let text = if update.is_final {
                            update.committed_text
                        } else {
                            update.provisional_text
                        };
                        if observation.latest != text {
                            observation.changed += 1;
                            observation.latest = text;
                        }
                    }
                }),
            )?;
            let feed = Instant::now();
            let mut max_late = 0.0_f64;
            let mut blocked_us = 0_u64;
            for (index, block) in audio.chunks(320).enumerate() {
                // A real callback receives a completed packet, so wait for its end deadline first.
                let delivered = ((index + 1) * 320).min(audio.len());
                let deadline = Duration::from_secs_f64(delivered as f64 / 16000.0);
                if let Some(wait) = deadline.checked_sub(feed.elapsed()) {
                    thread::sleep(wait);
                }
                max_late = max_late
                    .max(feed.elapsed().as_secs_f64() * 1000.0 - deadline.as_secs_f64() * 1000.0);
                let processed = if let Some(frontend) = &mut frontend {
                    mix.push_original(block)?;
                    mix.accept(&frontend.push(block, 16000, &flag)?)?
                } else {
                    block.to_vec()
                };
                if !processed.is_empty() {
                    let push = Instant::now();
                    session.push(processed, 16000)?;
                    blocked_us += us(push);
                }
            }
            let feed_ms = ms(feed);
            let stop = Instant::now();
            if let Some(frontend) = &mut frontend {
                let tail = mix.accept(&frontend.finish(&flag)?)?;
                mix.finish()?;
                if !tail.is_empty() {
                    session.push(tail, 16000)?;
                }
            }
            let raw = session.finish()?;
            let finished = Instant::now();
            let stop_ms = finished
                .duration_since(feed + Duration::from_secs_f64(audio.len() as f64 / 16000.0))
                .as_secs_f64()
                * 1000.0;
            let feed_finished_to_final_ms = finished.duration_since(stop).as_secs_f64() * 1000.0;
            let finish_ms = finished.duration_since(started).as_secs_f64() * 1000.0;
            timings = json!({"feedMs":feed_ms,"streamFinishMs":finish_ms,"stopToFinalMs":stop_ms,
                "feedFinishedToFinalMs":feed_finished_to_final_ms,
                "maxDeliveryLatenessMs":max_late,"asrPushWaitUs":blocked_us,"mixUs":mix.mix_us,
                "maxOriginalPendingSamples":mix.max_pending,"frontend":frontend.as_ref().map(|f| f.metrics())});
            Ok(raw)
        })();
        let observed = observations.lock().map_err(|e| e.to_string())?;
        let (raw, error) = match attempt {
            Ok(raw) => (Some(raw), None),
            Err(e) => (None, Some(e.to_string())),
        };
        let natural = sample.replay_kind == ReplayKind::Natural;
        rows.push(json!({"sampleId":sample.id,"audioSeconds":sample.duration_seconds,"profile":profile,
            "firstPartialMs":observed.first_ms,"partialUpdates":observed.updates,"changedUpdates":observed.changed,
            "observerRetainedSnapshots":1,"rawScore":raw.as_ref().filter(|_|natural).map(|t|score(&sample.reference,t)),
            "rawHypothesis":raw,"error":error,"timings":timings,"execution":manager.execution_evidence(&sample.id)}));
        write(
            output,
            &json!({"profile":profile,"manifestSha256":hash(&fs::read(&manifest_path)?),
            "modelVersions":vesper_inference::model_versions(&asr),"frontendModelVersions":vesper_inference::model_versions(&frontend_config)["audioFrontend"],
            "artifactProvenance":vesper_models::provenance(&asr),"runs":rows}),
        )?;
        eprintln!("streamed {profile} {} error={}", sample.id, error.is_some());
    }
    Ok(())
}

fn contracts(root: &Path, models: &Path) -> Result<()> {
    let manifest = read(&root.join("quality.json"))?;
    let audio = input(root, &manifest["samples"][0])?;
    let mut cache = FrontendCache::default();
    let mut comparisons = 0;
    for name in ["dpdfnet4", "dpdfnet8"] {
        for len in [0, 1, 159, 160, 161, 639, 640, 641, audio.len()] {
            let (a, _) = render(&mut cache, &audio[..len], name, models, 0.5, &[320])?;
            let (b, m) = render(
                &mut cache,
                &audio[..len],
                name,
                models,
                0.5,
                &[1, 319, 1053, 160],
            )?;
            assert_eq!(a.len(), b.len());
            assert!(a.iter().zip(&b).all(|(x, y)| (x - y).abs() <= 1e-6));
            assert!(m["maxOriginalPendingSamples"].as_u64().unwrap() <= 1053 + 1024);
            comparisons += 1;
        }
        let (a, _) = render(&mut cache, &audio, name, models, 0.5, &[320])?;
        {
            let flag = AtomicBool::new(false);
            let mut session = cache.start(&config(name), models, &flag)?;
            session.push(&[0.8; 320], 16000, &flag)?;
        }
        let (b, _) = render(&mut cache, &audio, name, models, 0.5, &[320])?;
        assert_eq!(a, b);
        let mut impulses = vec![0.0; 1601];
        for position in [0, 159, 1600] {
            impulses[position] = 0.75;
        }
        let (original, _) = render(&mut cache, &impulses, name, models, 1.0, &[1, 319, 1053])?;
        assert_eq!(
            original, impulses,
            "unaltered endpoints and onset/Stop impulses"
        );
    }
    write(
        &root.join("contracts.json"),
        &json!({"packetTailComparisons":comparisons,"dropResetComparisons":2,"passed":true}),
    )
}

fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["prepare", root, models] => prepare(Path::new(root), Path::new(models)),
        ["contracts", root, models] => contracts(Path::new(root), Path::new(models)),
        ["stream", root, models, profile, output] => stream(
            Path::new(root),
            Path::new(models),
            profile,
            Path::new(output),
        ),
        _ => Err(
            "denoising_followup prepare|contracts ROOT MODELS; stream ROOT MODELS PROFILE OUTPUT"
                .into(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn delayed_original_matches_only_valid_enhanced_samples() {
        for weight in [0.0, 0.5, 1.0] {
            let mut mixer = AlignedMix::new(weight).unwrap();
            mixer.push_original(&[1.0, 2.0, 3.0]).unwrap();
            assert!(mixer.accept(&[]).unwrap().is_empty());
            assert_eq!(mixer.accept(&[0.0]).unwrap(), vec![weight]);
            assert!(mixer.finish().is_err());
            assert!(mixer.accept(&[0.0, 0.0, 0.0]).is_err());
            assert_eq!(
                mixer.accept(&[0.0, 0.0]).unwrap(),
                vec![2.0 * weight, 3.0 * weight]
            );
            mixer.finish().unwrap();
        }
    }
    #[test]
    fn nonfinite_or_invalid_mix_cannot_enter_audio() {
        assert!(AlignedMix::new(f32::NAN).is_err());
        assert!(AlignedMix::new(1.1).is_err());
        let mut mixer = AlignedMix::new(0.5).unwrap();
        assert!(mixer.push_original(&[f32::NAN]).is_err());
        mixer.push_original(&[0.2]).unwrap();
        assert!(mixer.accept(&[f32::INFINITY]).is_err());
        assert_eq!(mixer.accept(&[0.4]).unwrap(), vec![0.3]);
    }
}
