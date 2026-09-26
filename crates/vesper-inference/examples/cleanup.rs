//! Inspect real cleanup outputs for one or more transcripts in a single process.
//! cargo run -p vesper-inference --example cleanup -- <model-root> [--minimal|--balanced] "text" ...

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::time::Instant;
use vesper_core::{AppConfig, CleanupPreset};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let model_root = PathBuf::from(args.next().ok_or("missing model root")?);
    let mut config = AppConfig::default();
    let mut outputs = Vec::new();
    for raw in args {
        if raw == "--minimal" {
            config.cleanup_preset = CleanupPreset::Minimal;
            continue;
        }
        if raw == "--balanced" {
            config.cleanup_preset = CleanupPreset::Balanced;
            continue;
        }
        let started = Instant::now();
        let result = vesper_inference::rewrite(&raw, &config, &model_root, &AtomicBool::new(false));
        let (cleaned, error) = match result {
            Ok(text) => (Some(text), None),
            Err(error) => (None, Some(error.to_string())),
        };
        outputs.push(serde_json::json!({"raw":raw,"cleaned":cleaned,"error":error,"preset":config.cleanup_preset,"elapsedMs":started.elapsed().as_millis()}));
    }
    if outputs.is_empty() {
        return Err("provide at least one transcript".into());
    }
    println!("{}", serde_json::to_string_pretty(&outputs)?);
    Ok(())
}
