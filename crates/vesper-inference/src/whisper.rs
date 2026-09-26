//! Whisper-specific model construction and completed-result policy.
//! Scores come from pinned native logits, not text heuristics or a volume gate.
use crate::{native, path_string, require_file, worker_threads, Result};
use serde::{Deserialize, Serialize};
use sherpa_onnx::{OfflineModelConfig, OfflineRecognizerConfig, OfflineWhisperModelConfig};
use std::path::Path;
use vesper_core::{AppConfig, AsrModel, Language};

pub(crate) const SCORE_OPTION: &str = "vesper.whisper_scores.v1";
// Preserve the padding used in the prior qualification screen. The official
// shorter 300-frame suggestion needs its own quality/latency comparison.
pub(crate) const TAIL_PADDING_FRAMES: i32 = 1000;
// Guard below the runtime's 2,950-frame boundary. Preserve Whisper's context
// rather than forcing another architecture's ten-second segmentation contract.
pub(crate) const MAX_SEGMENT_SECONDS: usize = 29;
pub(crate) use vesper_core::{
    WHISPER_LOG_PROBABILITY_THRESHOLD as LOG_PROBABILITY_THRESHOLD,
    WHISPER_NO_SPEECH_THRESHOLD as NO_SPEECH_THRESHOLD,
};

pub(crate) fn export(model: AsrModel) -> (&'static str, &'static str) {
    match model {
        AsrModel::WhisperSmall => ("whisper-small", "8f3c18b358db4d1f2fc1eae49d75cd20989e4309"),
        AsrModel::WhisperTurbo => ("whisper-turbo", "2ca6ff69fc878651b770880507669577ac41c2ff"),
        _ => unreachable!("Whisper architecture selection"),
    }
}

pub(crate) fn recognizer_config(
    config: &AppConfig,
    root: &Path,
) -> Result<OfflineRecognizerConfig> {
    let folder = root.join(export(config.models.asr).0);
    let path = |file: &str| {
        let path = folder.join(file);
        require_file(&path)?;
        path_string(&path)
    };
    Ok(OfflineRecognizerConfig {
        model_config: OfflineModelConfig {
            whisper: OfflineWhisperModelConfig {
                encoder: Some(path("encoder.int8.onnx")?),
                decoder: Some(path("decoder.int8.onnx")?),
                language: match config.language {
                    Language::Auto => None,
                    Language::English => Some("en".into()),
                    Language::French => Some("fr".into()),
                    Language::Hindi => {
                        return Err(native("Whisper language", "Hindi is not qualified"))
                    }
                },
                task: Some("transcribe".into()),
                tail_paddings: TAIL_PADDING_FRAMES,
                ..Default::default()
            },
            tokens: Some(path("tokens.txt")?),
            provider: Some("cpu".into()),
            num_threads: worker_threads(),
            ..Default::default()
        },
        decoding_method: Some("greedy_search".into()),
        ..Default::default()
    })
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Scores {
    schema_version: u32,
    no_speech_probability: f64,
    average_log_probability: f64,
}

impl Scores {
    pub(crate) fn parse(value: &str) -> Result<Self> {
        let scores: Self = serde_json::from_str(value)
            .map_err(|_| native("Whisper confidence", "missing or invalid native scores"))?;
        if scores.schema_version != 1
            || vesper_core::whisper_no_speech(
                scores.no_speech_probability,
                scores.average_log_probability,
            )
            .is_none()
        {
            return Err(native("Whisper confidence", "invalid native score values"));
        }
        Ok(scores)
    }

    pub(crate) fn no_speech(&self) -> bool {
        // OpenAI transcribe.py's combined silence heuristic. A confident decode
        // overrides high no-speech probability; low confidence alone drops nothing.
        vesper_core::whisper_no_speech(self.no_speech_probability, self.average_log_probability)
            .expect("validated Whisper scores")
    }
}

/// Called only AFTER native EOS/nontruncation validation, never to hide a cap.
pub(crate) fn completed_text(
    scores: &str,
    text: String,
    detected_language: &str,
) -> Result<String> {
    let scores = Scores::parse(scores)?;
    if scores.no_speech() {
        return Ok(String::new());
    }
    if !matches!(detected_language, "en" | "fr") {
        return Err(native(
            "Whisper language",
            "detected language is outside the qualified English/French modes",
        ));
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn whisper_streaming_is_rejected_before_loading_any_model() {
        use std::sync::{atomic::AtomicBool, Arc};
        let manager = crate::InferenceManager::new().unwrap();
        for model in [AsrModel::WhisperSmall, AsrModel::WhisperTurbo] {
            let mut config = AppConfig::default();
            config.models.asr = model;
            assert!(matches!(
                manager.start_stream(
                    &config,
                    Path::new("missing-models"),
                    "unsupported-stream",
                    Arc::new(AtomicBool::new(false)),
                    Arc::new(|_| {})
                ),
                Err(crate::InferenceError::UnsupportedAsr(_))
            ));
        }
    }

    #[test]
    fn combined_silence_policy_preserves_confident_and_low_volume_speech() {
        for (probability, confidence, skipped) in [
            (0.99, -0.1, false),
            (0.99, -1.1, true),
            (0.1, -2.0, false),
            (0.6, -1.0, false),
            (0.6001, -1.0, true),
        ] {
            let scores = json!({"schemaVersion":1,"noSpeechProbability":probability,"averageLogProbability":confidence}).to_string();
            let result = completed_text(&scores, "quiet words".into(), "en").unwrap();
            assert_eq!(result.is_empty(), skipped);
            if !skipped {
                assert_eq!(result, "quiet words");
            }
        }
    }

    #[test]
    fn scores_and_automatic_language_fail_closed() {
        for scores in [
            "",
            "{}",
            "null",
            r#"{"schemaVersion":2,"noSpeechProbability":0.2,"averageLogProbability":-1}"#,
            r#"{"schemaVersion":1,"noSpeechProbability":1.2,"averageLogProbability":-1}"#,
            r#"{"schemaVersion":1,"noSpeechProbability":0.2,"averageLogProbability":1}"#,
        ] {
            assert!(completed_text(scores, "words".into(), "en").is_err());
        }
        let scores =
            r#"{"schemaVersion":1,"noSpeechProbability":0.01,"averageLogProbability":-0.1}"#;
        assert!(completed_text(scores, "words".into(), "hi").is_err());
        assert!(completed_text(scores, "words".into(), "").is_err());
        assert!(completed_text(scores, "words".into(), "fr").is_ok());
    }
}
