//! `term-decode` and `term-score`: run the fixed term evaluator through the
//! application's inference manager, then score saved outputs without models.
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs::{self, OpenOptions},
    io::{BufRead, Write},
    path::PathBuf,
    sync::{atomic::AtomicBool, Arc},
    time::Instant,
};
use vesper_core::{AppConfig, AsrModel, DecodeContext, Language, TermTier, VocabularyEntry};
use vesper_eval::term_eval::{
    condition_terms, score_rows, working_set, Condition, DecodeRow, TermManifest,
};
use vesper_eval::BenchmarkEnvironment;
use vesper_inference::InferenceManager;

/// Evaluator-only ceiling for the `large` condition; native runtimes still
/// enforce their own hard limits and such failures are kept.
const LARGE_HINT_LIMITS: (usize, usize) = (512, 65_536);

struct DecodeOptions {
    manifest: PathBuf,
    corpus_root: PathBuf,
    model_root: PathBuf,
    environment: PathBuf,
    output: PathBuf,
    model: AsrModel,
    asr_engine: vesper_core::AsrEngine,
    conditions: Vec<Condition>,
    categories: Option<Vec<String>>,
    noise: Option<Vec<String>>,
    split: Option<String>,
    frontend: vesper_core::AudioFrontendConfig,
    cleanup: bool,
    limit: Option<usize>,
}

fn list(value: Option<&String>, flag: &str) -> Result<Vec<String>, String> {
    Ok(value
        .ok_or(format!("{flag} needs a value"))?
        .split(',')
        .map(str::to_owned)
        .collect())
}

fn parse(arguments: &[String]) -> Result<DecodeOptions, String> {
    if arguments.len() < 5 {
        return Err(
            "term-decode needs TERM_MANIFEST CORPUS_ROOT MODEL_ROOT ENVIRONMENT OUTPUT_JSONL"
                .into(),
        );
    }
    let mut options = DecodeOptions {
        manifest: arguments[0].clone().into(),
        corpus_root: arguments[1].clone().into(),
        model_root: arguments[2].clone().into(),
        environment: arguments[3].clone().into(),
        output: arguments[4].clone().into(),
        model: AsrModel::CohereTranscribe,
        asr_engine: vesper_core::AsrEngine::Automatic,
        conditions: Condition::ALL.to_vec(),
        categories: None,
        noise: None,
        split: Some("test".into()),
        frontend: Default::default(),
        cleanup: false,
        limit: None,
    };
    let mut index = 5;
    while index < arguments.len() {
        let value = arguments.get(index + 1);
        match arguments[index].as_str() {
            "--model" => {
                let id = value.ok_or("--model needs a value")?;
                options.model = AsrModel::ALL
                    .into_iter()
                    .find(|m| m.id() == id)
                    .ok_or(format!("unsupported model {id}"))?;
            }
            "--asr-engine" => {
                options.asr_engine = serde_json::from_value(serde_json::Value::String(
                    value.ok_or("--asr-engine needs a value")?.clone(),
                ))
                .map_err(|_| "unknown --asr-engine")?;
            }
            "--conditions" => {
                options.conditions = list(value, "--conditions")?
                    .iter()
                    .map(|c| Condition::parse(c))
                    .collect::<Result<_, _>>()?;
            }
            "--categories" => options.categories = Some(list(value, "--categories")?),
            "--noise" => options.noise = Some(list(value, "--noise")?),
            "--split" => {
                let split = value.ok_or("--split needs a value")?;
                options.split = (split != "all").then(|| split.clone());
            }
            "--denoiser" => {
                options.frontend.noise_reduction = match value.map(String::as_str) {
                    Some("off") => vesper_core::NoiseReduction::Off,
                    Some("dpdfnet4") => vesper_core::NoiseReduction::Dpdfnet4,
                    Some("dpdfnet8") => vesper_core::NoiseReduction::Dpdfnet8,
                    _ => return Err("--denoiser needs off, dpdfnet4 or dpdfnet8".into()),
                }
            }
            "--limit" => {
                options.limit = Some(
                    value
                        .ok_or("--limit needs a value")?
                        .parse()
                        .map_err(|_| "--limit must be a number")?,
                )
            }
            "--vad" => {
                options.frontend.vad_enabled = true;
                index += 1;
                continue;
            }
            "--cleanup" => {
                options.cleanup = true;
                index += 1;
                continue;
            }
            other => return Err(format!("unknown term-decode argument: {other}")),
        }
        index += 2;
    }
    Ok(options)
}

fn frontend_label(frontend: &vesper_core::AudioFrontendConfig) -> String {
    let denoiser = match frontend.noise_reduction {
        vesper_core::NoiseReduction::Off => "raw",
        vesper_core::NoiseReduction::Dpdfnet4 => "dpdfnet4",
        vesper_core::NoiseReduction::Dpdfnet8 => "dpdfnet8",
    };
    if frontend.vad_enabled {
        format!("vad+{denoiser}")
    } else {
        denoiser.into()
    }
}

