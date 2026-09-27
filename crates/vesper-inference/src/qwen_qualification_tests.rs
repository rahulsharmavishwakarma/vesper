//! Frozen cleanup regressions, rejected prompts, and the accepted bounded recovery.
use super::*;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs::OpenOptions, io::Write};

fn legacy_minimal(
    model: &Rewriter,
    raw: &str,
    config: &AppConfig,
    cancelled: &AtomicBool,
) -> Result<String> {
    *model.metrics.borrow_mut() = RewriteMetrics::default();
    model
        .generate_cleanup(raw, config, false, cancelled, "")
        .and_then(|output| {
            crate::fidelity::validate(raw, &output, CleanupPreset::Minimal)?;
            Ok(output)
        })
}

const FIXTURE: &str = include_str!("../../../benchmarks/cleanup/native-learning-safety.json");
const KNOWN_FAILURES: [&str; 3] = [
    "svarah-281474976893848_f3250_chunk_2",
    "svarah-281474976891471_f1218_chunk_0",
    "svarah-281474976889300_f3157_chunk_0",
];

#[test]
#[ignore = "installed Qwen; compare production recovery with the preserved original path"]
fn bounded_recovery_against_frozen_cases() {
    let fixture = if vesper_core::environment::var_os("VESPER_CLEANUP_HOLDOUT").is_some() {
        include_str!("../../../benchmarks/cleanup/native-learning-holdout.json")
    } else {
        FIXTURE
    };
    let root = PathBuf::from(vesper_core::environment::var_os("VESPER_SMOKE_MODELS").unwrap());
    let mut report = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(vesper_core::environment::var_os("VESPER_CLEANUP_REPORT").unwrap())
        .unwrap();
    let config = AppConfig {
        compute_backend: ComputeBackend::Cpu,
        ..Default::default()
    };
    let cancelled = AtomicBool::new(false);
    let mut model = Rewriter::load(&config, &root, &cancelled).unwrap();
    model.prefix_cache_enabled = false;
    let manifest: Value = serde_json::from_str(fixture).unwrap();
    let mut rows = vec![];
    let mut failures = vec![];
    for case in manifest["cases"].as_array().unwrap() {
        let raw = case["raw"].as_str().unwrap();
        let id = case["id"].as_str().unwrap();
        let baseline = legacy_minimal(&model, raw, &config, &cancelled).map_err(|e| e.to_string());
        let baseline_metrics = model.metrics.borrow().clone();
        model.metrics.borrow_mut().passes.clear();
        let candidate = model
            .rewrite(raw, &config, &cancelled)
            .map_err(|e| e.to_string());
        if (baseline.is_ok() && candidate != baseline)
            || (KNOWN_FAILURES.contains(&id) && candidate.is_err())
        {
            failures.push(id);
        }
        rows.push(json!({"id":id,"raw":raw,"baseline":baseline,"candidate":candidate,"baselineMetrics":baseline_metrics,"candidateMetrics":*model.metrics.borrow()}));
    }
    let value = json!({"fixtureSha256":format!("{:x}",Sha256::digest(fixture)),"provider":model.initialized_provider,"threads":worker_threads(),"candidate":"bounded-formatting-recovery-v1","failures":failures,"rows":rows});
    writeln!(report, "{}", serde_json::to_string_pretty(&value).unwrap()).unwrap();
    println!("{}", json!({"caseCount":rows.len(),"failures":failures}));
    assert!(
        failures.is_empty(),
        "candidate failed qualification: {failures:?}"
    );
}

#[test]
#[ignore = "installed Qwen; qualify proper conversation examples before changing production"]
fn chat_examples_against_frozen_cases() {
    let fixture = if vesper_core::environment::var_os("VESPER_CLEANUP_HOLDOUT").is_some() {
        include_str!("../../../benchmarks/cleanup/native-learning-holdout.json")
    } else {
        FIXTURE
    };
    let root = PathBuf::from(vesper_core::environment::var_os("VESPER_SMOKE_MODELS").unwrap());
    let path = vesper_core::environment::var_os("VESPER_CLEANUP_REPORT").expect("new report path");
    let mut report = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .unwrap();
    let config = AppConfig {
        compute_backend: ComputeBackend::Cpu,
        ..Default::default()
    };
    let cancelled = AtomicBool::new(false);
    let mut model = Rewriter::load(&config, &root, &cancelled).unwrap();
    model.prefix_cache_enabled = false;
    let manifest: Value = serde_json::from_str(fixture).unwrap();
    let mut rows = vec![];
    let mut failures = vec![];
    for case in manifest["cases"].as_array().unwrap() {
        let raw = case["raw"].as_str().unwrap();
        let id = case["id"].as_str().unwrap();
        let baseline = legacy_minimal(&model, raw, &config, &cancelled).map_err(|e| e.to_string());
        let baseline_metrics = model.metrics.borrow().clone();
        let prompt = formatting_retry_prompt(raw, config.effective_cleanup_instruction()).unwrap();
        let candidate = model
            .generate(
                raw,
                &prompt,
                false,
                true,
                &cancelled,
                "chat-examples",
                false,
                None,
            )
            .and_then(|text| {
                crate::fidelity::validate(raw, &text, CleanupPreset::Minimal)?;
                Ok(text)
            })
            .map_err(|e| e.to_string());
        if candidate.is_err() && (baseline.is_ok() || KNOWN_FAILURES.contains(&id)) {
            failures.push(id);
        }
        rows.push(json!({"id":id,"raw":raw,"baseline":baseline,"candidate":candidate,"baselineMetrics":baseline_metrics,"candidatePass":model.metrics.borrow().passes.last()}));
    }
    let value = json!({"fixtureSha256":format!("{:x}",Sha256::digest(fixture)),"provider":model.initialized_provider,"threads":worker_threads(),"candidate":"proper-chat-examples-v1","failures":failures,"rows":rows});
    writeln!(report, "{}", serde_json::to_string_pretty(&value).unwrap()).unwrap();
    println!("{}", json!({"caseCount":rows.len(),"failures":failures}));
    assert!(
        failures.is_empty(),
        "candidate failed qualification: {failures:?}"
    );
}

