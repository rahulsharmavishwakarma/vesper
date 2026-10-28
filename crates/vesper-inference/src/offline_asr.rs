//! Completed-recording generative ASR; each segment has fresh state.
use crate::{
    check_cancelled, native, path_string, require_file, worker_threads, InferenceError, Result,
    TranscriptionProgress,
};
use serde::{Deserialize, Serialize};
use sherpa_onnx::{
    OfflineCohereTranscribeModelConfig, OfflineModelConfig, OfflineMoonshineModelConfig,
    OfflineQwen3ASRModelConfig, OfflineRecognizer, OfflineRecognizerConfig, OfflineStream,
};
use std::{path::Path, sync::atomic::AtomicBool};
use vesper_audio::{MonoPcm, ASR_SAMPLE_RATE};
use vesper_core::{AppConfig, AsrModel, Language};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SegmentObservation {
    samples: usize,
    accept_us: u64,
    decode_us: u64,
    completion: Option<CompletionReport>,
    whisper_scores: Option<crate::whisper::Scores>,
}

const REPORT_OPTION: &str = "vesper.decode_report.v1";
pub(crate) const QWEN_REVISION: &str = "68818b2313fe77bd06f6a7c5068ff3ef59d02b8a";
pub(crate) const COHERE_REVISION: &str = "156a470cf08eefe706a0004f3c52d9ee567ca7a0";
pub(crate) const MOONSHINE_REVISION: &str = "577269bb93e5cb0f92c01bd4444d93f9a9203805";
pub(crate) const COHERE_USE_PUNCT: bool = true;
pub(crate) const COHERE_USE_ITN: bool = false;
// Use the existing context allowance without an independent lower output ceiling.
pub(crate) const QWEN_CONTEXT_TOKENS: i32 = 512;
#[cfg(test)]
use crate::segmented_audio::decode_segments;
use crate::segmented_audio::decode_segments_with_limit;
pub(crate) use crate::segmented_audio::MAX_SEGMENT_SECONDS;
#[cfg(test)]
use crate::segmented_audio::{segment_end, ENERGY_WINDOW, MAX_SEGMENT_SAMPLES};

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CompletionReport {
    schema_version: u32,
    stop_reason: String,
    input_context_truncated: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum DecodePurpose {
    Completed,
    Preview,
}

#[cfg(test)]
fn completed_text(report: &str, text: &str) -> Result<String> {
    accepted_text(report, text, DecodePurpose::Completed)
}

fn accepted_text(report: &str, text: &str, purpose: DecodePurpose) -> Result<String> {
    let report: CompletionReport = serde_json::from_str(report).map_err(|_| {
        native(
        "speech completion check",
        "missing or invalid completion report; rebuild the app with its pinned patched ASR runtime",
    )
    })?;
    if report.schema_version != 1 {
        return Err(native(
            "speech completion check",
            "unsupported completion report version",
        ));
    }
    if report.input_context_truncated {
        return Err(native(
            "speech decode",
            "the model discarded audio context; incomplete transcription was discarded",
        ));
    }
    match report.stop_reason.as_str() {
        "eos" => Ok(text.trim().to_owned()),
        "silence_detected" | "no_input" if text.trim().is_empty() => Ok(String::new()),
        // An unfinished speech window can legitimately exhaust its duration
        // budget. Such text is a replaceable draft, never a completed result.
        "duration_token_limit" if purpose == DecodePurpose::Preview => Ok(text.trim().to_owned()),
        // A window cut mid-utterance can stop early for these reasons. Skip that
        // draft without treating the resident decoder as unhealthy.
        "max_new_tokens" | "context_limit" | "model_token_limit" | "repetition"
        | "silence_detected" | "no_input"
            if purpose == DecodePurpose::Preview =>
        {
            Err(InferenceError::IncompletePreview(report.stop_reason))
        }
        "max_new_tokens"
        | "context_limit"
        | "duration_token_limit"
        | "model_token_limit"
        | "repetition" => Err(native(
            "speech decode",
            format!(
                "decoding stopped before completion ({}); incomplete transcription was discarded",
                report.stop_reason
            ),
        )),
        _ => Err(native(
            "speech decode",
            "the model did not complete successfully; partial text was discarded",
        )),
    }
}

/// Vendor-declared language prompts from models/languages.json. Qwen takes an
/// English language name, Cohere takes an ISO code (standard mappings of the
/// vendor's language names). Unmapped modes yield no prompt; invalid
/// combinations are rejected by AppConfig::validate.
fn language(config: &AppConfig) -> Option<&'static str> {
    let (runtime, language) = match (config.models.asr, config.language) {
        (_, Language::Auto) => return None,
        (AsrModel::Qwen3Asr06b, language) => ("qwen", language),
        (AsrModel::CohereTranscribe, language) => ("sherpa", language),
        _ => return None,
    };
    vesper_core::runtime_language_code(
        runtime,
        config.models.asr.id(),
        language.code().expect("explicit language has an ISO code"),
    )
}

/// Moonshine trains on 4-30 s segments; its reference runtime caps VAD
/// segments at 15 s (`vad_max_segment_duration`). Above about 9.2 s the
/// pinned decoder needed the backported raw-length attention mask (#3976).
pub(crate) const MOONSHINE_MAX_SEGMENT_SECONDS: usize = 15;

