//! Decode/resample an audio file and run real native inference.
//! cargo run -p transcribe-inference --example pipeline -- <model-root> <audio.wav> [--asr-only] [--q8] [--cpu] [--english|--hindi|--french]

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::time::Instant;
use transcribe_core::{AppConfig, ComputeBackend, Language, RewriteQuantization};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let model_root = PathBuf::from(
        args.next()
            .ok_or("usage: pipeline <model-root> <audio-file> [--asr-only] [--q8] [--cpu] [--english|--hindi|--french]")?,
    );
    let filename = args.next().ok_or("missing audio filename")?;
    let mut config = AppConfig::default();
    for argument in args {
        match argument.as_str() {
            "--asr-only" => config.cleanup_enabled = false,
            "--q8" => config.rewrite_quantization = RewriteQuantization::Q8_0,
            "--cpu" => config.compute_backend = ComputeBackend::Cpu,
            "--english" => config.language = Language::English,
            "--hindi" => config.language = Language::Hindi,
            "--french" => config.language = Language::French,
            _ => return Err(format!("unrecognized argument: {argument}").into()),
        }
    }
    let audio = transcribe_audio::decode_file(&filename, config.effective_recording_seconds())?;
    let started = Instant::now();
    let cancelled = AtomicBool::new(false);
    let raw = transcribe_inference::transcribe(&audio, &config, &model_root, &cancelled)?;
    let asr_elapsed = started.elapsed();
    let cleanup_started = Instant::now();
    let (cleaned, cleanup_error) = if config.cleanup_enabled && !raw.trim().is_empty() {
        match transcribe_inference::rewrite(&raw, &config, &model_root, &cancelled) {
            Ok(text) => (Some(text), None),
            Err(error) => (None, Some(error.to_string())),
        }
    } else {
        (None, None)
    };
    let cleanup_elapsed = cleanup_started.elapsed();
    let result = transcribe_inference::PipelineOutput {
        raw,
        cleaned,
        cleanup_error,
        speaker_timeline: None,
        speaker_error: None,
        model_versions: transcribe_inference::model_versions(&config),
    };
    println!("{}", serde_json::to_string_pretty(&result)?);
    eprintln!(
        "Audio {:.2}s; ASR {:.2}s; cleanup {:.2}s; native pipeline {:.2}s (each stage includes model load)",
        audio.duration_seconds(),
        asr_elapsed.as_secs_f64(),
        cleanup_elapsed.as_secs_f64(),
        started.elapsed().as_secs_f64()
    );
    if result.cleanup_error.is_some() {
        return Err("speech succeeded but cleanup failed; raw text is included above".into());
    }
    Ok(())
}
