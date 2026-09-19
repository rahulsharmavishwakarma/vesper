//! Opt-in installed-model screen. All failures remain in denominators; no tuning.
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    time::{Duration, Instant},
};
use transcribe_core::{
    AppConfig, ComputeBackend, LearningContext, SemanticAnalysisRequest, SemanticMemory,
    SemanticStatus,
};
use transcribe_inference::InferenceManager;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 5 {
        return Err("usage: semantic_shadow MODEL_ROOT FIXTURE OUTPUT cpu|metal".into());
    }
    let root = PathBuf::from(&args[1]);
    let fixture: Value = serde_json::from_slice(&std::fs::read(&args[2])?)?;
    let config = AppConfig {
        models: transcribe_core::ModelSelection {
            asr: transcribe_core::AsrModel::Nemotron35AsrStreaming06b,
            ..Default::default()
        },
        compute_backend: match args[4].as_str() {
            "cpu" => ComputeBackend::Cpu,
            "metal" => ComputeBackend::Metal,
            _ => return Err("explicit backend required".into()),
        },
        ..Default::default()
    };
    let manager = InferenceManager::new()?;
    let started = Instant::now();
    // This foreground operation, not background work, owns model loading.
    let warmup = manager.rewrite(
        "The meeting starts at noon",
        &config,
        &root,
        Arc::new(AtomicBool::new(false)),
    );
    warmup?;
    let cold_ms = started.elapsed().as_secs_f64() * 1000.0;
    let mut rows = vec![];
    let mut confusion =
        std::collections::BTreeMap::<String, std::collections::BTreeMap<String, usize>>::new();
    let mut valid = 0;
    let mut correct = 0;
    let mut intent_errors = 0;
    let mut memory_false_positive = 0;
    let mut memory_missed = 0;
    let mut last_request = None;
    for case in fixture["cases"].as_array().ok_or("cases required")? {
        let id = case["id"].as_str().ok_or("id required")?;
        let request = SemanticAnalysisRequest {
            schema_version: 1,
            origin: transcribe_core::SemanticOrigin::AuthoredEvaluation,
            session_id: None,
            context: LearningContext {
                result_id: id.into(),
                source_key: format!("authored-{id}"),
                episode_id: id.into(),
                learning_enabled: true,
                ..Default::default()
            },
            result_revision: 1,
            before: case["before"].as_str().ok_or("before")?.into(),
            after: case["after"].as_str().ok_or("after")?.into(),
            earlier: case["earlier"].as_str().map(str::to_owned),
            memories: serde_json::from_value::<Vec<SemanticMemory>>(
                case.get("memories").cloned().unwrap_or(json!([])),
            )?,
        };
        let (tx, rx) = mpsc::channel();
        let began = Instant::now();
        manager.submit_semantic_shadow(
            request.clone(),
            config.clone(),
            root.clone(),
            Arc::new(AtomicBool::new(false)),
            move |r| {
                let _ = tx.send(r);
            },
        )?;
        let report = rx.recv_timeout(Duration::from_secs(15))?;
        let expected = case["relation"].as_str().ok_or("relation")?;
        let mut predicted = "failed".to_owned();
        if let Some(p) = &report.proposal {
            valid += 1;
            predicted = serde_json::to_value(p.relation)?
                .as_str()
                .unwrap()
                .to_owned();
            if expected == predicted {
                correct += 1;
            }
            if serde_json::to_value(p.intent)? != case["intent"] {
                intent_errors += 1;
            }
            let relevant: Vec<String> =
                serde_json::from_value(case.get("relevant").cloned().unwrap_or(json!([])))?;
            memory_false_positive += p
                .memories
                .iter()
                .filter(|m| !relevant.contains(&m.id))
                .count();
            memory_missed += relevant
                .iter()
                .filter(|id| !p.memories.iter().any(|m| &m.id == *id))
                .count();
        } else {
            memory_missed += case["relevant"].as_array().map_or(0, Vec::len);
        }
        *confusion
            .entry(expected.into())
            .or_default()
            .entry(predicted)
            .or_default() += 1;
        rows.push(json!({"id":id,"expected":case,"report":report,"submitToCallbackMs":began.elapsed().as_secs_f64()*1000.0}));
        last_request = Some(request);
    }
    let request = last_request.unwrap();
    let mut interference = vec![];
    for pair in 0..20 {
        let mut timings = [0.0; 2];
        let mut outcomes = vec![];
        for shadow in if pair % 2 == 0 {
            [false, true]
        } else {
            [true, false]
        } {
            let mut receiver = None;
            if shadow {
                let (tx, rx) = mpsc::channel();
                manager.submit_semantic_shadow(
                    request.clone(),
                    config.clone(),
                    root.clone(),
                    Arc::new(AtomicBool::new(false)),
                    move |r| {
                        let _ = tx.send(r);
                    },
                )?;
                receiver = Some(rx);
                let wait = Instant::now();
                while manager.semantic_queue_evidence()["running"] != true
                    && wait.elapsed() < Duration::from_secs(1)
                {
                    std::thread::sleep(Duration::from_millis(1));
                }
            }
            let started = Instant::now();
            let output = manager.rewrite(
                "The meeting starts at noon",
                &config,
                &root,
                Arc::new(AtomicBool::new(false)),
            );
            timings[usize::from(shadow)] = started.elapsed().as_secs_f64() * 1000.0;
            outcomes.push(json!({"shadow":shadow,"output":output.as_ref().ok(),"error":output.as_ref().err().map(ToString::to_string)}));
            if let Some(rx) = receiver {
                let report = rx.recv_timeout(Duration::from_secs(15))?;
                assert_eq!(report.status, SemanticStatus::Cancelled);
                assert!(report.proposal.is_none());
            }
        }
        interference.push(json!({"pair":pair,"baselineMs":timings[0],"withShadowMs":timings[1],"delayMs":timings[1]-timings[0],"outcomes":outcomes}));
    }
    // Pre-cancelled work must also abstain without touching the model.
    let flag = Arc::new(AtomicBool::new(true));
    assert!(flag.load(Ordering::Acquire));
    let (tx, rx) = mpsc::channel();
    manager.submit_semantic_shadow(request, config.clone(), root, flag, move |r| {
        let _ = tx.send(r);
    })?;
    assert_eq!(
        rx.recv_timeout(Duration::from_secs(2))?.status,
        SemanticStatus::Cancelled
    );
    let total = rows.len();
    let per_class: Vec<_> = ["repair","continuation","changed_intent","unrelated","unknown"].into_iter().map(|class| {
        let tp = confusion.get(class).and_then(|r| r.get(class)).copied().unwrap_or(0);
        let expected: usize = confusion.get(class).map(|r| r.values().sum()).unwrap_or(0);
        let predicted: usize = confusion.values().map(|r| r.get(class).copied().unwrap_or(0)).sum();
        json!({"class":class,"precision":if predicted>0 {Some(tp as f64/predicted as f64)} else {None},"recall":if expected>0 {Some(tp as f64/expected as f64)} else {None},"expected":expected,"predicted":predicted})
    }).collect();
    let statistics = |key: &str| {
        let mut values: Vec<f64> = interference
            .iter()
            .map(|r| r[key].as_f64().unwrap())
            .collect();
        values.sort_by(f64::total_cmp);
        json!({"meanMs":values.iter().sum::<f64>()/values.len() as f64,"p95Ms":values[(values.len()*95).div_ceil(100)-1]})
    };
    let output = json!({"schemaVersion":1,"origin":fixture["origin"],"backendRequested":config.compute_backend,"coldForegroundMs":cold_ms,"cases":rows,"classification":{"total":total,"valid":valid,"correct":correct,"failed":total-valid,"intentErrorsAmongValid":intent_errors,"memoryFalsePositives":memory_false_positive,"memoryMissed":memory_missed,"confusion":confusion,"perClass":per_class},"interference":interference,"foregroundBaseline":statistics("baselineMs"),"foregroundWithShadow":statistics("withShadowMs"),"foregroundDelay":statistics("delayMs"),"queue":manager.semantic_queue_evidence(),"activation":"not permitted by this authored mechanism screen"});
    std::fs::write(&args[3], serde_json::to_vec_pretty(&output)?)?;
    println!("{}", output["classification"]);
    println!("foreground delay: {}", output["foregroundDelay"]);
    Ok(())
}
