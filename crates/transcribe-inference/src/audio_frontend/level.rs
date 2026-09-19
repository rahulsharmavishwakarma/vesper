//! Experimental causal level conditioning. The thresholds below are engineering
//! hypotheses, not transferred WebRTC defaults or trained model guarantees.
//! One state per recording; callers supply deterministic 512-sample frames.
use serde::Serialize;

pub(super) const FRAME: usize = 512;
const RATE: f32 = 16_000.0;
const TARGET_DB: f32 = -32.0;
const MAX_GAIN_DB: f32 = 30.0;
const NOISE_CEILING_DB: f32 = -50.0;
const MARGIN_DB: f32 = 6.0;
const PEAK_LIMIT: f32 = 0.95;
const SILENCE_DB: f32 = -120.0;

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LevelMetrics {
    pub frames: usize,
    pub speech_frames: usize,
    pub noise_limited_frames: usize,
    pub peak_limited_frames: usize,
    pub increased_frames: usize,
    pub input_peak: f32,
    pub output_peak: f32,
    pub max_gain_db: f32,
    pub max_analysis_gain_db: f32,
    pub estimated_noise_dbfs: Option<f32>,
    pub first_speech_sample: Option<usize>,
    pub first_gain_sample: Option<usize>,
    pub samples: usize,
}

#[derive(Default)]
pub(super) struct LevelController {
    analysis_gain_db: Option<f32>,
    gain_db: f32,
    noise_db: Option<f32>,
    speech_db: Option<f32>,
    non_speech_samples: usize,
    pub metrics: LevelMetrics,
}

fn level(samples: &[f32]) -> (f32, f32) {
    let power =
        samples.iter().map(|&x| f64::from(x).powi(2)).sum::<f64>() / samples.len().max(1) as f64;
    let db = (10.0 * power.max(1e-12).log10()) as f32;
    let peak = samples.iter().fold(0.0_f32, |a, &x| a.max(x.abs()));
    (db, peak)
}

fn amplitude(db: f32) -> f32 {
    10.0_f32.powf(db / 20.0)
}

fn peak_gain_db(peak: f32) -> f32 {
    20.0 * (PEAK_LIMIT / peak.max(1e-12)).log10()
}

fn ramp(samples: &[f32], start_db: f32, end_db: f32) -> Vec<f32> {
    let start = amplitude(start_db);
    let end = amplitude(end_db);
    samples
        .iter()
        .enumerate()
        .map(|(i, &x)| x * (start + (end - start) * (i + 1) as f32 / samples.len() as f32))
        .collect()
}

impl LevelController {
    /// Analysis-only continuous gain; these samples never reach ASR or the
    /// original recording. This changes Silero's input distribution and is
    /// therefore part of the experimental policy, not a noise classifier.
    pub fn detection_input(&mut self, samples: &[f32]) -> Vec<f32> {
        let (db, peak) = level(samples);
        if db <= SILENCE_DB {
            return samples.to_vec();
        }
        let cap = peak_gain_db(peak).min(60.0);
        let target = (-26.0 - db).clamp(0.0, 60.0).min(cap);
        // At the first non-silent frame there is no preceding nonzero envelope.
        let start = self.analysis_gain_db.unwrap_or(target).min(cap);
        let step = 96.0 * samples.len() as f32 / RATE;
        let end = target.clamp(start - step, start + step).min(cap);
        self.analysis_gain_db = Some(end);
        self.metrics.max_analysis_gain_db = self.metrics.max_analysis_gain_db.max(start.max(end));
        ramp(samples, start, end)
    }

