//! Controlled native cleanup/retrieval replay, not a held-out quality benchmark.
use serde_json::{json, Value};
use std::{
    env, fs,
    path::Path,
    sync::{atomic::AtomicBool, Arc},
    time::Instant,
};
use transcribe_core::{AppConfig, ComputeBackend, Language, PersonalizationProfile};
use transcribe_inference::InferenceManager;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 3 {
        return Err("usage: memory_cleanup MODEL_ROOT CASES_JSON OUTPUT_JSON".into());
    }
    let cases: Value = serde_json::from_slice(&fs::read(&args[1])?)?;
    let manager = InferenceManager::new()?;
    let config = AppConfig {
        compute_backend: ComputeBackend::Cpu,
        language: Language::English,
        ..Default::default()
    };
    let mut results = Vec::new();
    let mut failures = 0;
    for (i, case) in cases["cases"]
        .as_array()
        .ok_or("missing cases")?
        .iter()
        .enumerate()
    {
        let raw = case["raw"].as_str().ok_or("missing raw")?;
        let profile: PersonalizationProfile = serde_json::from_value(case["profile"].clone())?;
        let operation = format!("memory-cleanup-{i}");
        let started = Instant::now();
        let output = manager.rewrite_personalized(
            raw,
            &config,
            Path::new(&args[0]),
            &operation,
            Arc::new(AtomicBool::new(false)),
            &profile,
        );
        let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
        let evidence = manager.execution_evidence(&operation);
        let ids = &evidence["rewriter"]["metrics"]["personalization"]["vocabularyIds"];
        let expected = &case["expectedVocabularyIds"];
        let retrieved = &evidence["rewriter"]["metrics"]["personalization"];
        let retrieval_passed = ids == expected
            && retrieved["exampleIds"] == case["expectedExampleIds"]
            && retrieved["outputIntent"] == case["expectedOutputIntent"];
        let (cleaned, error) = match output {
            Ok(s) => (Some(s), None),
            Err(e) => (None, Some(e.to_string())),
        };
        if !retrieval_passed || error.is_some() {
            failures += 1;
        }
        let personalized = cleaned
            .as_ref()
            .map(|s| profile.personalize(s, Language::English));
        results.push(json!({"id":case["id"],"raw":raw,"reference":case["reference"],
            "cleaned":cleaned,"personalized":personalized,"elapsedMs":elapsed_ms,"execution":evidence,
            "expectedVocabularyIds":expected,"retrievalPassed":retrieval_passed,"error":error}));
    }
    fs::write(
        &args[2],
        serde_json::to_vec_pretty(
            &json!({"schemaVersion":1,"source":cases["source"],"failures":failures,"results":results}),
        )?,
    )?;
    println!(
        "{} controlled cleanup calls, {failures} retrieval/runtime failures",
        results.len()
    );
    if failures > 0 {
        return Err("controlled native cleanup failed; see artifact".into());
    }
    Ok(())
}