#[test]
#[ignore = "installed Qwen CPU comparison; rejected candidate is expected to fail qualification"]
fn minimal_prompt_candidate_against_frozen_cases() {
    let root = PathBuf::from(
        vesper_core::environment::var_os("VESPER_SMOKE_MODELS").expect("VESPER_SMOKE_MODELS"),
    );
    // Reserve the result before loading a model; refuse to overwrite evidence.
    let mut report = vesper_core::environment::var_os("VESPER_CLEANUP_REPORT").map(|path| {
        OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(path)
            .expect("new report path")
    });
    let config = AppConfig {
        compute_backend: ComputeBackend::Cpu,
        ..Default::default()
    };
    let cancelled = AtomicBool::new(false);
    let mut model = Rewriter::load(&config, &root, &cancelled).unwrap();
    model.prefix_cache_enabled = false;
    let fixture: Value = serde_json::from_str(FIXTURE).unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 58);
    let mut rows = vec![];
    let mut new_rejections = vec![];
    let mut resolved_rejections = vec![];
    let mut unresolved_known = vec![];
    let mut baseline_rejected = 0;
    let mut candidate_rejected = 0;
    let mut ids = std::collections::BTreeSet::new();
    for case in cases {
        let id = case["id"].as_str().unwrap();
        assert!(ids.insert(id));
        let raw = case["raw"].as_str().unwrap();
        assert!(!raw.trim().is_empty());
        let baseline =
            legacy_minimal(&model, raw, &config, &cancelled).map_err(|error| error.to_string());
        let baseline_metrics = model.metrics.borrow().clone();
        let instruction = config.effective_cleanup_instruction();
        // Keep this rejected hypothesis inside the ignored experiment. Both
        // arms use the same official nonthinking wrapper and strict guard.
        let prompt = format!("<|im_start|>system\n{instruction} Preserve contractions and repeated words exactly.\n<|im_end|>\n<|im_start|>user\n{raw}<|im_end|>\n<|im_start|>assistant\n<think>\n\n</think>\n\n");
        let candidate = model
            .generate(
                raw,
                &prompt,
                false,
                true,
                &cancelled,
                "candidate",
                false,
                None,
            )
            .and_then(|text| {
                crate::fidelity::validate(raw, &text, CleanupPreset::Minimal)?;
                Ok(text)
            })
            .map_err(|error| error.to_string());
        baseline_rejected += usize::from(baseline.is_err());
        candidate_rejected += usize::from(candidate.is_err());
        if baseline.is_ok() && candidate.is_err() {
            new_rejections.push(id);
        }
        if baseline.is_err() && candidate.is_ok() {
            resolved_rejections.push(id);
        }
        if KNOWN_FAILURES.contains(&id) && candidate.is_err() {
            unresolved_known.push(id);
        }
        rows.push(json!({"id":id,"raw":raw,"baseline":baseline,"candidate":candidate,
            "baselineMetrics":baseline_metrics,"candidatePass":model.metrics.borrow().passes.last()}));
    }
    assert!(KNOWN_FAILURES.iter().all(|id| ids.contains(id)));
    let qualified = new_rejections.is_empty() && unresolved_known.is_empty();
    let value = json!({"schemaVersion":1,"fixtureSha256":format!("{:x}",Sha256::digest(FIXTURE)),
        "evidenceClass":"development prompt comparison; reused public ASR text and authored controls, no ASR inference or human sessions",
        "provider":model.initialized_provider,"threads":worker_threads(),"prefixCache":false,
        "modelProvenance":vesper_models::provenance(&config),"modelVersions":crate::model_versions(&config),
        "caseCount":rows.len(),"baselineRejected":baseline_rejected,"candidateRejected":candidate_rejected,
        "newRejections":new_rejections,"resolvedRejections":resolved_rejections,
        "unresolvedKnown":unresolved_known,"qualified":qualified,"rows":rows,
        "limitations":["Baseline-first order; these timings are diagnostic, not a paired latency claim.",
        "Acceptance by lexical, number and negation guards is not a guarantee of semantic correctness.",
        "The candidate is test-only. Its failure must not change production admission or fidelity rules."]});
    let encoded = serde_json::to_string_pretty(&value).unwrap();
    if let Some(file) = report.as_mut() {
        file.write_all(encoded.as_bytes()).unwrap();
        file.write_all(b"\n").unwrap();
    }
    println!("{encoded}");
    assert!(
        qualified,
        "candidate failed: {}",
        json!({"newRejections":new_rejections,"unresolvedKnown":unresolved_known})
    );
}
