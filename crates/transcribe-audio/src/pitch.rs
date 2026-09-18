//! Bounded visual pitch telemetry, evaluated on the existing capture worker.
//! Uses the cumulative-normalized difference and first reliable trough from
//! de Cheveigne/Kawahara's YIN (2002). This is a UI estimate, not a transcript
//! feature or a tuner. No analysis or allocation runs on the audio callback.

const WINDOW: usize = 512;
const COMPARISON: usize = WINDOW / 2;
const MIN_HZ: f32 = 60.0;
const MAX_HZ: f32 = 700.0;

pub(crate) fn estimate_pitch(samples: &[f32], rate: u32, channels: u16) -> Option<f32> {
    if rate < 8_000 || channels == 0 {
        return None;
    }
    // Average short native-rate blocks for this disposable analysis view only.
    // Keeping roughly 8 kHz bounds the difference calculation on high-rate mics.
    // The exact effective rate also handles 44.1 kHz and other non-multiples.
    let stride = (rate as usize / 8_000).max(1);
    let analysis_rate = rate as f32 / stride as f32;
    let channels = usize::from(channels);
    let end = samples.len() / channels * channels;
    let block = stride * channels;
    let start = end.checked_sub(WINDOW * block)?;
    let mut mono = [0.0_f32; WINDOW];
    for (output, input) in mono.iter_mut().zip(samples[start..end].chunks_exact(block)) {
        let mut sum = 0.0_f64;
        for &sample in input {
            if !sample.is_finite() {
                return None;
            }
            sum += f64::from(sample.clamp(-1.0, 1.0));
        }
        *output = (sum / block as f64) as f32;
    }
    let mean = mono.iter().sum::<f32>() / WINDOW as f32;
    let variance = mono
        .iter()
        .map(|sample| (sample - mean).powi(2))
        .sum::<f32>()
        / WINDOW as f32;
    // Reject silence and DC, independent of the callback's RMS meter.
    if variance < 0.000_004 {
        return None;
    }
    let last_lag = (analysis_rate / MIN_HZ).ceil() as usize;
    let mut normalized = [1.0_f32; COMPARISON];
    let mut cumulative = 0.0;
    for lag in 1..=(last_lag + 1).min(COMPARISON - 1) {
        let difference = (0..COMPARISON)
            .map(|index| {
                // Anchor every comparison to the newest half-window. Starting
                // at zero leaves up to ~30 ms of fresh microphone input unused,
                // so brief pitch changes mix with older speech or disappear.
                let recent = WINDOW - COMPARISON + index;
                (mono[recent] - mono[recent - lag]).powi(2)
            })
            .sum::<f32>();
        cumulative += difference;
        if cumulative > f32::EPSILON {
            normalized[lag] = difference * lag as f32 / cumulative;
        }
    }
    // Noise has no reliable period. Do not fall back to its least-bad trough.
    // Inspect short periods too: otherwise a tone above MAX_HZ could have its
    // second period mistaken for a lower, in-range fundamental.
    for lag in 2..=last_lag.min(COMPARISON - 2) {
        let [left, center, right] = [normalized[lag - 1], normalized[lag], normalized[lag + 1]];
        if center >= 0.15 || center >= left || center > right {
            continue;
        }
        let curvature = left - 2.0 * center + right;
        let correction = if curvature.abs() > f32::EPSILON {
            (0.5 * (left - right) / curvature).clamp(-0.5, 0.5)
        } else {
            0.0
        };
        let hz = analysis_rate / (lag as f32 + correction);
        return (MIN_HZ..=MAX_HZ).contains(&hz).then_some(hz);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::TAU;

    fn tone(rate: u32, hz: f32, amplitude: f32, channels: u16) -> Vec<f32> {
        (0..rate / 10)
            .flat_map(|frame| {
                let value = amplitude * (TAU * hz * frame as f32 / rate as f32).sin();
                std::iter::repeat_n(value, channels as usize)
            })
            .collect()
    }

    #[test]
    fn pitch_tracks_frequency_independently_of_volume_and_native_format() {
        for rate in [8_000, 16_000, 44_100, 48_000, 96_000, 192_000] {
            for hz in [65.0, 110.0, 220.0, 440.0, 650.0] {
                for amplitude in [0.01, 0.2, 0.8] {
                    for channels in [1, 2] {
                        let samples = tone(rate, hz, amplitude, channels);
                        let result = estimate_pitch(&samples, rate, channels).unwrap();
                        assert!((result - hz).abs() / hz < 0.025,
                            "{rate} Hz, {channels} channels, amplitude {amplitude}: {result} vs {hz}");
                    }
                }
            }
        }
    }

    #[test]
    fn pitch_uses_recent_audio_after_a_change_in_voice_pitch() {
        for rate in [16_000, 44_100, 48_000] {
            let mut samples = tone(rate, 110.0, 0.2, 1);
            let next = tone(rate, 330.0, 0.2, 1);
            samples.extend_from_slice(&next[..(rate as usize * 40 / 1000)]);
            let measured = estimate_pitch(&samples, rate, 1);
            assert!(
                measured.is_some_and(|hz| (hz - 330.0).abs() < 10.0),
                "Newest 40 ms should follow the new tone at {rate} Hz; got {measured:?}"
            );
        }
    }

    #[test]
    fn pitch_finds_fundamental_with_a_stronger_second_harmonic_and_dc() {
        let rate = 48_000;
        let mut samples = tone(rate, 150.0, 0.18, 1);
        for (index, sample) in samples.iter_mut().enumerate() {
            *sample += 0.3 * (TAU * 300.0 * index as f32 / rate as f32).sin() + 0.2;
        }
        let original = samples.clone();
        assert!((estimate_pitch(&samples, rate, 1).unwrap() - 150.0).abs() < 2.0);
        assert_eq!(samples, original, "analysis must preserve archived PCM");
    }

    #[test]
    fn pitch_rejects_silence_noise_invalid_and_insufficient_input() {
        assert_eq!(estimate_pitch(&[0.0; 4096], 48_000, 1), None);
        assert_eq!(estimate_pitch(&[0.5; 4096], 48_000, 1), None);
        assert_eq!(estimate_pitch(&[f32::NAN; 4096], 48_000, 1), None);
        assert_eq!(estimate_pitch(&[f32::INFINITY; 4096], 48_000, 1), None);
        assert_eq!(estimate_pitch(&[0.5; 80], 48_000, 1), None);
        assert_eq!(estimate_pitch(&[], 0, 0), None);
        for hz in [40.0, 800.0, 1000.0] {
            assert_eq!(estimate_pitch(&tone(48_000, hz, 0.2, 1), 48_000, 1), None);
        }
        assert_eq!(
            estimate_pitch(&tone(48_000, 220.0, 0.0001, 1), 48_000, 1),
            None
        );
        let mut seed = 41_u32;
        let noise: Vec<_> = (0..4800)
            .map(|_| {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (seed as f64 / u32::MAX as f64 - 0.5) as f32
            })
            .collect();
        assert_eq!(estimate_pitch(&noise, 48_000, 1), None);
    }
}
