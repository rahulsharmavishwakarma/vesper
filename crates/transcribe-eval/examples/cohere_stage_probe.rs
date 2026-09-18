//! Isolated measurement of the existing per-job Cohere contract; no production changes.
use serde_json::json;
use sha2::{Digest, Sha256};
use sherpa_onnx::{
    OfflineCohereTranscribeModelConfig, OfflineModelConfig, OfflineRecognizer,
    OfflineRecognizerConfig,
};
use std::{fs, path::Path, sync::atomic::AtomicBool, time::Instant};
use transcribe_core::{AppConfig, AsrModel, ComputeBackend, Language};
use transcribe_eval::{score, BenchmarkEnvironment, BenchmarkManifest};

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 5 {
        return Err("usage: cohere_stage_probe MANIFEST CORPUS MODELS ENV OUTPUT".into());
    }
    let manifest = BenchmarkManifest::from_path(Path::new(&args[0]))?;
    let environment = BenchmarkEnvironment::from_path(Path::new(&args[3]))?;
    let config = AppConfig {
        compute_backend: ComputeBackend::Cpu,
        language: Language::English,
        models: transcribe_core::ModelSelection {
            asr: AsrModel::CohereTranscribe,
            ..Default::default()
        },
        ..Default::default()
    };
    let cancelled = AtomicBool::new(false);
    let threads = std::thread::available_parallelism()
        .map(|n| n.get().min(8) as i32)
        .unwrap_or(4);
    let model_root = Path::new(&args[2]);
    let model_path = |file: &str| {
        model_root
            .join("cohere-transcribe")
            .join(file)
            .to_string_lossy()
            .into_owned()
    };
    // Deliberately mirrors the inspected production Cohere configuration.
    // A different model/adapter must not inherit this architecture-specific probe.
    let native = OfflineRecognizerConfig {
        model_config: OfflineModelConfig {
            cohere_transcribe: OfflineCohereTranscribeModelConfig {
                encoder: Some(model_path("encoder.int8.onnx")),
                decoder: Some(model_path("decoder.int8.onnx")),
                language: Some("en".into()),
                use_punct: true,
                use_itn: false,
            },
            tokens: Some(model_path("tokens.txt")),
            provider: Some("cpu".into()),
            num_threads: threads,
            ..Default::default()
        },
        decoding_method: Some("greedy_search".into()),
        ..Default::default()
    };
    let mut report = json!({"schemaVersion":1,"manifest":manifest,"environment":environment,
        "modelVersions":transcribe_inference::model_versions(&config),"requestedNativeThreads":threads,
        "source":include_str!("cohere_stage_probe.rs"),
        "sourceSha256":hash(include_bytes!("cohere_stage_probe.rs")),
        "executableSha256":hash(&fs::read(std::env::current_exe()?)?),"runs":[],
        "limitations":["Separate verification/load/feature/decode probe, not application UI timing",
            "Per-case recognizer ownership; OS filesystem cache not flushed",
            "Completion reason diagnoses these cases only; no frontend or model parameter changes"]});
    for sample in &manifest.samples {
        let mut row = json!({"id":sample.id});
        let result: Result<(), String> = (|| {
            if sample.language != "en-US" {
                return Err("probe implements explicit English only".into());
            }
            let path = Path::new(&args[1]).join(&sample.audio_path);
            if hash(&fs::read(&path).map_err(|e| e.to_string())?) != sample.sha256 {
                return Err("audio integrity failure".into());
            }
            let audio = transcribe_audio::decode_file(path, 0).map_err(|e| e.to_string())?;
            if audio.sample_rate() != 16000
                || !(0.0..=30.0).contains(&audio.duration_seconds())
                || audio.samples().is_empty()
            {
                return Err("probe requires nonempty mono16kHz up to30s".into());
            }
            let bytes: Vec<_> = audio
                .samples()
                .iter()
                .flat_map(|x| x.to_le_bytes())
                .collect();
            row["inputPcmSha256"] = json!(hash(&bytes));
            row["audioSeconds"] = json!(audio.duration_seconds());
            let started = Instant::now();
            transcribe_models::verify_asr(&config, model_root, &cancelled)
                .map_err(|e| e.to_string())?;
            row["verificationMs"] = json!(started.elapsed().as_secs_f64() * 1000.0);
            let started = Instant::now();
            let model =
                OfflineRecognizer::create(&native).ok_or("native recognizer create failed")?;
            row["modelLoadMs"] = json!(started.elapsed().as_secs_f64() * 1000.0);
            let started = Instant::now();
            let stream = model.create_stream();
            stream.set_option("language", "en");
            stream.accept_waveform(16000, audio.samples());
            row["acceptWaveformMs"] = json!(started.elapsed().as_secs_f64() * 1000.0);
            let started = Instant::now();
            model.decode(&stream);
            row["decodeMs"] = json!(started.elapsed().as_secs_f64() * 1000.0);
            let completion = stream.get_option("transcribe.decode_report.v1");
            row["completion"] = serde_json::from_str(&completion)
                .map_err(|e| format!("invalid completion report: {e}"))?;
            let result = stream.get_result().ok_or("missing recognition result")?;
            row["raw"] = json!(result.text);
            row["rawScore"] = json!(score(&sample.reference, &result.text));
            let started = Instant::now();
            drop(stream);
            drop(model);
            row["modelReleaseMs"] = json!(started.elapsed().as_secs_f64() * 1000.0);
            let reason = row["completion"]["stopReason"].as_str();
            let accepted = (reason == Some("eos")
                || (matches!(reason, Some("silence_detected" | "no_input"))
                    && result.text.trim().is_empty()))
                && row["completion"]["inputContextTruncated"] == false;
            row["completionAccepted"] = json!(accepted);
            if !accepted {
                return Err(
                    "native completion rejected; diagnostic output is not an accepted transcript"
                        .into(),
                );
            }
            Ok(())
        })();
        row["error"] = json!(result.err());
        eprintln!("{}: {}", sample.id, row["error"]);
        report["runs"].as_array_mut().unwrap().push(row);
        fs::write(&args[4], serde_json::to_vec_pretty(&report)?)?;
    }
    Ok(())
}
