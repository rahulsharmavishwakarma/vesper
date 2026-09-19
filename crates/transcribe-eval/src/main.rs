mod memory;
use chrono::Utc;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
    sync::{atomic::AtomicBool, Arc, Mutex},
    thread,
    time::{Duration, Instant},
};
use transcribe_core::{AppConfig, AsrModel, ComputeBackend, HuggingFaceProfile, Language};
use transcribe_eval::{score, BenchmarkEnvironment, BenchmarkManifest, TranscriptScore};
use transcribe_inference::{InferenceManager, StreamingTranscript, StreamingUpdateSink};

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum RunMode {
    Offline,
    Streaming,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum ReplayPace {
    Accelerated,
    RealTime,
}

#[derive(Debug)]
struct RunOptions {
    manifest: PathBuf,
    corpus_root: PathBuf,
    model_root: PathBuf,
    environment: PathBuf,
    output: PathBuf,
    model: AsrModel,
    asr_profile_path: Option<PathBuf>,
    mode: RunMode,
    pace: ReplayPace,
    cleanup: bool,
    audio_frontend: transcribe_core::AudioFrontendConfig,
    language: Option<Language>,
    chunk_ms: u32,
    profile_path: Option<PathBuf>,
    profile: transcribe_core::PersonalizationProfile,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BenchmarkReport {
    schema_version: u32,
    generated_at: String,
    manifest_sha256: String,
    environment: BenchmarkEnvironment,
    model_versions: serde_json::Value,
    artifact_provenance: serde_json::Value,
    personalization: Option<memory::EvaluationMemory>,
    personalization_sha256: Option<String>,
    runs: Vec<SampleRun>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SampleRun {
    execution: serde_json::Value,
    sample_id: String,
    source_id: String,
    language: String,
    configured_language: &'static str,
    scenario: transcribe_eval::Scenario,
    replay_kind: transcribe_eval::ReplayKind,
    mode: RunMode,
    pace: Option<ReplayPace>,
    audio_seconds: f64,
    chunk_ms: Option<u32>,
    resident_run: bool,
    setup_ms: f64,
    asr_ms: f64,
    cleanup_ms: Option<f64>,
    real_time_factor: f64,
    first_partial_ms: Option<f64>,
    stop_to_final_ms: Option<f64>,
    partial_updates: usize,
    partial_edit_distance: usize,
    raw_hypothesis: Option<String>,
    cleaned_hypothesis: Option<String>,
    personalized_hypothesis: Option<String>,
    personalized_score: Option<TranscriptScore>,
    personalization_ms: Option<f64>,
    personalization_evidence: Option<transcribe_core::MemoryEvidence>,
    raw_score: Option<TranscriptScore>,
    cleaned_score: Option<TranscriptScore>,
    raw_non_speech_score: Option<transcribe_eval::NonSpeechScore>,
    cleaned_non_speech_score: Option<transcribe_eval::NonSpeechScore>,
    personalized_non_speech_score: Option<transcribe_eval::NonSpeechScore>,
    error: Option<String>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    match arguments.first().map(String::as_str) {
        Some("validate") if arguments.len() == 2 => {
            let manifest = BenchmarkManifest::from_path(Path::new(&arguments[1]))?;
            println!(
                "valid benchmark manifest: {} source(s), {} sample(s)",
                manifest.sources.len(),
                manifest.samples.len()
            );
            Ok(())
        }
        Some("recommend") if arguments.len() == 4 => {
            let request: transcribe_eval::RecommendationRequest = read_json(&arguments[1])?;
            let capabilities: Vec<transcribe_eval::ModelCapability> = read_json(&arguments[2])?;
            let evidence: Vec<transcribe_eval::BenchmarkEvidence> = read_json(&arguments[3])?;
            println!(
                "{}",
                serde_json::to_string_pretty(&transcribe_eval::recommend(
                    &request,
                    &capabilities,
                    &evidence
                ))?
            );
            Ok(())
        }
        Some("build-memory") if arguments.len() == 4 => {
            memory::build(&arguments[1], &arguments[2], &arguments[3])
        }
        Some("run") => run(parse_run(&arguments[1..])?),
        _ => Err(usage().into()),
    }
}

fn parse_run(arguments: &[String]) -> Result<RunOptions, String> {
    if arguments.len() < 5 {
        return Err(usage());
    }
    let mut options = RunOptions {
        manifest: PathBuf::from(&arguments[0]),
        corpus_root: PathBuf::from(&arguments[1]),
        model_root: PathBuf::from(&arguments[2]),
        environment: PathBuf::from(&arguments[3]),
        output: PathBuf::from(&arguments[4]),
        model: AsrModel::Nemotron35AsrStreaming06b,
        asr_profile_path: None,
        mode: RunMode::Offline,
        pace: ReplayPace::Accelerated,
        cleanup: false,
        audio_frontend: Default::default(),
        language: None,
        chunk_ms: 20,
        profile_path: None,
        profile: Default::default(),
    };
    let mut model_explicit = false;
    let mut index = 5;
    while index < arguments.len() {
        match arguments[index].as_str() {
            "--model" => {
                if options.asr_profile_path.is_some() {
                    return Err("--model and --asr-profile are mutually exclusive".into());
                }
                index += 1;
                options.model = parse_model(arguments.get(index).ok_or("--model needs a value")?)?;
                model_explicit = true;
            }
            "--asr-profile" => {
                if model_explicit || options.asr_profile_path.is_some() {
                    return Err("--model and --asr-profile are mutually exclusive".into());
                }
                index += 1;
                options.asr_profile_path = Some(PathBuf::from(
                    arguments.get(index).ok_or("--asr-profile needs a path")?,
                ));
            }
            "--profile" => {
                index += 1;
                options.profile_path = Some(PathBuf::from(
                    arguments.get(index).ok_or("--profile needs a path")?,
                ));
            }
            "--streaming" => options.mode = RunMode::Streaming,
            "--realtime" => options.pace = ReplayPace::RealTime,
            "--cleanup" => options.cleanup = true,
            "--vad" => options.audio_frontend.vad_enabled = true,
            "--auto-level" => options.audio_frontend.auto_level_enabled = true,
            "--denoiser" => {
                index += 1;
                options.audio_frontend.noise_reduction =
                    match arguments.get(index).map(String::as_str) {
                        Some("off") => transcribe_core::NoiseReduction::Off,
                        Some("dpdfnet4") => transcribe_core::NoiseReduction::Dpdfnet4,
                        Some("dpdfnet8") => transcribe_core::NoiseReduction::Dpdfnet8,
                        _ => return Err("--denoiser needs off, dpdfnet4 or dpdfnet8".into()),
                    };
            }
            "--language" => {
                index += 1;
                options.language = Some(parse_language(
                    arguments.get(index).ok_or("--language needs a value")?,
                )?);
            }
            "--chunk-ms" => {
                index += 1;
                options.chunk_ms = arguments
                    .get(index)
                    .ok_or("--chunk-ms needs a value")?
                    .parse()
                    .map_err(|_| "--chunk-ms must be a whole number")?;
                if !(10..=1_000).contains(&options.chunk_ms) {
                    return Err("--chunk-ms must be between 10 and 1000".into());
                }
            }
            other => return Err(format!("unknown argument: {other}")),
        }
        index += 1;
    }
    Ok(options)
}

fn read_json<T: serde::de::DeserializeOwned>(path: &str) -> Result<T, Box<dyn std::error::Error>> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

fn run(mut options: RunOptions) -> Result<(), Box<dyn std::error::Error>> {
    options.manifest = fs::canonicalize(&options.manifest)?;
    options.corpus_root = fs::canonicalize(&options.corpus_root)?;
    options.model_root = fs::canonicalize(&options.model_root)?;
    options.environment = fs::canonicalize(&options.environment)?;
    let manifest_bytes = fs::read(&options.manifest)?;
    let manifest_sha256 = format!("{:x}", Sha256::digest(&manifest_bytes));
    let manifest = BenchmarkManifest::from_path(&options.manifest)?;
    let environment = BenchmarkEnvironment::from_path(&options.environment)?;
    let (personalization, personalization_sha256) = if let Some(path) = &options.profile_path {
        let bytes = fs::read(path)?;
        let memory: memory::EvaluationMemory = serde_json::from_slice(&bytes)?;
        memory.validate_holdout(&manifest)?;
        options.profile = memory.profile.clone();
        (Some(memory), Some(format!("{:x}", Sha256::digest(&bytes))))
    } else {
        (None, None)
    };
    let mut config = AppConfig::default();
    config.models.asr = options.model;
    if let Some(path) = &options.asr_profile_path {
        config.models.custom_asr = Some(read_json::<HuggingFaceProfile>(
            path.to_str().ok_or("ASR profile path is not valid UTF-8")?,
        )?);
    }
    config.cleanup_enabled = options.cleanup;
    config.audio_frontend = options.audio_frontend.clone();
    config.compute_backend = parse_backend(&environment.backend)?;
    config.language = options.language.unwrap_or_else(|| {
        language_for_sample(
            &manifest
                .samples
                .first()
                .expect("validated manifests contain samples")
                .language,
        )
    });
    config.validate()?;
    if matches!(options.mode, RunMode::Streaming)
        && !config.speech_adapter().capabilities.native_streaming
    {
        return Err("the selected adapter does not expose native streaming".into());
    }
    let installation = transcribe_models::inspect(&config, &options.model_root);
    if !installation.ready {
        return Err(format!(
            "selected model is not ready in {}: {}",
            options.model_root.display(),
            installation.message
        )
        .into());
    }
    let manager = InferenceManager::new()?;
    let mut runs = Vec::with_capacity(manifest.samples.len());
    for (index, sample) in manifest.samples.iter().enumerate() {
        let audio_path = options.corpus_root.join(&sample.audio_path);
        let configured_language = options
            .language
            .unwrap_or_else(|| language_for_sample(&sample.language));
        config.language = configured_language;
        let resident_run = index > 0 && config.speech_adapter().capabilities.resident_weights;
        let result = run_sample(
            &manager,
            &config,
            &options,
            sample,
            &audio_path,
            resident_run,
        );
        eprintln!("benchmark sample {}/{}: audio={:.1}s asr={:.1}ms first_partial={:?} stop_to_final={:?} failed={}", index + 1, manifest.samples.len(), result.audio_seconds, result.asr_ms, result.first_partial_ms, result.stop_to_final_ms, result.error.is_some());
        runs.push(result);
    }
    let report = BenchmarkReport {
        schema_version: 1,
        generated_at: Utc::now().to_rfc3339(),
        manifest_sha256,
        environment,
        model_versions: transcribe_inference::model_versions(&config),
        artifact_provenance: transcribe_models::provenance(&config),
        personalization,
        personalization_sha256,
        runs,
    };
    if let Some(parent) = options.output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&options.output, serde_json::to_vec_pretty(&report)?)?;
    println!("wrote {}", options.output.display());
    Ok(())
}

fn run_sample(
    manager: &InferenceManager,
    config: &AppConfig,
    options: &RunOptions,
    sample: &transcribe_eval::BenchmarkSample,
    audio_path: &Path,
    resident_run: bool,
) -> SampleRun {
    let configured_language = language_name(config.language);
    let operation_id = match options.mode {
        RunMode::Offline => format!("benchmark-{}", sample.id),
        RunMode::Streaming => format!("benchmark-stream-{}", sample.id),
    };
    let failed = |error: String| SampleRun {
        execution: manager.execution_evidence(&operation_id),
        sample_id: sample.id.clone(),
        source_id: sample.source_id.clone(),
        language: sample.language.clone(),
        configured_language,
        scenario: sample.scenario,
        replay_kind: sample.replay_kind,
        mode: options.mode,
        pace: matches!(options.mode, RunMode::Streaming).then_some(options.pace),
        audio_seconds: sample.duration_seconds,
        chunk_ms: matches!(options.mode, RunMode::Streaming).then_some(options.chunk_ms),
        resident_run,
        setup_ms: 0.0,
        asr_ms: 0.0,
        cleanup_ms: None,
        real_time_factor: 0.0,
        first_partial_ms: None,
        stop_to_final_ms: None,
        partial_updates: 0,
        partial_edit_distance: 0,
        raw_hypothesis: None,
        cleaned_hypothesis: None,
        personalized_hypothesis: None,
        personalized_score: None,
        personalization_ms: None,
        personalization_evidence: None,
        raw_score: None,
        cleaned_score: None,
        raw_non_speech_score: None,
        cleaned_non_speech_score: None,
        personalized_non_speech_score: None,
        error: Some(error),
    };
    if let Err(error) = config.validate() {
        return failed(error.to_string());
    }
    if let Err(error) = verify_sha256(audio_path, &sample.sha256) {
        return failed(error);
    }
    let setup = Instant::now();
    let audio = match transcribe_audio::decode_file(audio_path, 0) {
        Ok(audio) => audio,
        Err(error) => return failed(error.to_string()),
    };
    let setup_ms = setup.elapsed().as_secs_f64() * 1_000.0;
    if (audio.duration_seconds() - sample.duration_seconds).abs() > 0.25 {
        return failed(format!(
            "decoded duration {:.3}s differs from manifest {:.3}s",
            audio.duration_seconds(),
            sample.duration_seconds
        ));
    }
    let asr_started = Instant::now();
    let streaming = match options.mode {
        RunMode::Offline => StreamingOutcome::offline(manager.transcribe_personalized(
            &audio,
            config,
            &options.model_root,
            &format!("benchmark-{}", sample.id),
            Arc::new(AtomicBool::new(false)),
            Arc::new(|_| {}),
            &options.profile,
        )),
        RunMode::Streaming => run_streaming(manager, config, options, sample, &audio),
    };
    let asr_ms = asr_started.elapsed().as_secs_f64() * 1_000.0;
    let raw = match streaming.result {
        Ok(raw) => raw,
        Err(error) => {
            return SampleRun {
                setup_ms,
                asr_ms,
                real_time_factor: asr_ms / 1_000.0 / audio.duration_seconds(),
                first_partial_ms: streaming.first_partial_ms,
                stop_to_final_ms: streaming.stop_to_final_ms,
                partial_updates: streaming.partial_updates,
                partial_edit_distance: streaming.partial_edit_distance,
                ..failed(error)
            }
        }
    };
    let cleanup_started = Instant::now();
    let (cleaned, cleanup_error) = if config.cleanup_enabled {
        match manager.rewrite_personalized(
            &raw,
            config,
            &options.model_root,
            &operation_id,
            Arc::new(AtomicBool::new(false)),
            &options.profile,
        ) {
            Ok(value) => (Some(value), None),
            Err(error) => (None, Some(error.to_string())),
        }
    } else {
        (None, None)
    };
    let cleanup_ms = config
        .cleanup_enabled
        .then(|| cleanup_started.elapsed().as_secs_f64() * 1_000.0);
    let memory_started = Instant::now();
    let personalized = options
        .profile
        .personalize(cleaned.as_deref().unwrap_or(&raw), config.language);
    let personalization_ms = memory_started.elapsed().as_secs_f64() * 1000.0;
    SampleRun {
        personalized_score: (!sample.reference.trim().is_empty())
            .then(|| score(&sample.reference, &personalized.text)),
        personalized_non_speech_score: sample
            .reference
            .trim()
            .is_empty()
            .then(|| {
                transcribe_eval::score_non_speech(&personalized.text, audio.duration_seconds())
            })
            .flatten(),
        personalized_hypothesis: Some(personalized.text),
        personalization_ms: Some(personalization_ms),
        personalization_evidence: Some(personalized.evidence),
        execution: manager.execution_evidence(&operation_id),
        sample_id: sample.id.clone(),
        source_id: sample.source_id.clone(),
        language: sample.language.clone(),
        configured_language,
        scenario: sample.scenario,
        replay_kind: sample.replay_kind,
        mode: options.mode,
        pace: matches!(options.mode, RunMode::Streaming).then_some(options.pace),
        audio_seconds: audio.duration_seconds(),
        chunk_ms: matches!(options.mode, RunMode::Streaming).then_some(options.chunk_ms),
        resident_run,
        setup_ms,
        asr_ms,
        cleanup_ms,
        real_time_factor: asr_ms / 1_000.0 / audio.duration_seconds(),
        first_partial_ms: streaming.first_partial_ms,
        stop_to_final_ms: streaming.stop_to_final_ms,
        partial_updates: streaming.partial_updates,
        partial_edit_distance: streaming.partial_edit_distance,
        raw_score: (!sample.reference.trim().is_empty()).then(|| score(&sample.reference, &raw)),
        cleaned_score: cleaned
            .as_deref()
            .filter(|_| !sample.reference.trim().is_empty())
            .map(|value| score(&sample.reference, value)),
        raw_non_speech_score: sample
            .reference
            .trim()
            .is_empty()
            .then(|| transcribe_eval::score_non_speech(&raw, audio.duration_seconds()))
            .flatten(),
        cleaned_non_speech_score: cleaned
            .as_deref()
            .filter(|_| sample.reference.trim().is_empty())
            .and_then(|value| transcribe_eval::score_non_speech(value, audio.duration_seconds())),
        raw_hypothesis: Some(raw),
        cleaned_hypothesis: cleaned,
        error: cleanup_error,
    }
}

struct StreamingOutcome {
    result: Result<String, String>,
    first_partial_ms: Option<f64>,
    stop_to_final_ms: Option<f64>,
    partial_updates: usize,
    partial_edit_distance: usize,
}

impl StreamingOutcome {
    fn offline(result: transcribe_inference::Result<String>) -> Self {
        Self {
            result: result.map_err(|error| error.to_string()),
            first_partial_ms: None,
            stop_to_final_ms: None,
            partial_updates: 0,
            partial_edit_distance: 0,
        }
    }
}

fn run_streaming(
    manager: &InferenceManager,
    config: &AppConfig,
    options: &RunOptions,
    sample: &transcribe_eval::BenchmarkSample,
    audio: &transcribe_audio::MonoPcm,
) -> StreamingOutcome {
    let updates = Arc::new(Mutex::new(Vec::<(f64, StreamingTranscript)>::new()));
    let sink_updates = Arc::clone(&updates);
    let stream_started = Instant::now();
    let sink: StreamingUpdateSink = Arc::new(move |update| {
        if let Ok(mut updates) = sink_updates.lock() {
            updates.push((stream_started.elapsed().as_secs_f64() * 1_000.0, update));
        }
    });
    let session = match manager.start_stream_personalized(
        config,
        &options.model_root,
        &format!("benchmark-stream-{}", sample.id),
        Arc::new(AtomicBool::new(false)),
        sink,
        &options.profile,
    ) {
        Ok(session) => session,
        Err(error) => {
            return StreamingOutcome {
                result: Err(error.to_string()),
                first_partial_ms: None,
                stop_to_final_ms: None,
                partial_updates: 0,
                partial_edit_distance: 0,
            }
        }
    };
    let samples_per_chunk =
        ((u64::from(audio.sample_rate()) * u64::from(options.chunk_ms)) / 1_000).max(1) as usize;
    let feed_started = Instant::now();
    for (index, chunk) in audio.samples().chunks(samples_per_chunk).enumerate() {
        if let Err(error) = session.push(chunk.to_vec(), audio.sample_rate()) {
            return StreamingOutcome {
                result: Err(error.to_string()),
                first_partial_ms: first_partial(&updates),
                stop_to_final_ms: None,
                partial_updates: update_count(&updates),
                partial_edit_distance: partial_edits(&updates),
            };
        }
        if matches!(options.pace, ReplayPace::RealTime) {
            let delivered = ((index + 1) * samples_per_chunk).min(audio.samples().len());
            let deadline =
                Duration::from_secs_f64(delivered as f64 / f64::from(audio.sample_rate()));
            if let Some(remaining) = deadline.checked_sub(feed_started.elapsed()) {
                thread::sleep(remaining);
            }
        }
    }
    let stop = Instant::now();
    let result = session.finish().map_err(|error| error.to_string());
    StreamingOutcome {
        result,
        first_partial_ms: first_partial(&updates),
        stop_to_final_ms: Some(stop.elapsed().as_secs_f64() * 1_000.0),
        partial_updates: update_count(&updates),
        partial_edit_distance: partial_edits(&updates),
    }
}

fn first_partial(updates: &Arc<Mutex<Vec<(f64, StreamingTranscript)>>>) -> Option<f64> {
    updates.lock().ok()?.iter().find_map(|(elapsed, update)| {
        (!update.is_final && !update.provisional_text.trim().is_empty()).then_some(*elapsed)
    })
}

fn update_count(updates: &Arc<Mutex<Vec<(f64, StreamingTranscript)>>>) -> usize {
    updates.lock().map_or(0, |updates| updates.len())
}

fn partial_edits(updates: &Arc<Mutex<Vec<(f64, StreamingTranscript)>>>) -> usize {
    let Ok(updates) = updates.lock() else {
        return 0;
    };
    let partials = updates
        .iter()
        .filter(|(_, update)| !update.is_final && !update.provisional_text.trim().is_empty())
        .map(|(_, update)| update.provisional_text.as_str())
        .collect::<Vec<_>>();
    partials
        .windows(2)
        .map(|pair| transcribe_eval::word_edit_distance(pair[0], pair[1]))
        .sum()
}

fn verify_sha256(path: &Path, expected: &str) -> Result<(), String> {
    let mut file = File::open(path)
        .map_err(|error| format!("cannot read benchmark audio {}: {error}", path.display()))?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| format!("cannot read benchmark audio {}: {error}", path.display()))?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    let actual = format!("{:x}", digest.finalize());
    if actual == expected {
        Ok(())
    } else {
        Err(format!(
            "benchmark audio hash mismatch for {}: expected {expected}, got {actual}",
            path.display()
        ))
    }
}

fn language_for_sample(value: &str) -> Language {
    if value.contains('+') {
        return Language::Auto;
    }
    match value.split(['-', '_']).next().unwrap_or(value) {
        "en" => Language::English,
        "fr" => Language::French,
        "hi" => Language::Hindi,
        _ => Language::Auto,
    }
}

fn parse_language(value: &str) -> Result<Language, String> {
    match value {
        "auto" => Ok(Language::Auto),
        "en" => Ok(Language::English),
        "fr" => Ok(Language::French),
        "hi" => Ok(Language::Hindi),
        _ => Err(format!("unsupported language: {value}")),
    }
}

fn language_name(value: Language) -> &'static str {
    match value {
        Language::Auto => "auto",
        Language::English => "en",
        Language::French => "fr",
        Language::Hindi => "hi",
    }
}