fn sha256_file(path: &std::path::Path) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

pub fn decode(arguments: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let options = parse(arguments)?;
    let manifest_bytes = fs::read(&options.manifest)?;
    let manifest = TermManifest::from_slice(&manifest_bytes)?;
    let environment = BenchmarkEnvironment::from_path(&options.environment)?;
    let mut config = AppConfig::default();
    config.models.asr = options.model;
    config.models.asr_engine = options.asr_engine;
    config.language = Language::English;
    config.cleanup_enabled = options.cleanup;
    config.audio_frontend = options.frontend.clone();
    config.compute_backend =
        serde_json::from_value(serde_json::Value::String(environment.backend.clone()))?;
    config.validate()?;
    let installation = vesper_models::inspect(&config, &options.model_root);
    if !installation.ready {
        return Err(format!("model not ready: {}", installation.message).into());
    }
    let selection = vesper_models::resolve_speech(&config, &options.model_root)
        .map_err(|e| format!("cannot resolve speech model: {e}"))?;
    let hook = selection.config.speech_adapter().capabilities.speech_hook;
    let hooked = hook.kind != vesper_core::SpeechHookKind::None;
    // Hints cannot reach a hook-less model: one decode stands for every condition.
    // The writer arm still needs each condition's vocabulary, so it decodes all.
    let conditions = if hooked || options.cleanup {
        options.conditions.clone()
    } else {
        vec![Condition::None]
    };
    let model_label = format!(
        "{}@{}",
        config.models.asr.id(),
        serde_json::to_value(selection.config.effective_asr_engine())?
            .as_str()
            .unwrap_or("engine")
    );
    let mut frontend = frontend_label(&options.frontend);
    if options.cleanup {
        // A separate lane: writer rows never replace the plain decode rows.
        frontend.push_str("+writer");
    }
    let mut done = HashSet::new();
    if let Ok(file) = fs::File::open(&options.output) {
        for line in std::io::BufReader::new(file).lines() {
            if let Ok(row) = serde_json::from_str::<DecodeRow>(&line?) {
                done.insert((row.sample_id, row.condition, row.frontend, row.model));
            }
        }
    }
    if let Some(parent) = options.output.parent() {
        fs::create_dir_all(parent)?;
    }
    let meta_path = options.output.with_extension("meta.json");
    if !meta_path.exists() {
        fs::write(
            &meta_path,
            serde_json::to_vec_pretty(&serde_json::json!({
                "schemaVersion": 1,
                "startedAt": chrono::Utc::now().to_rfc3339(),
                "manifestSha256": format!("{:x}", Sha256::digest(&manifest_bytes)),
                "environment": environment,
                "model": model_label,
                "speechHook": hook,
                "frontend": frontend,
                "conditions": conditions,
                "cleanup": options.cleanup,
                "modelVersions": vesper_inference::model_versions(&selection.config),
                "artifactProvenance": vesper_models::provenance(&selection.config),
                "arguments": arguments,
            }))?,
        )?;
    }
    let mut output = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&options.output)?;
    let manager = InferenceManager::new()?;
    let samples = manifest
        .samples
        .iter()
        .filter(|s| s.audio_path.is_some())
        .filter(|s| options.split.as_ref().is_none_or(|split| &s.split == split))
        .filter(|s| {
            options
                .categories
                .as_ref()
                .is_none_or(|c| c.contains(&s.category))
        })
        .filter(|s| options.noise.as_ref().is_none_or(|n| n.contains(&s.noise)))
        .take(options.limit.unwrap_or(usize::MAX))
        .collect::<Vec<_>>();
    for (index, sample) in samples.iter().enumerate() {
        let path = options
            .corpus_root
            .join(sample.audio_path.as_deref().unwrap_or_default());
        let audio = sha256_file(&path).and_then(|actual| {
            if Some(actual.as_str()) == sample.sha256.as_deref() {
                vesper_audio::decode_file(&path, 0).map_err(|e| e.to_string())
            } else {
                Err(format!("audio hash mismatch for {}", sample.id))
            }
        });
        for &condition in &conditions {
            let key = (
                sample.id.clone(),
                condition,
                frontend.clone(),
                model_label.clone(),
            );
            if done.contains(&key) {
                continue;
            }
            let terms = condition_terms(&manifest, sample, condition);
            let mut profile = vesper_core::PersonalizationProfile {
                decode_context: Some(DecodeContext {
                    working_set: working_set(&terms, TermTier::Asserted),
                    evaluate_declared_hooks: true,
                    evaluation_limits: (condition == Condition::Large).then_some(LARGE_HINT_LIMITS),
                }),
                ..Default::default()
            };
            if options.cleanup {
                // The writer's reference spellings come from profile vocabulary.
                profile.vocabulary = terms
                    .iter()
                    .take(128)
                    .enumerate()
                    .map(|(i, t)| VocabularyEntry {
                        id: format!("eval-{i}"),
                        phrase: t.spelling.clone(),
                        language: Language::English,
                        enabled: true,
                        ..Default::default()
                    })
                    .collect();
            }
            let hints = vesper_core::recognizer_hints(&hook, &profile, Language::English);
            let operation = format!("term-{}-{}", sample.id, condition.name());
            let started = Instant::now();
            let result = match &audio {
                Ok(audio) => manager
                    .transcribe_personalized(
                        audio,
                        &selection.config,
                        &options.model_root,
                        &operation,
                        Arc::new(AtomicBool::new(false)),
                        Arc::new(|_| {}),
                        &profile,
                    )
                    .map_err(|e| e.to_string()),
                Err(error) => Err(error.clone()),
            };
            let asr_ms = started.elapsed().as_secs_f64() * 1000.0;
            let frontend_evidence = manager.execution_evidence(&operation)["audioFrontend"].clone();
            // A requested frontend that fell back to original audio is not a
            // frontend measurement: record it as a failed row, never as a result.
            let result = match result {
                Ok(_)
                    if options.frontend.enabled()
                        && (frontend_evidence["originalAudioFallback"] == true
                            || frontend_evidence.is_null()) =>
                {
                    Err(format!(
                        "audio frontend did not run: {}",
                        frontend_evidence["error"]
                            .as_str()
                            .unwrap_or("no frontend evidence")
                    ))
                }
                other => other,
            };
            let (mut cleaned, mut cleanup_ms, mut cleanup_error) = (None, None, None);
            if let (true, Ok(raw)) = (options.cleanup, &result) {
                let started = Instant::now();
                match manager.rewrite_personalized(
                    raw,
                    &selection.config,
                    &options.model_root,
                    &operation,
                    Arc::new(AtomicBool::new(false)),
                    &profile,
                ) {
                    Ok(text) => cleaned = Some(text),
                    Err(error) => cleanup_error = Some(error.to_string()),
                }
                cleanup_ms = Some(started.elapsed().as_secs_f64() * 1000.0);
            }
            let row = DecodeRow {
                model: model_label.clone(),
                hook_kind: serde_json::to_value(hook.kind)?
                    .as_str()
                    .unwrap_or_default()
                    .into(),
                hook_status: serde_json::to_value(hook.status)?
                    .as_str()
                    .unwrap_or_default()
                    .into(),
                frontend: frontend.clone(),
                sample_id: sample.id.clone(),
                condition,
                hints_sent: hints.phrases.len(),
                hint_hash: hints.hint_hash,
                hint_source: serde_json::to_value(hints.source)?
                    .as_str()
                    .unwrap_or_default()
                    .into(),
                raw: result.as_ref().ok().cloned(),
                cleaned,
                asr_ms,
                cleanup_ms,
                error: result.err(),
                frontend_evidence: (!frontend_evidence.is_null()).then_some(frontend_evidence),
                cleanup_error,
            };
            writeln!(output, "{}", serde_json::to_string(&row)?)?;
            eprintln!(
                "term sample {}/{} {} {}: {:.0} ms failed={}",
                index + 1,
                samples.len(),
                sample.id,
                condition.name(),
                asr_ms,
                row.error.is_some()
            );
        }
    }
    Ok(())
}

