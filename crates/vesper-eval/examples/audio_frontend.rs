//! Development-only frontend screening; production capture/inference are unchanged.
//! audio_frontend ARTIFACTS ID MANIFEST CORPUS ASR_MODELS ENV OUTPUT ASR DELIVERY CONDITION [CHUNKS]
//! ASR: none|cohere|lfm. DELIVERY: offline|chunks|realtime. CONDITION: clean|noise0|noise10|quiet.

use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sherpa_onnx::{
    AudioTagging, AudioTaggingConfig, AudioTaggingModelConfig, OfflineSpeechDenoiser,
    OfflineSpeechDenoiserConfig, OfflineSpeechDenoiserModelConfig, OnlineSpeechDenoiser,
    OnlineSpeechDenoiserConfig, SileroVadModelConfig, VadModelConfig, VoiceActivityDetector,
};
use std::{
    fs,
    path::Path,
    sync::{atomic::AtomicBool, Arc},
    time::{Duration, Instant},
};
use vesper_audio::{MonoPcm, ASR_SAMPLE_RATE};
use vesper_core::{AppConfig, AsrModel, ComputeBackend, Language};
use vesper_eval::{score, score_non_speech, BenchmarkEnvironment, BenchmarkManifest};
use vesper_inference::InferenceManager;

const SOURCE: &str = include_str!("audio_frontend.rs");

#[derive(Deserialize)]
struct Artifact {
    id: String,
    path: String,
    size: u64,
    sha256: String,
}

enum Frontend {
    Raw,
    Offline(OfflineSpeechDenoiser, bool),
    Online(OnlineSpeechDenoiser, bool),
    Vad(VoiceActivityDetector, usize),
    Tags(AudioTagging),
}

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn pcm_hash(samples: &[f32]) -> String {
    let mut h = Sha256::new();
    for sample in samples {
        h.update(sample.to_le_bytes());
    }
    format!("{:x}", h.finalize())
}
fn ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

fn artifact(rows: &[Artifact], root: &Path, id: &str) -> Result<String, String> {
    let a = rows.iter().find(|a| a.id == id).ok_or("unknown artifact")?;
    if Path::new(&a.path)
        .components()
        .any(|c| !matches!(c, std::path::Component::Normal(_)))
    {
        return Err("artifact path must be relative and contain no parent components".into());
    }
    let p = root.join(&a.path);
    let bytes = fs::read(&p).map_err(|e| e.to_string())?;
    if bytes.len() as u64 != a.size || hash(&bytes) != a.sha256 {
        return Err(format!("artifact integrity failure: {id}"));
    }
    p.to_str()
        .map(str::to_owned)
        .ok_or("non-UTF8 artifact path".into())
}

fn create(rows: &[Artifact], root: &Path, id: &str, delivery: &str) -> Result<Frontend, String> {
    if id == "raw" {
        return Ok(Frontend::Raw);
    }
    let model = artifact(rows, root, id)?;
    if id.starts_with("silero-") {
        let config = VadModelConfig {
            silero_vad: SileroVadModelConfig {
                model: Some(model),
                threshold: 0.5,
                min_silence_duration: 0.25,
                min_speech_duration: 0.064,
                window_size: 512,
                max_speech_duration: 30.0,
            },
            sample_rate: 16000,
            num_threads: 1,
            provider: Some("cpu".into()),
            ..Default::default()
        };
        return VoiceActivityDetector::create(&config, 65.0)
            .map(|m| Frontend::Vad(m, if id == "silero-v6.2" { 64 } else { 0 }))
            .ok_or("VAD create failed".into());
    }
    if id.starts_with("ced-") {
        let config = AudioTaggingConfig {
            model: AudioTaggingModelConfig {
                ced: Some(model),
                ..Default::default()
            },
            labels: Some(artifact(rows, root, "labels")?),
            top_k: 527,
        };
        return AudioTagging::create(&config)
            .map(Frontend::Tags)
            .ok_or("tagger create failed".into());
    }
    let mut config = OfflineSpeechDenoiserModelConfig::default();
    match id {
        "gtcrn" => config.gtcrn.model = Some(model),
        "dpdfnet4" | "dpdfnet8" => config.dpdfnet.model = Some(model),
        _ => return Err("unsupported frontend family".into()),
    }
    if delivery == "offline" {
        OfflineSpeechDenoiser::create(&OfflineSpeechDenoiserConfig { model: config })
            .map(|m| Frontend::Offline(m, id.starts_with("dpdfnet")))
            .ok_or("offline denoiser create failed".into())
    } else {
        OnlineSpeechDenoiser::create(&OnlineSpeechDenoiserConfig { model: config })
            .map(|m| Frontend::Online(m, id.starts_with("dpdfnet")))
            .ok_or("online denoiser create failed".into())
    }
}

