//! Development-only Whisper screen using the already pinned native runtime.
//! This is not an application adapter or a streaming implementation.
use serde_json::json;
use sha2::{Digest, Sha256};
use sherpa_onnx::{
    OfflineModelConfig, OfflineRecognizer, OfflineRecognizerConfig, OfflineWhisperModelConfig,
};
use std::{
    fs,
    path::Path,
    sync::{atomic::AtomicBool, Arc},
    time::Instant,
};
use transcribe_core::{AppConfig, ComputeBackend, Language};
use transcribe_eval::{score, score_non_speech, BenchmarkEnvironment, BenchmarkManifest};
use transcribe_inference::InferenceManager;

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 9 {
        return Err("usage: whisper_probe MANIFEST CORPUS ARTIFACT_JSON ENV OUTPUT LANGUAGE TAIL_FRAMES WRITER_MODELS CLEANUP_BOOL".into());
    }
    let manifest = BenchmarkManifest::from_path(Path::new(&args[0]))?;
    let environment = BenchmarkEnvironment::from_path(Path::new(&args[3]))?;
    let artifacts: serde_json::Value = serde_json::from_slice(&fs::read(&args[2])?)?;
    let language = match args[5].as_str() {
        "en" | "fr" | "hi" => args[5].as_str(),
        "auto" => "",
        _ => return Err("screen supports only en/fr/hi/auto".into()),
    };
    let cleanup: bool = args[8].parse()?;
    if cleanup && language != "en" {
        return Err("this screen qualifies the existing writer only in explicit English".into());
    }
    let tail: i32 = args[6].parse()?;
    if !matches!(tail, 300 | 1000) || !(1..=8).contains(&environment.threads) {
        return Err("screen requires 300/1000 tail frames and 1..8 CPU threads".into());
    }
    // Refuse accidental replacement of evidence, including a failed attempt.
    let _output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args[4])?;
    let config = AppConfig {
        compute_backend: ComputeBackend::Cpu,
        language: Language::English,
        cleanup_enabled: cleanup,
        ..Default::default()
    };
    let mut report = json!({"schemaVersion":1,"manifest":manifest,"environment":environment,
        "artifacts":artifacts,"language":args[5],"tailPaddingFrames":tail,"cleanupEnabled":cleanup,
        "writerVersions":transcribe_inference::model_versions(&config)["rewriter"],
        "sourceSha256":hash(include_bytes!("whisper_probe.rs")),
        "executableSha256":hash(&fs::read(std::env::current_exe()?)?),"runs":[],
        "limitations":["Isolated native probe, not application Whisper support",
            "Resident recognizer; fresh OfflineStream per recording; greedy CPU INT8, timestamps off",
            "Up to 29 seconds only; longer recordings fail explicitly before native decode",
            "Native API has no reliable EOS/token-limit/error completion report; retain stderr",
            "ASR stage excludes artifact verification and model load, recorded separately once",
            "Writer uses the production InferenceManager; model load is included in its first call",
            "No microphone, streaming, endpoint, UI, sustained thermal or edge-device qualification"]});
    fs::write(&args[4], serde_json::to_vec_pretty(&report)?)?;
    let started = Instant::now();
    let artifact_path = |role: &str| -> Result<String, Box<dyn std::error::Error>> {
        let entry = &artifacts[role];
        let path = entry["path"].as_str().ok_or("missing artifact path")?;
        let bytes = fs::read(path)?;
        if entry["size"].as_u64() != Some(bytes.len() as u64)
            || entry["sha256"].as_str() != Some(hash(&bytes).as_str())
        {
            return Err(format!("artifact integrity failure: {role}").into());
        }
        Ok(path.to_owned())
    };
    let native = OfflineRecognizerConfig {
        model_config: OfflineModelConfig {
            whisper: OfflineWhisperModelConfig {
                encoder: Some(artifact_path("encoder")?),
                decoder: Some(artifact_path("decoder")?),
                language: Some(language.into()),
                task: Some("transcribe".into()),
                tail_paddings: tail,
                ..Default::default()
            },
            tokens: Some(artifact_path("tokens")?),
            provider: Some("cpu".into()),
            num_threads: environment.threads as i32,
            ..Default::default()
        },
        decoding_method: Some("greedy_search".into()),
        ..Default::default()
    };
    report["artifactVerificationMs"] = json!(started.elapsed().as_secs_f64() * 1000.0);
    let started = Instant::now();
    let model = OfflineRecognizer::create(&native).ok_or("native recognizer creation failed")?;
    report["modelLoadMs"] = json!(started.elapsed().as_secs_f64() * 1000.0);
    let writer = InferenceManager::new()?;
    for (ordinal, sample) in manifest.samples.iter().enumerate() {
        let mut row = json!({"sampleId":sample.id,"requestOrdinal":ordinal+1,
            "audioSeconds":sample.duration_seconds,"rawHypothesis":"","error":null});
        let attempt: Result<(), String> = (|| {
            let path = Path::new(&args[1]).join(&sample.audio_path);
            if hash(&fs::read(&path).map_err(|e| e.to_string())?) != sample.sha256 {
                return Err("audio integrity failure".into());
            }
            let audio = transcribe_audio::decode_file(path, 0).map_err(|e| e.to_string())?;
            if audio.sample_rate() != 16000
                || audio.samples().is_empty()
                || audio.duration_seconds() > 29.0
            {
                return Err(
                    "requires nonempty 16kHz audio of at most 29s; no silent truncation".into(),
                );
            }
            row["inputPcmSha256"] = json!(hash(
                &audio
                    .samples()
                    .iter()
                    .flat_map(|v| v.to_le_bytes())
                    .collect::<Vec<_>>()
            ));
            let started = Instant::now();
            let stream = model.create_stream();
            stream.accept_waveform(16000, audio.samples());
            row["acceptWaveformMs"] = json!(started.elapsed().as_secs_f64() * 1000.0);
            let decode_started = Instant::now();
            model.decode(&stream);
            row["decodeMs"] = json!(decode_started.elapsed().as_secs_f64() * 1000.0);
            let result = stream.get_result().ok_or("missing native result")?;
            row["rawHypothesis"] = json!(result.text);
            row["tokenCount"] = json!(result.tokens.len());
            drop(stream);
            row["asrMs"] = json!(started.elapsed().as_secs_f64() * 1000.0);
            row["completionStatus"] = json!("unknown-native-api-does-not-report-stop-reason");
            if result.text.trim().is_empty() && !sample.reference.trim().is_empty() {
                return Err("empty speech result; silence, decoder failure and exhaustion are not distinguished by native API".into());
            }
            if cleanup {
                let started = Instant::now();
                let cleaned = writer.rewrite(
                    &result.text,
                    &config,
                    Path::new(&args[7]),
                    Arc::new(AtomicBool::new(false)),
                );
                row["cleanupMs"] = json!(started.elapsed().as_secs_f64() * 1000.0);
                row["writerEvidence"] = writer.latest_execution_evidence();
                match cleaned {
                    Ok(text) => {
                        row["cleanedScore"] = json!(score(&sample.reference, &text));
                        row["cleanedHypothesis"] = json!(text);
                    }
                    Err(error) => row["cleanupError"] = json!(error.to_string()),
                }
            }
            Ok(())
        })();
        row["error"] = json!(attempt.err());
        let raw = row["rawHypothesis"].as_str().unwrap_or_default().to_owned();
        row["rawScore"] = if sample.reference.trim().is_empty() {
            serde_json::Value::Null
        } else {
            json!(score(&sample.reference, &raw))
        };
        if sample.reference.trim().is_empty() {
            row["nonSpeechScore"] = json!(score_non_speech(&raw, sample.duration_seconds));
        }
        eprintln!(
            "{}: error={}, asrMs={}",
            sample.id, row["error"], row["asrMs"]
        );
        report["runs"].as_array_mut().unwrap().push(row);
        fs::write(&args[4], serde_json::to_vec_pretty(&report)?)?;
    }
    Ok(())
}
