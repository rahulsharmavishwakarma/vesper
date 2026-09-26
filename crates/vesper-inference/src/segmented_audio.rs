//! Bounded completed-audio delivery shared by offline architectures.
//! No overlap or silence trimming; this is not native streaming.
use crate::{check_cancelled, native, InferenceError, Result, TranscriptionProgress};
use std::sync::atomic::AtomicBool;
use vesper_audio::ASR_SAMPLE_RATE;

// This bounds each native call,
// not recording or imported-file duration. Update provenance if this policy changes.
pub(crate) const MAX_SEGMENT_SECONDS: usize = 10;
pub(crate) const MAX_SEGMENT_SAMPLES: usize = MAX_SEGMENT_SECONDS * ASR_SAMPLE_RATE as usize;
#[cfg(test)]
pub(crate) const ENERGY_WINDOW: usize = ASR_SAMPLE_RATE as usize / 10;

/// Balance the remaining input before searching the final second for a quieter
/// 100 ms window. This avoids a tiny tail for inputs such as 10 seconds + 1 sample.
/// Energy only chooses a boundary; it never removes quiet audio or detects speech.
#[cfg(test)]
pub(crate) fn segment_end(samples: &[f32], start: usize) -> usize {
    segment_end_with_limit(samples, start, MAX_SEGMENT_SAMPLES)
}

fn segment_end_with_limit(samples: &[f32], start: usize, maximum: usize) -> usize {
    vesper_audio::completed_segment_end(samples, start, maximum)
}

pub(crate) fn decode_segments(
    samples: &[f32],
    cancelled: &AtomicBool,
    decode: impl FnMut(&[f32]) -> Result<String>,
    progress: &dyn Fn(TranscriptionProgress),
) -> Result<String> {
    decode_segments_with_limit(samples, cancelled, MAX_SEGMENT_SAMPLES, decode, progress)
}

/// Limits come from compiled architecture contracts, never user-controlled input.
pub(crate) fn decode_segments_with_limit(
    samples: &[f32],
    cancelled: &AtomicBool,
    maximum: usize,
    mut decode: impl FnMut(&[f32]) -> Result<String>,
    progress: &dyn Fn(TranscriptionProgress),
) -> Result<String> {
    // A balanced half-window must accommodate the energy search on both sides.
    assert!(maximum >= 4 * ASR_SAMPLE_RATE as usize);
    let mut raw = String::new();
    let mut start = 0;
    let mut segment = 0;
    while start < samples.len() {
        check_cancelled(cancelled)?;
        let end = segment_end_with_limit(samples, start, maximum);
        segment += 1;
        let input = &samples[start..end];
        // The exact-zero safeguard also applies to silence between speech segments.
        if input.iter().any(|sample| *sample != 0.0) {
            let text = decode(input).map_err(|error| match error {
                InferenceError::Cancelled => error,
                _ => native(
                    "speech transcription",
                    format!("part {segment} could not be completed: {error}"),
                ),
            })?;
            check_cancelled(cancelled)?;
            let text = text.trim();
            if !text.is_empty() {
                if !raw.is_empty() {
                    raw.push(' ');
                }
                // Never deduplicate words: repetitions can be intentional speech.
                raw.push_str(text);
            }
        }
        check_cancelled(cancelled)?;
        start = end;
        progress(TranscriptionProgress {
            processed_samples: end,
            total_samples: samples.len(),
        });
    }
    // Cancellation raised by the final progress callback must discard the result too.
    check_cancelled(cancelled)?;
    Ok(raw)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[test]
    fn architecture_limits_preserve_all_samples_and_balance_tails() {
        for seconds in [10, crate::whisper::MAX_SEGMENT_SECONDS] {
            let maximum = seconds * ASR_SAMPLE_RATE as usize;
            for size in [maximum - 1, maximum, maximum + 1, maximum * 3 + 197] {
                let samples: Vec<f32> = (0..size).map(|i| (i % 71 + 1) as f32 / 71.0).collect();
                let mut received = Vec::new();
                let progress = RefCell::new(Vec::new());
                decode_segments_with_limit(
                    &samples,
                    &AtomicBool::new(false),
                    maximum,
                    |part| {
                        assert!(part.len() <= maximum);
                        if size > maximum {
                            assert!(part.len() >= maximum / 2 - ASR_SAMPLE_RATE as usize);
                        }
                        received.extend_from_slice(part);
                        Ok("intentional repetition".into())
                    },
                    &|p| progress.borrow_mut().push(p),
                )
                .unwrap();
                assert_eq!(samples, received);
                assert_eq!(progress.borrow().last().unwrap().processed_samples, size);
            }
        }
    }
}