/// Fixed diagnostic corruption, not a model of natural noise. Scaling uses the
/// full reference waveform's RMS; no clipping or peak normalization is applied.
fn condition(samples: &[f32], name: &str) -> Result<Vec<f32>, String> {
    let mut out = samples.to_vec();
    if name == "clean" {
        return Ok(out);
    }
    if name == "quiet" {
        for x in &mut out {
            *x *= 0.03;
        }
        return Ok(out);
    }
    let snr: f64 = match name {
        "noise0" => 0.0,
        "noise10" => 10.0,
        _ => return Err("unknown condition".into()),
    };
    let rms =
        (samples.iter().map(|x| f64::from(*x).powi(2)).sum::<f64>() / samples.len() as f64).sqrt();
    let mut seed = 0x9183_a735_u32;
    let noise: Vec<f32> = (0..samples.len())
        .map(|i| {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            (f64::from(seed) / f64::from(u32::MAX) * 2.0 - 1.0
                + 0.5 * (std::f64::consts::TAU * 120.0 * i as f64 / 16000.0).sin())
                as f32
        })
        .collect();
    let noise_rms =
        (noise.iter().map(|x| f64::from(*x).powi(2)).sum::<f64>() / noise.len() as f64).sqrt();
    let gain = rms / (10.0_f64.powf(snr / 20.0) * noise_rms);
    for (x, n) in out.iter_mut().zip(noise) {
        *x += (f64::from(n) * gain) as f32;
    }
    if out.iter().any(|x| !x.is_finite() || x.abs() > 1.0) {
        return Err("diagnostic mixture would clip; failed rather than rescaled".into());
    }
    Ok(out)
}