    /// `speech` is the native hysteretic detected() state, not a probability.
    /// Estimates always use original PCM, never the normalized detector input.
    pub fn apply(&mut self, samples: &[f32], speech: bool) -> Vec<f32> {
        if samples.is_empty() {
            return Vec::new();
        }
        let (db, peak) = level(samples);
        let seconds = samples.len() as f32 / RATE;
        self.metrics.frames += 1;
        self.metrics.input_peak = self.metrics.input_peak.max(peak);
        if speech {
            self.metrics.speech_frames += 1;
            self.metrics
                .first_speech_sample
                .get_or_insert(self.metrics.samples);
            self.non_speech_samples = 0;
        } else {
            self.non_speech_samples += samples.len();
            if self.non_speech_samples >= 8000 {
                self.speech_db = None;
            }
            // Early false states include Silero's speech-confirmation delay.
            // Do not call the first 160 ms of a recording/background gap noise.
            if db > SILENCE_DB && self.non_speech_samples >= 5 * FRAME {
                // Slow upward tracking avoids immediately labelling a speech
                // onset as noise; faster downward tracking follows quieter gaps.
                self.noise_db = Some(match self.noise_db {
                    None => db,
                    Some(previous) => {
                        let alpha = if db < previous { 0.2 } else { 0.01 };
                        previous + alpha * (db - previous)
                    }
                });
            }
        }
        let above_noise = self.noise_db.is_none_or(|noise| db >= noise + MARGIN_DB);
        let mut requested = 0.0;
        if speech && above_noise && db > SILENCE_DB {
            // Hold rather than drop the existing gain between voiced syllables.
            // Never increase it merely because the instantaneous level fell.
            requested = self.gain_db.max(0.0);
            // A held speech level prevents chasing falling tails during VAD's
            // silence hangover. New utterances reset after a non-speech gap.
            let speech_db = self.speech_db.map_or(db, |old| db.max(old - 6.0 * seconds));
            self.speech_db = Some(speech_db);
            if db >= speech_db - MARGIN_DB {
                requested = (TARGET_DB - speech_db).clamp(0.0, MAX_GAIN_DB);
            }
        }
        let noise_cap = self.noise_db.map_or(MAX_GAIN_DB, |noise| {
            (NOISE_CEILING_DB - noise).clamp(0.0, MAX_GAIN_DB)
        });
        if requested > noise_cap {
            self.metrics.noise_limited_frames += 1;
        }
        let cap = peak_gain_db(peak).min(noise_cap);
        if peak_gain_db(peak) < self.gain_db.max(requested.min(noise_cap)) {
            self.metrics.peak_limited_frames += 1;
        }
        // Current-frame headroom always wins, including on abrupt loud input.
        let start = self.gain_db.min(cap);
        let target = requested.min(cap);
        let speed = if target > start { 30.0 } else { 120.0 };
        let end = target
            .clamp(start - speed * seconds, start + speed * seconds)
            .min(cap);
        if end > start {
            self.metrics.increased_frames += 1;
        }
        if start.max(end) > 0.01 {
            self.metrics
                .first_gain_sample
                .get_or_insert(self.metrics.samples);
        }
        let output = ramp(samples, start, end);
        self.gain_db = end;
        self.metrics.max_gain_db = self.metrics.max_gain_db.max(start.max(end));
        self.metrics.output_peak = self.metrics.output_peak.max(level(&output).1);
        self.metrics.estimated_noise_dbfs = self.noise_db;
        self.metrics.samples += samples.len();
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(amplitude: f32) -> Vec<f32> {
        (0..FRAME)
            .map(|i| amplitude * (i as f32 * 0.13).sin())
            .collect()
    }

    #[test]
    fn silence_and_unconfirmed_noise_never_gain() {
        let mut controller = LevelController::default();
        for amplitude in [0.0, 0.00001, 0.01, 0.1] {
            for _ in 0..100 {
                let input = tone(amplitude);
                controller.detection_input(&input);
                assert_eq!(controller.apply(&input, false), input);
            }
        }
        assert_eq!(controller.metrics.max_gain_db, 0.0);
    }

    #[test]
    fn speech_gain_is_bounded_slewed_and_reacts_to_loud_onsets() {
        let mut controller = LevelController::default();
        for _ in 0..100 {
            controller.apply(&tone(0.00001), true);
        }
        assert!((controller.metrics.max_gain_db - MAX_GAIN_DB).abs() < 1e-5);
        let input = vec![1.0; FRAME];
        let output = controller.apply(&input, true);
        assert!(output.iter().all(|x| x.abs() <= PEAK_LIMIT + 1e-6));
        assert!(controller.metrics.peak_limited_frames > 0);
    }

    #[test]
    fn stationary_noise_false_detection_cannot_raise_gain() {
        let mut controller = LevelController::default();
        let noise = tone(0.001);
        for _ in 0..10 {
            controller.apply(&noise, false);
        }
        for _ in 0..100 {
            assert_eq!(controller.apply(&noise, true), noise);
        }
        assert_eq!(controller.metrics.increased_frames, 0);
    }

    #[test]
    fn hangover_does_not_chase_falling_tail() {
        let mut controller = LevelController::default();
        for _ in 0..50 {
            controller.apply(&tone(0.005), true);
        }
        let previous = controller.gain_db;
        controller.apply(&tone(0.00001), true);
        assert!(controller.gain_db <= previous);
    }

    #[test]
    fn detector_gain_is_separate_bounded_and_leaves_original_unchanged() {
        let mut controller = LevelController::default();
        let input = tone(0.00001);
        let copy = input.clone();
        let detection = controller.detection_input(&input);
        assert!(level(&detection).0 > level(&input).0 + 40.0);
        assert!(controller.metrics.max_analysis_gain_db <= 60.0);
        assert_eq!(controller.apply(&input, false), copy);
        assert_eq!(input, copy);
    }
}
