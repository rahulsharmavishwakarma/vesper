//! Installed-writer paired screen; all failed outputs remain in the result set.
use serde_json::{json, Value};
use std::{
    fs,
    path::Path,
    sync::{atomic::AtomicBool, Arc},
    time::Instant,
};
use vesper_core::{
    AppConfig, AsrModel, ComputeBackend, Language, ModelSelection, PersonalizationProfile,
};
use vesper_inference::InferenceManager;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 4 {
        return Err("usage: automatic_writing MODEL_ROOT FIXTURE OUTPUT cpu|metal".into());
    }
    let fixture: Value = serde_json::from_slice(&fs::read(&args[1])?)?;
    let config = AppConfig {
        models: ModelSelection {
            asr: AsrModel::Nemotron35AsrStreaming06b,
            ..Default::default()
        },
        compute_backend: match args[3].as_str() {
            "cpu" => ComputeBackend::Cpu,
            "metal" => ComputeBackend::Metal,
            _ => return Err("explicit backend required".into()),
        },
        ..Default::default()
    };
    let manager = InferenceManager::new()?;
    let cold = Instant::now();
    manager.rewrite(
        "The meeting begins at noon",
        &config,
        Path::new(&args[0]),
        Arc::new(AtomicBool::new(false)),
    )?;
    let cold_ms = cold.elapsed().as_secs_f64() * 1000.;
    let mut rows = vec![];
    for (index, case) in fixture["cases"]
        .as_array()
        .ok_or("cases required")?
        .iter()
        .enumerate()
    {
        let raw = case["raw"].as_str().ok_or("raw required")?;
        let config = AppConfig {
            language: serde_json::from_value::<Language>(case["language"].clone())?,
            cleanup_preset: serde_json::from_value(case["preset"].clone())?,
            ..config.clone()
        };
        let mut results = serde_json::Map::new();
        for automatic in if index % 2 == 0 {
            [false, true]
        } else {
            [true, false]
        } {
            let operation = format!("writing-{index}-{automatic}");
            let profile = PersonalizationProfile {
                automatic_writing: automatic,
                ..Default::default()
            };
            let start = Instant::now();
            let output = manager.rewrite_personalized(
                raw,
                &config,
                Path::new(&args[0]),
                &operation,
                Arc::new(AtomicBool::new(false)),
                &profile,
            );
            let elapsed = start.elapsed().as_secs_f64() * 1000.;
            results.insert(if automatic {"candidate"} else {"baseline"}.into(),json!({"text":output.as_ref().ok(),"error":output.as_ref().err().map(ToString::to_string),"elapsedMs":elapsed,"execution":manager.execution_evidence(&operation)}));
        }
        rows.push(json!({"case":case,"results":results}));
    }
    let report = json!({"schemaVersion":1,"evidenceClass":fixture["evidence"],"coldWriterMs":cold_ms,"backend":args[3],"rows":rows,"limitations":["Text-input component screen: excludes ASR, microphone, UI, insertion and store latency.","Authored labels are not independent natural-user evidence. Failures remain included."]});
    fs::write(&args[2], serde_json::to_vec_pretty(&report)?)?;
    println!(
        "{} paired cases written",
        report["rows"].as_array().unwrap().len()
    );
    Ok(())
}