fn parse_model(value: &str) -> Result<AsrModel, String> {
    AsrModel::ALL
        .into_iter()
        .find(|model| model.id() == value)
        .ok_or_else(|| format!("unsupported built-in ASR model: {value}"))
}

fn parse_backend(value: &str) -> Result<ComputeBackend, String> {
    match value {
        "auto" => Ok(ComputeBackend::Auto),
        "cpu" => Ok(ComputeBackend::Cpu),
        "metal" => Ok(ComputeBackend::Metal),
        "cuda" => Ok(ComputeBackend::Cuda),
        "vulkan" => Ok(ComputeBackend::Vulkan),
        "rocm" => Ok(ComputeBackend::Rocm),
        _ => Err(format!("unsupported backend: {value}")),
    }
}

fn usage() -> String {
    "usage:\n  transcribe-eval validate MANIFEST\n  transcribe-eval run MANIFEST CORPUS_ROOT MODEL_ROOT ENVIRONMENT OUTPUT [--model MODEL_ID | --asr-profile PROFILE_JSON] [--streaming] [--realtime] [--cleanup] [--vad] [--auto-level] [--denoiser off|dpdfnet4|dpdfnet8] [--language auto|en|fr|hi] [--chunk-ms 20] [--profile EVALUATION_MEMORY]\n  transcribe-eval build-memory CALIBRATION_MANIFEST BASELINE_REPORT OUTPUT\n  transcribe-eval recommend REQUEST_JSON CAPABILITIES_JSON EVIDENCE_JSON".into()
}