pub fn score(arguments: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if arguments.len() < 3 {
        return Err("term-score needs TERM_MANIFEST OUTPUT_JSON DECODE_JSONL...".into());
    }
    let manifest_bytes = fs::read(&arguments[0])?;
    let manifest = TermManifest::from_slice(&manifest_bytes)?;
    let mut rows = Vec::new();
    let mut inputs = Vec::new();
    for path in &arguments[2..] {
        let bytes = fs::read(path)?;
        inputs.push(serde_json::json!({
            "file": std::path::Path::new(path).file_name().and_then(|n| n.to_str()),
            "sha256": format!("{:x}", Sha256::digest(&bytes)),
        }));
        for line in bytes.split(|b| *b == b'\n').filter(|l| !l.is_empty()) {
            rows.push(serde_json::from_slice::<DecodeRow>(line)?);
        }
    }
    let started = Instant::now();
    let groups = score_rows(&manifest, &rows, &Condition::ALL, true);
    let report = serde_json::json!({
        "schemaVersion": 1,
        "generatedAt": chrono::Utc::now().to_rfc3339(),
        "manifestSha256": format!("{:x}", Sha256::digest(&manifest_bytes)),
        "inputs": inputs,
        "decodeRows": rows.len(),
        "scoringMs": started.elapsed().as_secs_f64() * 1000.0,
        "groups": groups.iter().map(|(key, score)| serde_json::json!({"key": key, "score": score})).collect::<Vec<_>>(),
    });
    fs::write(&arguments[1], serde_json::to_vec_pretty(&report)?)?;
    println!("wrote {} groups to {}", groups.len(), arguments[1]);
    Ok(())
}