fn process(
    frontend: &Frontend,
    samples: &[f32],
    delivery: &str,
    packet_sizes: &[usize],
) -> Result<(Vec<f32>, Value), String> {
    let started = Instant::now();
    match frontend {
        Frontend::Raw => Ok((samples.to_vec(), json!({"computeMs":ms(started)}))),
        Frontend::Offline(model, dpdf) => {
            // Native offline iSTFT rounds down to a hop. DPDF additionally
            // shifts left 640 samples internally. Supply EOS context before
            // inference; never manufacture a missing speech tail afterwards.
            let (hop, window, tail) = if *dpdf {
                (160, 320, 640)
            } else {
                (256, 512, 0)
            };
            let mut padded = samples.to_vec();
            padded.resize((samples.len() + tail).max(window).div_ceil(hop) * hop, 0.0);
            let mut output = model.run(&padded, 16000);
            let elapsed = ms(started);
            if output.sample_rate != 16000 || output.samples.len() < samples.len() {
                return Err("denoiser returned wrong sample rate or incomplete EOS output".into());
            }
            let native_samples = output.samples.len();
            output.samples.truncate(samples.len());
            Ok((
                output.samples,
                json!({"computeMs":elapsed,"api":"offline-whole-waveform",
                    "nativeOutputSamples":native_samples,"eosPaddingSamples":padded.len()-samples.len(),
                    "nativeInternalDelayCompensationSamples":tail}),
            ))
        }
        Frontend::Tags(model) => {
            // Native offline acceptance finalizes features. Never feed it live packets.
            let stream = model.create_stream();
            stream.accept_waveform(16000, samples);
            let events = model.compute(&stream, 527);
            let elapsed = ms(started);
            if events.len() != 527 || events.iter().any(|e| !e.prob.is_finite()) {
                return Err("classifier returned invalid/incomplete event scores".into());
            }
            let values: Vec<_> = events
                .iter()
                .map(|e| json!({"id":e.index,"label":e.name,"probability":e.prob}))
                .collect();
            Ok((
                samples.to_vec(),
                json!({"computeMs":elapsed,"api":"offline-clip; no live event timestamps","events":values}),
            ))
        }
        Frontend::Online(_, _) | Frontend::Vad(_, _) => {
            match frontend {
                Frontend::Online(m, _) => m.reset(),
                Frontend::Vad(m, context) => {
                    m.reset();
                    m.clear();
                    if *context > 0 {
                        m.accept_waveform(&vec![0.0; *context]);
                    }
                }
                _ => unreachable!(),
            }
            let mut output = Vec::new();
            let mut timings = Vec::new();
            let mut segments = Vec::new();
            let mut first_output = None;
            let mut lag_ms = 0.0_f64;
            let mut accepted = 0;
            let mut detected_chunks = 0;
            for size in packet_sizes.iter().cycle() {
                if accepted == samples.len() {
                    break;
                }
                let end = (accepted + size).min(samples.len());
                let chunk = &samples[accepted..end];
                accepted += chunk.len();
                if delivery == "realtime" {
                    let deadline = started + Duration::from_secs_f64(accepted as f64 / 16000.0);
                    if let Some(wait) = deadline.checked_duration_since(Instant::now()) {
                        std::thread::sleep(wait);
                    }
                    lag_ms = lag_ms.max(
                        Instant::now()
                            .saturating_duration_since(deadline)
                            .as_secs_f64()
                            * 1000.0,
                    );
                }
                let call = Instant::now();
                match frontend {
                    Frontend::Online(m, _) => {
                        let data = m.run(chunk, 16000);
                        // Native C API returns null for a buffered empty block;
                        // the Rust wrapper represents that as empty/rate zero.
                        if !data.samples.is_empty() && data.sample_rate != 16000 {
                            return Err("streaming denoiser sample rate mismatch".into());
                        }
                        if !data.samples.is_empty() && first_output.is_none() {
                            first_output = Some(accepted as f64 / 16.0);
                        }
                        output.extend(data.samples);
                    }
                    Frontend::Vad(m, _) => {
                        m.accept_waveform(chunk);
                        detected_chunks += usize::from(m.detected());
                        drain_vad(m, &mut segments);
                    }
                    _ => unreachable!(),
                }
                timings.push(ms(call));
            }
            let stop = Instant::now();
            let delay = match frontend {
                Frontend::Online(_, true) => 640,
                _ => 0,
            };
            let vad_context = match frontend {
                Frontend::Vad(_, context) => *context,
                _ => 0,
            };
            let mut vad_tail = 0;
            match frontend {
                Frontend::Online(m, _) => {
                    if delay > 0 {
                        let data = m.run(&vec![0.0; delay], 16000);
                        if !data.samples.is_empty() && data.sample_rate != 16000 {
                            return Err("EOS sample rate mismatch".into());
                        }
                        output.extend(data.samples);
                    }
                    let data = m.flush();
                    if !data.samples.is_empty() && data.sample_rate != 16000 {
                        return Err("flush sample rate mismatch".into());
                    }
                    output.extend(data.samples);
                    if output.len() != samples.len() + delay {
                        return Err("streaming denoiser incomplete cumulative output".into());
                    }
                    // Unlike offline DPDF, online DPDF does not remove its
                    // 640-sample signal delay. EOS context drains its tail.
                    output.drain(..delay);
                }
                Frontend::Vad(m, context) => {
                    // Flush emits already-processed segments; it never infers
                    // last_. Feed a window to expose all real tail samples.
                    vad_tail = 512 + context;
                    for chunk in vec![0.0; vad_tail].chunks(320) {
                        m.accept_waveform(chunk);
                        drain_vad(m, &mut segments);
                    }
                    m.flush();
                    drain_vad(m, &mut segments);
                    clip_vad_segments(&mut segments, *context, samples.len())?;
                    output.extend_from_slice(samples);
                }
                _ => unreachable!(),
            }
            let flush_ms = ms(stop);
            let compute_ms = timings.iter().sum::<f64>() + flush_ms;
            Ok((
                output,
                json!({"computeMs":compute_ms,"wallMs":ms(started),"chunkCallMs":timings,
                "flushMs":flush_ms,"firstReturnedOutputInputMs":first_output,"maximumDeliveryLatenessMs":lag_ms,
                "eosPaddingSamples":delay,"hostDelayCompensationSamples":delay,
                "vadPrefixContextSamples":vad_context,"vadTailPaddingSamples":vad_tail,
                "vadTimestampPolicy":"native buffer intervals intersected with real input; subtract prefix context",
                "segments":segments,"detectedChunks":detected_chunks,"chunkSamples":packet_sizes,
                "vadPolicy":"decision-only, original audio unchanged; threshold.5/minspeech.064/minsilence.25/maxspeech30",
                "pace":delivery,"queue":"synchronous replay; lateness measured, no actual microphone queue"}),
            ))
        }
    }
}