#[cfg(test)]
mod tests {
    #[test]
    fn automatic_level_flag_selects_shared_frontend_without_annotations() {
        let args = [
            "manifest",
            "corpus",
            "models",
            "environment",
            "output",
            "--auto-level",
        ]
        .map(str::to_owned);
        let options = super::parse_run(&args).unwrap();
        assert!(options.audio_frontend.auto_level_enabled);
        assert!(!options.audio_frontend.vad_enabled);
        assert_eq!(options.audio_frontend.model_ids(), ["silero-v6.2"]);
    }
    use super::*;

    #[test]
    fn benchmark_language_profiles_map_to_application_prompts() {
        assert_eq!(language_for_sample("hi-IN"), Language::Hindi);
        assert_eq!(language_for_sample("en-IN"), Language::English);
        assert_eq!(language_for_sample("en-US"), Language::English);
        assert_eq!(language_for_sample("hi+en"), Language::Auto);
    }

    #[test]
    fn custom_asr_profile_is_explicit_and_cannot_shadow_a_built_in_model() {
        let base = ["manifest", "corpus", "models", "environment", "output"].map(str::to_owned);
        let mut custom = base.to_vec();
        custom.extend(["--asr-profile".into(), "profile.json".into()]);
        let parsed = parse_run(&custom).unwrap();
        assert_eq!(parsed.asr_profile_path, Some(PathBuf::from("profile.json")));

        let mut ambiguous = base.to_vec();
        ambiguous.extend([
            "--model".into(),
            AsrModel::ParakeetTdt06bV3.id().into(),
            "--asr-profile".into(),
            "profile.json".into(),
        ]);
        assert_eq!(
            parse_run(&ambiguous).unwrap_err(),
            "--model and --asr-profile are mutually exclusive"
        );
    }
}
