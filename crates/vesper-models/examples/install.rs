use std::{fs, path::PathBuf, sync::atomic::AtomicBool};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| "models/downloads".into());
    let mut config = vesper_core::AppConfig::default();
    let mut args = std::env::args().skip(2);
    let mut asr_selected = false;
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--asr-model" => {
                if asr_selected {
                    return Err("select exactly one of --asr-model or --asr-profile".into());
                }
                let id = args.next().ok_or("--asr-model requires a model ID")?;
                config.models.asr = serde_json::from_value(serde_json::Value::String(id))?;
                config.models.custom_asr = None;
                config.cleanup_enabled = false;
                asr_selected = true;
                if config.models.asr == vesper_core::AsrModel::CohereTranscribe {
                    config.language = vesper_core::Language::English;
                }
            }
            "--asr-profile" => {
                if asr_selected {
                    return Err("select exactly one of --asr-model or --asr-profile".into());
                }
                let path = args.next().ok_or("--asr-profile requires a JSON path")?;
                config.models.custom_asr = Some(serde_json::from_slice(&fs::read(path)?)?);
                config.cleanup_enabled = false;
                config.language = vesper_core::Language::Auto;
                asr_selected = true;
            }
            "--asr-engine" => {
                let id = args
                    .next()
                    .ok_or("--asr-engine requires standard or unified_gguf")?;
                config.models.asr_engine = serde_json::from_value(serde_json::Value::String(id))?;
                config.language = vesper_core::Language::Auto;
            }
            "--backend" => {
                let id = args.next().ok_or("--backend requires a backend ID")?;
                config.compute_backend = serde_json::from_value(serde_json::Value::String(id))?;
            }
            _ => return Err(format!("unknown option: {argument}").into()),
        }
    }
    let result = vesper_models::install(&config, &root, &AtomicBool::new(false), |progress| {
        if progress.phase == vesper_models::InstallPhase::Installed {
            println!(
                "Verified {} ({} / {} bytes)",
                progress.file_name, progress.completed_bytes, progress.total_bytes
            );
        }
    })?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}
