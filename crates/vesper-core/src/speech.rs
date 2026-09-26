//! Shared completed-result policies, independent of inference backend.
pub const WHISPER_NO_SPEECH_THRESHOLD: f64 = 0.6;
pub const WHISPER_LOG_PROBABILITY_THRESHOLD: f64 = -1.0;

/// OpenAI's combined silence policy. Invalid evidence is never an admission.
/// Call only after successful EOS/nontruncation validation.
pub fn whisper_no_speech(probability: f64, average_log_probability: f64) -> Option<bool> {
    if !probability.is_finite()
        || !(0.0..=1.0).contains(&probability)
        || !average_log_probability.is_finite()
        || average_log_probability > 0.0
    {
        return None;
    }
    Some(
        probability > WHISPER_NO_SPEECH_THRESHOLD
            && average_log_probability <= WHISPER_LOG_PROBABILITY_THRESHOLD,
    )
}