pub(crate) fn recognizer_config(
    config: &AppConfig,
    root: &Path,
) -> Result<OfflineRecognizerConfig> {
    if config.models.asr.is_whisper() {
        return crate::whisper::recognizer_config(config, root);
    }
    let mut model = OfflineModelConfig {
        provider: Some("cpu".into()),
        num_threads: worker_threads(),
        ..Default::default()
    };
    let (folder, files): (_, &[&str]) = match config.models.asr {
        AsrModel::Qwen3Asr06b => (
            "qwen3-asr-0.6b",
            &[
                "conv_frontend.onnx",
                "encoder.int8.onnx",
                "decoder.int8.onnx",
                "tokenizer/vocab.json",
                "tokenizer/merges.txt",
                "tokenizer/tokenizer_config.json",
            ],
        ),
        AsrModel::CohereTranscribe => (
            "cohere-transcribe",
            &[
                "encoder.int8.onnx",
                "encoder.int8.onnx.data",
                "decoder.int8.onnx",
                "tokens.txt",
            ],
        ),
        AsrModel::MoonshineBase => (
            "moonshine-base-en",
            &[
                "encoder_model.ort",
                "decoder_model_merged.ort",
                "tokens.txt",
            ],
        ),
        _ => return Err(InferenceError::UnsupportedAsr(config.models.asr.id())),
    };
    for file in files {
        require_file(&root.join(folder).join(file))?;
    }
    let path = |file: &str| path_string(&root.join(folder).join(file));
    match config.models.asr {
        AsrModel::Qwen3Asr06b => {
            model.qwen3_asr = OfflineQwen3ASRModelConfig {
                conv_frontend: Some(path("conv_frontend.onnx")?),
                encoder: Some(path("encoder.int8.onnx")?),
                decoder: Some(path("decoder.int8.onnx")?),
                tokenizer: Some(path("tokenizer")?),
                // Exact export context is bounded; native completion reporting rejects overflow.
                max_total_len: QWEN_CONTEXT_TOKENS,
                max_new_tokens: QWEN_CONTEXT_TOKENS,
                ..Default::default()
            }
        }
        AsrModel::CohereTranscribe => {
            model.cohere_transcribe = OfflineCohereTranscribeModelConfig {
                encoder: Some(path("encoder.int8.onnx")?),
                decoder: Some(path("decoder.int8.onnx")?),
                language: language(config).map(str::to_owned),
                use_punct: COHERE_USE_PUNCT,
                use_itn: COHERE_USE_ITN,
            };
            model.tokens = Some(path("tokens.txt")?);
        }
        AsrModel::MoonshineBase => {
            // v2 exports: one encoder and one merged (cached/uncached) decoder.
            model.moonshine = OfflineMoonshineModelConfig {
                encoder: Some(path("encoder_model.ort")?),
                merged_decoder: Some(path("decoder_model_merged.ort")?),
                ..Default::default()
            };
            model.tokens = Some(path("tokens.txt")?);
        }
        _ => unreachable!(),
    }
    Ok(OfflineRecognizerConfig {
        model_config: model,
        decoding_method: Some("greedy_search".into()),
        ..Default::default()
    })
}

pub(crate) fn validate_audio(
    audio: &MonoPcm,
    config: &AppConfig,
    cancelled: &AtomicBool,
) -> Result<()> {
    check_cancelled(cancelled)?;
    config.validate()?;
    if config.models.custom_asr.is_some() || !config.models.asr.requires_completion_report() {
        return Err(InferenceError::UnsupportedAsr(config.models.asr.id()));
    }
    audio.require_sample_rate(ASR_SAMPLE_RATE)?;
    Ok(())
}

#[cfg(test)]
fn result_text(
    stream: &OfflineStream,
    config: &AppConfig,
    cancelled: &AtomicBool,
) -> Result<String> {
    result_text_for(stream, config, cancelled, DecodePurpose::Completed)
}

fn result_text_for(
    stream: &OfflineStream,
    config: &AppConfig,
    cancelled: &AtomicBool,
    purpose: DecodePurpose,
) -> Result<String> {
    check_cancelled(cancelled)?;
    let result = stream
        .get_result()
        .ok_or_else(|| native("speech decode", "no recognition result returned"))?;
    let text = accepted_text(&stream.get_option(REPORT_OPTION), &result.text, purpose)?;
    if config.models.asr.is_whisper() {
        crate::whisper::completed_text(
            &stream.get_option(crate::whisper::SCORE_OPTION),
            text,
            &stream.get_option("vesper.whisper_language.v1"),
        )
    } else {
        Ok(text)
    }
}

#[cfg(test)]
pub(crate) fn transcribe(
    audio: &MonoPcm,
    config: &AppConfig,
    root: &Path,
    cancelled: &AtomicBool,
) -> Result<String> {
    transcribe_with_progress(audio, config, root, cancelled, &|_| {})
}

