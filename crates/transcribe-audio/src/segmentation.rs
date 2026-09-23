//! Boundary selection for completed 16 kHz audio. No overlap or sample removal.
use crate::ASR_SAMPLE_RATE;

/// Choose the next complete-audio segment boundary, balancing the remaining
/// input and preferring a quiet 100 ms window in the final second. `maximum`
/// comes from the compiled architecture contract, not a recording duration cap.
///
/// The caller supplies finite mono 16 kHz PCM and retains every returned range.
/// Panics if `start` is beyond the input or `maximum` is under four seconds.
pub fn completed_segment_end(samples: &[f32], start: usize, maximum: usize) -> usize {
    assert!(start <= samples.len());
    assert!(maximum >= 4 * ASR_SAMPLE_RATE as usize);
    let remaining = samples.len() - start;
    if remaining <= maximum {
        return samples.len();
    }
    let target = start + remaining.div_ceil(remaining.div_ceil(maximum));
    let window = ASR_SAMPLE_RATE as usize / 10;
    let mut boundary = target;
    let mut lowest_energy = f64::INFINITY;
    for offset in (0..=ASR_SAMPLE_RATE as usize).step_by(ASR_SAMPLE_RATE as usize / 100) {
        let center = target - offset;
        let energy: f64 = samples[center - window / 2..center + window / 2]
            .iter()
            .map(|sample| f64::from(*sample).powi(2))
            .sum();
        // Prefer the later boundary on ties, including completely silent windows.
        if energy < lowest_energy {
            lowest_energy = energy;
            boundary = center;
        }
    }
    boundary
}