fn drain_vad(model: &VoiceActivityDetector, output: &mut Vec<Value>) {
    while let Some(segment) = model.front() {
        output.push(json!({"start":segment.start(),"samples":segment.n()}));
        model.pop();
    }
}

fn clip_vad_segments(
    segments: &mut Vec<Value>,
    prefix: usize,
    length: usize,
) -> Result<(), String> {
    let mut clipped = Vec::new();
    for segment in segments.iter() {
        let start = segment["start"]
            .as_u64()
            .ok_or("invalid VAD segment start")?;
        let count = segment["samples"]
            .as_u64()
            .ok_or("invalid VAD segment length")?;
        let end = start
            .checked_add(count)
            .ok_or("VAD segment overflow")?
            .min((prefix + length) as u64);
        let start = start.max(prefix as u64);
        if end > start {
            clipped.push(json!({"start":start-prefix as u64,"samples":end-start}));
        }
    }
    *segments = clipped;
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if !(10..=11).contains(&args.len()) {
        return Err("usage: audio_frontend ARTIFACTS ID MANIFEST CORPUS ASR_MODELS ENV OUTPUT ASR DELIVERY CONDITION [comma-separated packet sizes]".into());
    }
    let [artifacts_path, id, manifest_path, corpus, asr_models, environment, output_path, asr, delivery, corruption] =
        <&[String; 10]>::try_from(&args[..10])?;
    let packet_sizes = parse_packet_sizes(args.get(10).map(String::as_str).unwrap_or("320"))?;
    if !matches!(delivery.as_str(), "offline" | "chunks" | "realtime") {
        return Err("invalid delivery".into());
    }
    if !matches!(asr.as_str(), "none" | "cohere" | "lfm") {
        return Err("invalid ASR".into());
    }
    let artifacts: Vec<Artifact> = serde_json::from_slice(&fs::read(artifacts_path)?)?;
    let manifest = BenchmarkManifest::from_path(Path::new(manifest_path))?;
    let env = BenchmarkEnvironment::from_path(Path::new(environment))?;
    let setup = Instant::now();
    let frontend = create(
        &artifacts,
        Path::new(".cache/benchmarks/models/audio-frontends"),
        id,
        delivery,
    )?;
    let load_ms = ms(setup);
    let manager = if asr == "none" {
        None
    } else {
        Some(InferenceManager::new()?)
    };
    let executable = fs::read(std::env::current_exe()?)?;
    let mut report = json!({"schemaVersion":1,"date":chrono::Utc::now().to_rfc3339(),"environment":env,
        "frontend":id,"delivery":delivery,"condition":corruption,"asr":asr,"packetSizes":packet_sizes,"artifactVerificationAndLoadMs":load_ms,
        "manifestSha256":hash(&fs::read(manifest_path)?),"manifest":serde_json::from_slice::<Value>(&fs::read(manifest_path)?)?,
        "artifacts":serde_json::from_slice::<Value>(&fs::read(artifacts_path)?)?,"harnessSource":SOURCE,
        "sourceSha256":hash(SOURCE.as_bytes()),"executableSha256":hash(&executable),"runs":[],
        "limitations":["Screening, not natural noisy-dictation qualification.","Streaming delivery concerns frontend only; ASR runs after complete audio.","No frontend training or automatic speaker/filler deletion."]});
    for sample in &manifest.samples {
        let mut row = json!({"id":sample.id,"reference":sample.reference,"language":sample.language,"scenario":sample.scenario,"replayKind":sample.replay_kind});
        let result: Result<(), String> = (|| {
            let path = Path::new(corpus).join(&sample.audio_path);
            if hash(&fs::read(&path).map_err(|e| e.to_string())?) != sample.sha256 {
                return Err("audio hash mismatch".into());
            }
            let audio = vesper_audio::decode_file(&path, 0).map_err(|e| e.to_string())?;
            if audio.sample_rate() != ASR_SAMPLE_RATE
                || audio.samples().is_empty()
                || audio.duration_seconds() > if asr == "none" { 600.0 } else { 60.0 }
            {
                return Err("screen requires nonempty16kHz; ASR max60s, frontend max600s".into());
            }
            if (audio.duration_seconds() - sample.duration_seconds).abs() > 0.01 {
                return Err("manifest duration mismatch".into());
            }
            let input = if sample.reference.trim().is_empty() {
                audio.samples().to_vec()
            } else {
                condition(audio.samples(), corruption)?
            };
            row["audioSeconds"] = json!(audio.duration_seconds());
            row["inputPcmSha256"] = json!(pcm_hash(&input));
            let (enhanced, stats) = process(&frontend, &input, delivery, &packet_sizes)?;
            row["frontend"] = stats;
            row["outputSamples"] = json!(enhanced.len());
            row["inputSamples"] = json!(input.len());
            row["outputPcmSha256"] = json!(pcm_hash(&enhanced));
            if enhanced.len() != input.len() || enhanced.iter().any(|x| !x.is_finite()) {
                return Err("frontend violated sample length/finite-output contract".into());
            }
            // Development artifacts permit signal alignment and tail inspection.
            // Content-addressed names never derive filesystem paths from text.
            let pcm_dir = Path::new(output_path)
                .parent()
                .unwrap_or(Path::new("."))
                .join("pcm");
            fs::create_dir_all(&pcm_dir).map_err(|e| e.to_string())?;
            for waveform in [&input, &enhanced] {
                let bytes: Vec<_> = waveform.iter().flat_map(|x| x.to_le_bytes()).collect();
                fs::write(pcm_dir.join(format!("{}.f32", hash(&bytes))), bytes)
                    .map_err(|e| e.to_string())?;
            }
            if let Some(manager) = &manager {
                if !sample.language.starts_with("en") && !sample.language.starts_with("hi") {
                    return Err(
                        "this screen implements only explicit English/Hindi ASR prompts".into(),
                    );
                }
                if asr == "lfm" && !sample.language.starts_with("en") {
                    return Err("LFM adapter language unsupported; retained as failure".into());
                }
                let config = AppConfig {
                    compute_backend: ComputeBackend::Cpu,
                    language: if sample.language.starts_with("hi") {
                        Language::Hindi
                    } else {
                        Language::English
                    },
                    models: vesper_core::ModelSelection {
                        asr: if asr == "cohere" {
                            AsrModel::CohereTranscribe
                        } else {
                            AsrModel::Lfm25Audio15b
                        },
                        ..Default::default()
                    },
                    ..Default::default()
                };
                let audio = MonoPcm::new(enhanced, 16000).map_err(|e| e.to_string())?;
                row["modelVersions"] = json!(vesper_inference::model_versions(&config));
                let op = format!("frontend-{}-{}", id, sample.id);
                let started = Instant::now();
                let raw = manager.transcribe(
                    &audio,
                    &config,
                    Path::new(asr_models),
                    &op,
                    Arc::new(AtomicBool::new(false)),
                );
                row["asrMs"] = json!(ms(started));
                row["execution"] = manager.execution_evidence(&op);
                let raw = raw.map_err(|e| e.to_string())?;
                row["raw"] = json!(raw);
                if sample.reference.trim().is_empty() {
                    row["nonSpeechScore"] = json!(score_non_speech(&raw, audio.duration_seconds()));
                } else {
                    row["rawScore"] = json!(score(&sample.reference, &raw));
                }
                if !raw.trim().is_empty() {
                    let started = Instant::now();
                    let cleaned = manager.rewrite(
                        &raw,
                        &config,
                        Path::new(asr_models),
                        Arc::new(AtomicBool::new(false)),
                    );
                    row["cleanupMs"] = json!(ms(started));
                    match cleaned {
                        Ok(text) => {
                            row["cleaned"] = json!(text);
                            if !sample.reference.trim().is_empty() {
                                row["cleanedScore"] = json!(score(&sample.reference, &text));
                            } else {
                                row["cleanedNonSpeechScore"] =
                                    json!(score_non_speech(&text, audio.duration_seconds()));
                            }
                        }
                        Err(e) => row["cleanupError"] = json!(e.to_string()),
                    }
                }
            }
            Ok(())
        })();
        row["error"] = json!(result.err());
        eprintln!("{id} {}: {}", sample.id, row["error"]);
        report["runs"].as_array_mut().unwrap().push(row);
        fs::write(output_path, serde_json::to_vec_pretty(&report)?)?;
    }
    Ok(())
}