pub(crate) fn transcribe_with_progress(
    audio: &MonoPcm,
    config: &AppConfig,
    root: &Path,
    cancelled: &AtomicBool,
    progress: &dyn Fn(TranscriptionProgress),
) -> Result<String> {
    validate_audio(audio, config, cancelled)?;
    // Exact digital silence is not speech. This is not a volume threshold or VAD:
    // the pinned Qwen export can hallucinate words from all-zero PCM.
    if audio.samples().iter().all(|sample| *sample == 0.0) {
        check_cancelled(cancelled)?;
        progress(TranscriptionProgress {
            processed_samples: audio.samples().len(),
            total_samples: audio.samples().len(),
        });
        check_cancelled(cancelled)?;
        return Ok(String::new());
    }
    let native_config = recognizer_config(config, root)?;
    check_cancelled(cancelled)?;
    let recognizer = OfflineRecognizer::create(&native_config)
        .ok_or_else(|| native("speech model load", "could not load the pinned ONNX export"))?;
    check_cancelled(cancelled)?;
    decode_with_recognizer(
        &recognizer,
        audio,
        config,
        cancelled,
        progress,
        None,
        &|_| {},
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn decode_with_recognizer(
    recognizer: &OfflineRecognizer,
    audio: &MonoPcm,
    config: &AppConfig,
    cancelled: &AtomicBool,
    progress: &dyn Fn(TranscriptionProgress),
    healthy: Option<&std::cell::Cell<bool>>,
    observe: &dyn Fn(SegmentObservation),
) -> Result<String> {
    decode_for_purpose(
        recognizer,
        audio,
        config,
        cancelled,
        progress,
        healthy,
        observe,
        DecodePurpose::Completed,
        &[],
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn decode_for_purpose(
    recognizer: &OfflineRecognizer,
    audio: &MonoPcm,
    config: &AppConfig,
    cancelled: &AtomicBool,
    progress: &dyn Fn(TranscriptionProgress),
    healthy: Option<&std::cell::Cell<bool>>,
    observe: &dyn Fn(SegmentObservation),
    purpose: DecodePurpose,
    hints: &[String],
) -> Result<String> {
    validate_audio(audio, config, cancelled)?;
    let hotwords = qwen_hotwords(config, hints);
    if audio.samples().iter().all(|sample| *sample == 0.0) {
        progress(TranscriptionProgress {
            processed_samples: audio.samples().len(),
            total_samples: audio.samples().len(),
        });
        check_cancelled(cancelled)?;
        return Ok(String::new());
    }
    let decode_part = |samples: &[f32]| {
        decode_segment_once(
            recognizer, samples, config, cancelled, healthy, observe, purpose, &hotwords,
        )
    };
    decode_segments_with_limit(
        audio.samples(),
        cancelled,
        if config.models.asr.is_whisper() {
            crate::whisper::MAX_SEGMENT_SECONDS
        } else if config.models.asr == AsrModel::MoonshineBase {
            MOONSHINE_MAX_SEGMENT_SECONDS
        } else {
            MAX_SEGMENT_SECONDS
        } * ASR_SAMPLE_RATE as usize,
        |samples| {
            let text = decode_part(samples)?;
            if config.models.asr != AsrModel::MoonshineBase
                || !moonshine_retry_eligible(samples, &text)
            {
                return Ok(text);
            }
            // Moonshine sometimes emits EOS first on clear speech. Its reference
            // runtime decodes shorter VAD segments, so retry once as two halves
            // split at the quietest point near the middle.
            let split = quietest_split(samples);
            let first = decode_part(&samples[..split])?;
            let second = decode_part(&samples[split..])?;
            Ok([first, second]
                .into_iter()
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
                .join(" "))
        },
        progress,
    )
}

/// Qwen3-ASR context words for the pinned sherpa-onnx prompt. The native side
/// splits this on ASCII commas, trims each item and joins them with single spaces
/// inside the chat template's system turn (`offline-recognizer-qwen3-asr-impl.cc`
/// `Qwen3FormatHotwordsForPrompt`, `BuildSourceIds`). The prompt counts against
/// `max_total_len`; overflow truncates audio and the completion report then
/// rejects the result. Other models ignore hints here.
pub(crate) fn qwen_hotwords(config: &AppConfig, hints: &[String]) -> String {
    if config.models.asr != AsrModel::Qwen3Asr06b {
        return String::new();
    }
    hints
        .iter()
        .map(|hint| hint.trim())
        .filter(|hint| !hint.is_empty() && !hint.contains(','))
        .collect::<Vec<_>>()
        .join(",")
}

/// Retry only non-silent parts that decoded to nothing: the native recognizer
/// reports digital silence (|x| <= 1e-6) separately and returns no text for it.
fn moonshine_retry_eligible(samples: &[f32], text: &str) -> bool {
    text.is_empty()
        && samples.len() >= 2 * ASR_SAMPLE_RATE as usize
        && samples.iter().any(|sample| sample.abs() > 1e-6)
}

/// Quietest 100 ms window within 1.5 s of the middle; both halves stay >= 0.5 s.
fn quietest_split(samples: &[f32]) -> usize {
    let rate = ASR_SAMPLE_RATE as usize;
    let (window, middle, reach) = (rate / 10, samples.len() / 2, rate * 3 / 2);
    let low = (middle.saturating_sub(reach)).max(rate / 2 + window / 2);
    let high = (middle + reach).min(samples.len() - rate / 2 - window / 2);
    (low..=high.max(low))
        .step_by(rate / 100)
        .min_by(|a, b| {
            let energy = |c: usize| -> f64 {
                samples[c - window / 2..c + window / 2]
                    .iter()
                    .map(|x| f64::from(*x).powi(2))
                    .sum()
            };
            energy(*a).total_cmp(&energy(*b))
        })
        .unwrap_or(middle)
}

#[allow(clippy::too_many_arguments)]
fn decode_segment_once(
    recognizer: &OfflineRecognizer,
    samples: &[f32],
    config: &AppConfig,
    cancelled: &AtomicBool,
    healthy: Option<&std::cell::Cell<bool>>,
    observe: &dyn Fn(SegmentObservation),
    purpose: DecodePurpose,
    hotwords: &str,
) -> Result<String> {
    // Only weights/sessions may persist. Every segment gets fresh features,
    // options and generated decoding state, even across recordings.
    let started = std::time::Instant::now();
    let stream = recognizer.create_stream();
    if let Some(language) = language(config) {
        stream.set_option("language", language);
    }
    if !hotwords.is_empty() {
        // Per-stream, so hints never leak into the next segment or recording.
        stream.set_option("hotwords", hotwords);
    }
    stream.accept_waveform(ASR_SAMPLE_RATE as i32, samples);
    let accept_us = started.elapsed().as_micros().min(u64::MAX as u128) as u64;
    check_cancelled(cancelled)?;
    // Offline decode cannot be interrupted; cancellation discards its result.
    let started = std::time::Instant::now();
    recognizer.decode(&stream);
    observe(SegmentObservation {
        samples: samples.len(),
        accept_us,
        decode_us: started.elapsed().as_micros().min(u64::MAX as u128) as u64,
        completion: serde_json::from_str(&stream.get_option(REPORT_OPTION)).ok(),
        whisper_scores: config
            .models
            .asr
            .is_whisper()
            .then(|| {
                crate::whisper::Scores::parse(&stream.get_option(crate::whisper::SCORE_OPTION)).ok()
            })
            .flatten(),
    });
    if let Some(healthy) = healthy {
        // Inspect native completion health even when cancellation discards
        // its text, so an unknown fault cannot leave a cached session.
        let result = result_text_for(&stream, config, &AtomicBool::new(false), purpose);
        retain_native_health(result, healthy, cancelled)
    } else {
        result_text_for(&stream, config, cancelled, purpose)
    }
}

fn retain_native_health(
    result: Result<String>,
    healthy: &std::cell::Cell<bool>,
    cancelled: &AtomicBool,
) -> Result<String> {
    if result
        .as_ref()
        .is_err_and(|error| !matches!(error, InferenceError::IncompletePreview(_)))
    {
        healthy.set(false);
    }
    check_cancelled(cancelled)?;
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moonshine_retry_only_for_empty_non_silent_parts_of_two_seconds_or_more() {
        let rate = ASR_SAMPLE_RATE as usize;
        let speech = vec![0.1; 3 * rate];
        assert!(moonshine_retry_eligible(&speech, ""));
        assert!(!moonshine_retry_eligible(&speech, "text"));
        assert!(!moonshine_retry_eligible(&vec![0.1; rate], ""));
        // Digital silence is reported natively and never retried.
        assert!(!moonshine_retry_eligible(&vec![1e-7; 3 * rate], ""));
    }

    #[test]
    fn quietest_split_prefers_a_pause_near_the_middle_and_keeps_halves() {
        let rate = ASR_SAMPLE_RATE as usize;
        let mut samples = vec![0.2_f32; 10 * rate];
        // A 300 ms pause 1 s after the middle.
        samples[6 * rate..6 * rate + 3 * rate / 10].fill(0.0);
        let split = quietest_split(&samples);
        assert!(
            (6 * rate..6 * rate + 3 * rate / 10).contains(&split),
            "{split}"
        );
        // Short inputs still leave at least 0.5 s on each side.
        let short = vec![0.2_f32; 2 * rate];
        let split = quietest_split(&short);
        assert!(split >= rate / 2 && short.len() - split >= rate / 2);
    }

    #[test]
    fn duration_limited_drafts_cannot_pass_completed_validation() {
        let report = r#"{"schemaVersion":1,"stopReason":"duration_token_limit","inputContextTruncated":false}"#;
        assert_eq!(
            accepted_text(report, "draft", DecodePurpose::Preview).unwrap(),
            "draft"
        );
        assert!(completed_text(report, "draft").is_err());
        for reason in [
            "context_limit",
            "repetition",
            "max_new_tokens",
            "model_token_limit",
        ] {
            let report = serde_json::json!({"schemaVersion":1,"stopReason":reason,"inputContextTruncated":false}).to_string();
            assert!(matches!(
                accepted_text(&report, "draft", DecodePurpose::Preview),
                Err(InferenceError::IncompletePreview(_))
            ));
            assert!(completed_text(&report, "draft").is_err());
        }
        for reason in ["error", "unknown"] {
            let report = serde_json::json!({"schemaVersion":1,"stopReason":reason,"inputContextTruncated":false}).to_string();
            assert!(matches!(
                accepted_text(&report, "draft", DecodePurpose::Preview),
                Err(InferenceError::Native { .. })
            ));
        }
        let truncated = report.replace("false", "true");
        assert!(accepted_text(&truncated, "draft", DecodePurpose::Preview).is_err());
        assert!(accepted_text("{}", "draft", DecodePurpose::Preview).is_err());
    }
    use serde_json::json;

    #[test]
    fn cancellation_does_not_hide_failed_native_health() {
        let healthy = std::cell::Cell::new(true);
        assert!(matches!(
            retain_native_health(Ok("complete".into()), &healthy, &AtomicBool::new(true)),
            Err(InferenceError::Cancelled)
        ));
        assert!(healthy.get());
        assert!(matches!(
            retain_native_health(
                Err(native("decode", "incomplete")),
                &healthy,
                &AtomicBool::new(true)
            ),
            Err(InferenceError::Cancelled)
        ));
        assert!(!healthy.get());
    }

    #[test]
    fn incomplete_preview_keeps_native_health() {
        let healthy = std::cell::Cell::new(true);
        assert!(matches!(
            retain_native_health(
                Err(InferenceError::IncompletePreview("repetition".into())),
                &healthy,
                &AtomicBool::new(false)
            ),
            Err(InferenceError::IncompletePreview(_))
        ));
        assert!(healthy.get());
    }

    // Explicit opt-in only; these tests never fetch audio or model weights.
    fn qualify(model: AsrModel) {
        use std::{path::PathBuf, time::Instant};
        let root = PathBuf::from(
            vesper_core::environment::var("VESPER_MODEL_DIR").expect("set VESPER_MODEL_DIR"),
        );
        let fixtures = PathBuf::from(
            vesper_core::environment::var("VESPER_ASR_FIXTURES")
                .expect("set VESPER_ASR_FIXTURES to verified FLEURS fixtures"),
        );
        let mut config = AppConfig::default();
        config.models.asr = model;
        config.cleanup_enabled = false;
        config.language = Language::English;
        let cancelled = AtomicBool::new(false);
        vesper_models::verify_asr(&config, &root, &cancelled).unwrap();
        let mut cases = vec![
            ("fleurs-en_us.wav", Language::English),
            ("fleurs-fr_fr.wav", Language::French),
            ("fleurs-en_us-padded-10s.wav", Language::English),
            ("silence-1s.wav", Language::English),
            ("silence-10s.wav", Language::English),
        ];
        if model == AsrModel::Qwen3Asr06b {
            cases.push(("fleurs-hi_in.wav", Language::Hindi));
            cases.push(("fleurs-en_us.wav", Language::Auto));
        }
        for (file, choice) in cases {
            config.language = choice;
            let audio = vesper_audio::decode_file(
                fixtures.join(file),
                config.effective_recording_seconds(),
            )
            .unwrap();
            if file.starts_with("silence") {
                let started = Instant::now();
                assert!(transcribe(&audio, &config, &root, &cancelled)
                    .unwrap()
                    .is_empty());
                println!(
                    "{}",
                    json!({"model":model.id(),"fixture":file,"audioSeconds":audio.duration_seconds(),"applicationDigitalSilence":true,"seconds":started.elapsed().as_secs_f64()})
                );
                continue;
            }
            let native_config = recognizer_config(&config, &root).unwrap();
            let started = Instant::now();
            let recognizer = OfflineRecognizer::create(&native_config).expect("native recognizer");
            let loaded = started.elapsed();
            let stream = recognizer.create_stream();
            if let Some(language) = language(&config) {
                stream.set_option("language", language);
            }
            stream.accept_waveform(ASR_SAMPLE_RATE as i32, audio.samples());
            let decoding = Instant::now();
            recognizer.decode(&stream);
            let elapsed = decoding.elapsed();
            let report = stream.get_option(REPORT_OPTION);
            let text = result_text(&stream, &config, &cancelled);
            println!(
                "{}",
                json!({"model":model.id(),"fixture":file,"language":choice,"audioSeconds":audio.duration_seconds(),"loadSeconds":loaded.as_secs_f64(),"decodeSeconds":elapsed.as_secs_f64(),"report":report,"text":text.as_ref().ok(),"error":text.as_ref().err().map(ToString::to_string)})
            );
            let text = text.expect("complete public-fixture result");
            if file.starts_with("silence") {
                assert!(text.is_empty());
            } else {
                assert!(!text.trim().is_empty());
            }
            assert!(matches!(
                result_text(&stream, &config, &AtomicBool::new(true)),
                Err(InferenceError::Cancelled)
            ));
            // A previously successful report must not survive a later decode's failure.
            if file == "fleurs-en_us.wav" && choice == Language::English {
                if model == AsrModel::Qwen3Asr06b {
                    stream.set_option("max_new_tokens", "1");
                } else {
                    stream.set_option("language", "invalid");
                }
                recognizer.decode(&stream);
                assert!(result_text(&stream, &config, &cancelled).is_err());
                println!(
                    "{}",
                    json!({"model":model.id(),"case":"reuse-must-reject", "report":stream.get_option(REPORT_OPTION)})
                );
                if model == AsrModel::Qwen3Asr06b {
                    stream.set_option("max_new_tokens", &QWEN_CONTEXT_TOKENS.to_string());
                    stream.set_option("max_total_len", "40");
                    recognizer.decode(&stream);
                    assert!(result_text(&stream, &config, &cancelled).is_err());
                    println!(
                        "{}",
                        json!({"model":model.id(),"case":"forced-context-loss", "report":stream.get_option(REPORT_OPTION)})
                    );
                }
            }
        }
        let beyond_old_limit =
            vesper_audio::decode_file(fixtures.join("silence-10s-plus-one-sample.wav"), 11)
                .unwrap();
        assert!(transcribe(&beyond_old_limit, &config, &root, &cancelled)
            .unwrap()
            .is_empty());
    }

    #[test]
    #[ignore = "explicit installed-model/public-fixture qualification; no downloads"]
    fn qualify_qwen3_asr_public_fixtures() {
        qualify(AsrModel::Qwen3Asr06b);
    }

    #[test]
    #[ignore = "explicit installed-model/public-fixture qualification; no downloads"]
    fn qualify_cohere_public_fixtures() {
        qualify(AsrModel::CohereTranscribe);
    }

    fn qualify_long(model: AsrModel) {
        use std::{cell::RefCell, path::PathBuf, sync::atomic::Ordering, time::Instant};
        let root = PathBuf::from(vesper_core::environment::var_os("VESPER_MODEL_DIR").unwrap());
        let fixtures =
            PathBuf::from(vesper_core::environment::var_os("VESPER_ASR_FIXTURES").unwrap());
        let mut config = AppConfig::default();
        config.models.asr = model;
        config.language = Language::English;
        config.cleanup_enabled = false;
        let cancelled = AtomicBool::new(false);
        vesper_models::verify_asr(&config, &root, &cancelled).unwrap();
        let mut cases = vec![
            ("fleurs-en_us.wav", Language::English),
            ("fleurs-fr_fr.wav", Language::French),
        ];
        if model == AsrModel::Qwen3Asr06b {
            cases.extend([
                ("fleurs-hi_in.wav", Language::Hindi),
                ("fleurs-hi_in.wav", Language::Auto),
            ]);
        }
        let mut failures = Vec::new();
        for (file, language) in cases {
            config.language = language;
            let source = vesper_audio::decode_file(fixtures.join(file), 0).unwrap();
            // Preserve source PCM and repeated speech. Padding shifts boundaries,
            // while the continuous case adds no artificial gaps between copies.
            for padding_samples in [0, 2 * ASR_SAMPLE_RATE as usize] {
                let mut samples = vec![0.0; padding_samples];
                samples.extend(source.samples().repeat(3));
                let audio = MonoPcm::new(samples, ASR_SAMPLE_RATE).unwrap();
                let updates = RefCell::new(Vec::new());
                let started = Instant::now();
                let result = transcribe_with_progress(&audio, &config, &root, &cancelled, &|p| {
                    updates.borrow_mut().push(json!({"endSample":p.processed_samples,"totalSamples":p.total_samples,"elapsedSeconds":started.elapsed().as_secs_f64()}));
                });
                println!(
                    "{}",
                    json!({"case":"long-production-path","model":model.id(),"fixture":file,"language":language,"repeat":3,"leadingSilenceSamples":padding_samples,"audioSeconds":audio.duration_seconds(),"seconds":started.elapsed().as_secs_f64(),"segments":updates.borrow().as_slice(),"text":result.as_ref().ok(),"error":result.as_ref().err().map(ToString::to_string)})
                );
                match result {
                    Ok(text) => {
                        assert!(!text.is_empty());
                        assert_eq!(
                            updates.borrow().last().unwrap()["endSample"],
                            audio.samples().len()
                        );
                    }
                    Err(error) => {
                        failures.push(format!("{file}/{language:?}/{padding_samples}: {error}"))
                    }
                }
            }
        }
        config.language = Language::English;
        let source = vesper_audio::decode_file(fixtures.join("fleurs-en_us.wav"), 0).unwrap();
        let audio = MonoPcm::new(source.samples().repeat(3), ASR_SAMPLE_RATE).unwrap();
        // Deterministic cancellation at the first accepted part, through production.
        let result = transcribe_with_progress(&audio, &config, &root, &cancelled, &|_| {
            cancelled.store(true, Ordering::Relaxed)
        });
        assert!(matches!(result, Err(InferenceError::Cancelled)));
        cancelled.store(false, Ordering::Relaxed);

        // One native recognizer, three fresh streams: A / B / A must isolate features,
        // options and decoder cache. Then inject a failed second segment and retry A.
        let native_config = recognizer_config(&config, &root).unwrap();
        let recognizer = OfflineRecognizer::create(&native_config).unwrap();
        let other = vesper_audio::decode_file(fixtures.join("fleurs-fr_fr.wav"), 0).unwrap();
        let decode = |samples: &[f32], fail: bool| {
            let stream = recognizer.create_stream();
            stream.set_option("language", super::language(&config).unwrap());
            if fail {
                if model == AsrModel::Qwen3Asr06b {
                    stream.set_option("max_new_tokens", "1");
                } else {
                    stream.set_option("language", "invalid");
                }
            }
            stream.accept_waveform(ASR_SAMPLE_RATE as i32, samples);
            recognizer.decode(&stream);
            result_text(&stream, &config, &cancelled)
        };
        let first = decode(source.samples(), false).unwrap();
        let _ = decode(other.samples(), false); // B is intentionally prompted as English.
        assert_eq!(decode(source.samples(), false).unwrap(), first);
        let mut calls = 0;
        let failed = decode_segments(
            audio.samples(),
            &cancelled,
            |samples| {
                calls += 1;
                decode(samples, calls == 2)
            },
            &|_| {},
        );
        assert_eq!(calls, 2);
        assert!(failed.is_err());
        assert_eq!(decode(source.samples(), false).unwrap(), first);
        println!(
            "{}",
            json!({"model":model.id(),"case":"native-isolation-failure-cancellation","passed":true})
        );
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    #[test]
    #[ignore = "installed models and verified public FLEURS audio; no downloads"]
    fn qualify_qwen_long_audio() {
        qualify_long(AsrModel::Qwen3Asr06b);
    }

    #[test]
    #[ignore = "installed models and verified public FLEURS audio; no downloads"]
    fn qualify_cohere_long_audio() {
        qualify_long(AsrModel::CohereTranscribe);
    }

    #[test]
    fn only_confirmed_completion_or_empty_native_silence_is_accepted() {
        for reason in [
            "eos",
            "silence_detected",
            "no_input",
            "max_new_tokens",
            "context_limit",
            "duration_token_limit",
            "model_token_limit",
            "repetition",
            "error",
            "pending",
            "unknown",
        ] {
            for truncated in [false, true] {
                let report = json!({"schemaVersion":1,"stopReason":reason,"inputContextTruncated":truncated}).to_string();
                assert_eq!(
                    completed_text(&report, "a partial sentence").is_ok(),
                    reason == "eos" && !truncated
                );
                assert_eq!(
                    completed_text(&report, "").is_ok(),
                    matches!(reason, "eos" | "silence_detected" | "no_input") && !truncated
                );
            }
        }
        for report in [
            "",
            "{}",
            "null",
            r#"{"schemaVersion":2,"stopReason":"eos","inputContextTruncated":false}"#,
        ] {
            assert!(completed_text(report, "plausible partial text").is_err());
        }
    }

    #[test]
    fn admission_and_cancellation_precede_model_loading() {
        let mut config = AppConfig::default();
        config.models.asr = AsrModel::Qwen3Asr06b;
        let root = Path::new("missing-models");
        let cancelled = AtomicBool::new(false);
        let over = MonoPcm::new(vec![0.01; 160_001], ASR_SAMPLE_RATE).unwrap();
        assert!(matches!(
            transcribe(&over, &config, root, &cancelled),
            Err(InferenceError::MissingModel(_))
        ));
        let boundary = MonoPcm::new(vec![0.01; 160_000], ASR_SAMPLE_RATE).unwrap();
        assert!(matches!(
            transcribe(&boundary, &config, root, &cancelled),
            Err(InferenceError::MissingModel(_))
        ));
        let silence = MonoPcm::new(vec![0.0; 1_600_001], ASR_SAMPLE_RATE).unwrap();
        assert!(transcribe(&silence, &config, root, &cancelled)
            .unwrap()
            .is_empty());
        assert!(matches!(
            transcribe(&boundary, &config, root, &AtomicBool::new(true)),
            Err(InferenceError::Cancelled)
        ));
        let wrong_rate = MonoPcm::new(vec![0.0; 16], 48_000).unwrap();
        assert!(matches!(
            transcribe(&wrong_rate, &config, root, &cancelled),
            Err(InferenceError::Configuration(_))
        ));
        config.models.asr = AsrModel::CohereTranscribe;
        config.language = Language::Auto;
        assert!(matches!(
            transcribe(&boundary, &config, root, &cancelled),
            Err(InferenceError::Configuration(_))
        ));
    }

    #[test]
    fn qwen_hotwords_use_the_native_comma_list_and_only_reach_qwen() {
        let mut config = AppConfig::default();
        config.models.asr = AsrModel::Qwen3Asr06b;
        let hints = ["kubectl", " Cedar Point ", "a,b", ""].map(String::from);
        assert_eq!(super::qwen_hotwords(&config, &hints), "kubectl,Cedar Point");
        assert_eq!(super::qwen_hotwords(&config, &[]), "");
        config.models.asr = AsrModel::CohereTranscribe;
        assert_eq!(super::qwen_hotwords(&config, &hints), "");
    }

    #[test]
    fn language_mapping_matches_each_native_prompt_contract() {
        let mut config = AppConfig::default();
        config.models.asr = AsrModel::Qwen3Asr06b;
        for (choice, expected) in [
            (Language::Auto, None),
            (Language::English, Some("English")),
            (Language::French, Some("French")),
            (Language::Hindi, Some("Hindi")),
        ] {
            config.language = choice;
            assert_eq!(language(&config), expected);
        }
        config.models.asr = AsrModel::CohereTranscribe;
        config.language = Language::French;
        assert_eq!(language(&config), Some("fr"));
        config.language = Language::English;
        assert_eq!(language(&config), Some("en"));
    }

    #[test]
    fn vendor_prompts_match_the_manifest_for_every_declared_language() {
        // Manifest-driven: extending Qwen/Cohere support extends this coverage
        // with no test edit.
        let manifest: serde_json::Value =
            serde_json::from_str(include_str!("../../../models/languages.json")).unwrap();
        for (id, runtime, model) in [
            ("Qwen/Qwen3-ASR-0.6B", "qwen", AsrModel::Qwen3Asr06b),
            (
                "CohereLabs/cohere-transcribe-03-2026",
                "sherpa",
                AsrModel::CohereTranscribe,
            ),
        ] {
            let entry = &manifest[id];
            let mut count = 0;
            for (iso, expected) in entry["runtimeCodes"][runtime].as_object().unwrap() {
                let mut config = AppConfig::default();
                config.models.asr = model;
                config.language = serde_json::from_value(iso.clone().into()).unwrap();
                assert_eq!(
                    language(&config),
                    Some(expected.as_str().unwrap()),
                    "{id} {iso}"
                );
                count += 1;
            }
            assert_eq!(
                count,
                entry["languages"].as_array().unwrap().len(),
                "every offered language needs a {runtime} prompt"
            );
        }
    }

    #[test]
    fn segments_cover_every_sample_once_without_tiny_tails() {
        use std::cell::RefCell;
        for size in [
            1,
            MAX_SEGMENT_SAMPLES - 1,
            MAX_SEGMENT_SAMPLES,
            MAX_SEGMENT_SAMPLES + 1,
            2 * MAX_SEGMENT_SAMPLES,
            2 * MAX_SEGMENT_SAMPLES + 1,
            7 * MAX_SEGMENT_SAMPLES + 317,
        ] {
            let samples: Vec<_> = (0..size).map(|i| (i % 137 + 1) as f32 / 137.0).collect();
            let mut received = Vec::new();
            let progress = RefCell::new(Vec::new());
            let mut calls = 0;
            let output = decode_segments(
                &samples,
                &AtomicBool::new(false),
                |part| {
                    assert!(!part.is_empty() && part.len() <= MAX_SEGMENT_SAMPLES);
                    if size > MAX_SEGMENT_SAMPLES {
                        assert!(part.len() >= 4 * ASR_SAMPLE_RATE as usize);
                    }
                    received.extend_from_slice(part);
                    calls += 1;
                    Ok(format!("word{calls}"))
                },
                &|update| {
                    progress.borrow_mut().push(update);
                },
            )
            .unwrap();
            assert_eq!(received, samples);
            assert_eq!(
                output,
                (1..=calls)
                    .map(|n| format!("word{n}"))
                    .collect::<Vec<_>>()
                    .join(" ")
            );
            let updates = progress.into_inner();
            assert_eq!(updates.last().unwrap().processed_samples, size);
            assert!(updates.iter().all(|update| update.total_samples == size));
            assert!(updates
                .windows(2)
                .all(|pair| pair[0].processed_samples < pair[1].processed_samples));
        }
    }

    #[test]
    fn quieter_boundaries_preserve_silence_and_genuine_repetition() {
        let mut samples = vec![0.2; MAX_SEGMENT_SAMPLES * 2];
        let quiet = MAX_SEGMENT_SAMPLES - ASR_SAMPLE_RATE as usize / 2;
        samples[quiet - ENERGY_WINDOW..quiet + ENERGY_WINDOW].fill(0.0);
        let end = segment_end(&samples, 0);
        assert!(end >= quiet - ENERGY_WINDOW / 2 && end <= quiet + ENERGY_WINDOW / 2);
        let mut samples = vec![0.0; MAX_SEGMENT_SAMPLES * 3];
        samples[0] = 0.5;
        *samples.last_mut().unwrap() = 0.5;
        let mut calls = 0;
        let output = decode_segments(
            &samples,
            &AtomicBool::new(false),
            |part| {
                assert!(part.iter().any(|sample| *sample != 0.0));
                calls += 1;
                Ok(" again again ".into())
            },
            &|_| {},
        )
        .unwrap();
        assert_eq!(calls, 2);
        assert_eq!(output, "again again again again");
    }

    #[test]
    fn a_failed_segment_cannot_return_a_successful_prefix_or_contaminate_the_next_job() {
        let samples = vec![0.25; MAX_SEGMENT_SAMPLES * 3];
        let mut calls = 0;
        let result = decode_segments(
            &samples,
            &AtomicBool::new(false),
            |_| {
                calls += 1;
                if calls == 2 {
                    completed_text(
                        r#"{"schemaVersion":1,"stopReason":"context_limit","inputContextTruncated":false}"#,
                        "plausible incomplete text",
                    )
                } else {
                    Ok("first part".into())
                }
            },
            &|_| {},
        );
        assert_eq!(calls, 2);
        assert!(result.unwrap_err().to_string().contains("part 2"));
        assert_eq!(
            decode_segments(
                &samples[..100],
                &AtomicBool::new(false),
                |_| Ok("new job".into()),
                &|_| {}
            )
            .unwrap(),
            "new job"
        );
    }

    #[test]
    fn cancellation_before_during_between_and_after_segments_never_returns_text() {
        use std::sync::atomic::Ordering;
        let samples = vec![0.25; MAX_SEGMENT_SAMPLES * 3];
        let result = decode_segments(
            &samples,
            &AtomicBool::new(true),
            |_| panic!("must not decode"),
            &|_| {},
        );
        assert!(matches!(result, Err(InferenceError::Cancelled)));
        for cancel_during_decode in [true, false] {
            for input in [&samples[..100], &samples[..]] {
                let cancelled = AtomicBool::new(false);
                let mut calls = 0;
                let result = decode_segments(
                    input,
                    &cancelled,
                    |_| {
                        calls += 1;
                        if cancel_during_decode {
                            cancelled.store(true, Ordering::Relaxed);
                        }
                        Ok("must be discarded".into())
                    },
                    &|_| {
                        cancelled.store(true, Ordering::Relaxed);
                    },
                );
                assert!(matches!(result, Err(InferenceError::Cancelled)));
                assert_eq!(calls, 1);
            }
        }
    }
}