fn parse_packet_sizes(value: &str) -> Result<Vec<usize>, String> {
    let sizes: Vec<_> = value
        .split(',')
        .map(|v| {
            v.parse::<usize>()
                .map_err(|_| "invalid packet size".to_owned())
        })
        .collect::<Result<_, _>>()?;
    if sizes.is_empty() || sizes.len() > 32 || sizes.iter().any(|s| !(1..=16000).contains(s)) {
        return Err("packet sizes must contain 1..32 values, each within1..16000".into());
    }
    Ok(sizes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn packet_schedule_rejects_empty_zero_and_unbounded_sizes() {
        assert_eq!(
            parse_packet_sizes("1,319,511,1600").unwrap(),
            vec![1, 319, 511, 1600]
        );
        for invalid in ["", "0", "1,0", "-1", "16001", "1,"] {
            assert!(parse_packet_sizes(invalid).is_err());
        }
    }
    #[test]
    fn vad_context_and_eos_never_escape_original_sample_coordinates() {
        let mut segments = vec![
            json!({"start":0,"samples":32}),
            json!({"start":32,"samples":600}),
            json!({"start":800,"samples":512}),
        ];
        clip_vad_segments(&mut segments, 64, 512).unwrap();
        assert_eq!(segments, vec![json!({"start":0,"samples":512})]);
        assert!(clip_vad_segments(&mut vec![json!({"start":-1,"samples":20})], 64, 512).is_err());
    }
    #[test]
    fn diagnostic_noise_is_reproducible_and_preserves_silence_and_duration() {
        let input: Vec<_> = (0..16000).map(|i| 0.02 * (i as f32 * 0.03).sin()).collect();
        assert_eq!(
            condition(&input, "noise0").unwrap(),
            condition(&input, "noise0").unwrap()
        );
        assert_eq!(condition(&input, "noise0").unwrap().len(), input.len());
        assert_eq!(condition(&[0.0; 32], "noise10").unwrap(), vec![0.0; 32]);
        assert!(condition(&input, "invalid").is_err());
    }
}
