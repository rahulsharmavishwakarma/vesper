/// Recent 40 ms AC RMS, independent of device callback size or per-channel DC.
/// Storage is allocated before capture; `push` performs no allocation or locking.
pub(crate) struct RmsMeter {
    samples: Vec<f64>,
    sums: Vec<f64>,
    squares: Vec<f64>,
    counts: Vec<usize>,
    position: usize,
}

impl RmsMeter {
    pub(crate) fn new(sample_rate: u32, channels: u16) -> Self {
        let channels = usize::from(channels).max(1);
        let frames = (sample_rate as usize * 40 / 1000).max(1);
        Self {
            samples: vec![0.0; frames * channels],
            sums: vec![0.0; channels],
            squares: vec![0.0; channels],
            counts: vec![0; channels],
            position: 0,
        }
    }

    pub(crate) fn push(&mut self, sample: f32) {
        // Clamp the display measurement, never the stored audio.
        let amplitude = f64::from(sample.clamp(-1.0, 1.0));
        let channel = self.position % self.sums.len();
        let previous = self.samples[self.position];
        self.sums[channel] += amplitude - previous;
        self.squares[channel] += amplitude * amplitude - previous * previous;
        self.counts[channel] = (self.counts[channel] + 1).min(self.samples.len() / self.sums.len());
        self.samples[self.position] = amplitude;
        self.position = (self.position + 1) % self.samples.len();
    }

    pub(crate) fn level(&self) -> f32 {
        let count: usize = self.counts.iter().sum();
        if count == 0 {
            return 0.0;
        }
        // Subtract each channel's mean energy independently. Opposite stereo
        // offsets must not cancel their means while retaining fake AC energy.
        let energy: f64 = self
            .counts
            .iter()
            .enumerate()
            .filter(|(_, n)| **n > 0)
            .map(|(channel, n)| {
                (self.squares[channel] - self.sums[channel].powi(2) / *n as f64).max(0.0)
            })
            .sum();
        (energy / count as f64).sqrt().min(1.0) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn callback_partition_does_not_change_the_recent_energy() {
        for rate in [16_000, 44_100, 48_000] {
            for channels in [1, 2] {
                let samples: Vec<f32> = (0..rate * 2)
                    .flat_map(|frame| {
                        // Includes silence, a brief pulse, steady voice-range tone,
                        // and a level change. Stereo exercises interleaved input.
                        let time = frame as f32 / rate as f32;
                        let amplitude = if time < 0.2 {
                            0.0
                        } else if time < 0.21 {
                            0.8
                        } else if time < 1.0 {
                            0.025
                        } else {
                            0.1
                        };
                        let sample = amplitude * (time * 110.0 * std::f32::consts::TAU).sin();
                        (0..channels)
                            .map(move |channel| if channel == 0 { sample } else { sample * 0.5 })
                    })
                    .collect();
                let window = rate as usize * channels as usize * 40 / 1000;
                for callback_frames in [64, 128, 256, 512, 1024] {
                    let mut meter = RmsMeter::new(rate, channels);
                    let mut end = 0;
                    for block in samples.chunks(callback_frames * channels as usize) {
                        for &sample in block {
                            meter.push(sample);
                        }
                        end += block.len();
                        let recent = &samples[end.saturating_sub(window)..end];
                        let energy: f64 = (0..channels as usize)
                            .map(|channel| {
                                let values: Vec<_> = recent
                                    .iter()
                                    .skip(channel)
                                    .step_by(channels as usize)
                                    .map(|&x| f64::from(x))
                                    .collect();
                                let mean = values.iter().sum::<f64>() / values.len() as f64;
                                values.iter().map(|x| (x - mean).powi(2)).sum::<f64>()
                            })
                            .sum();
                        let expected = (energy / recent.len() as f64).sqrt();
                        assert!((f64::from(meter.level()) - expected).abs() < 1e-6, "rate={rate}, channels={channels}, callback={callback_frames}, end={end}");
                    }
                }
            }
        }
    }

    #[test]
    fn quiet_after_sound_clears_within_one_window_and_a_new_meter_is_empty() {
        let mut meter = RmsMeter::new(48_000, 1);
        for i in 0..1920 {
            meter.push(if i % 2 == 0 { 0.5 } else { -0.5 });
        }
        assert_eq!(meter.level(), 0.5);
        for _ in 0..1920 {
            meter.push(0.0);
        }
        assert_eq!(meter.level(), 0.0);
        assert_eq!(RmsMeter::new(48_000, 1).level(), 0.0);
    }

    #[test]
    fn dc_bias_is_not_sound_and_does_not_amplify_real_audio() {
        for rate in [16_000, 44_100, 48_000] {
            for channels in [1, 2] {
                for amplitude in [0.0, 0.002, 0.02, 0.4] {
                    let mut clean = RmsMeter::new(rate, channels);
                    let mut biased = RmsMeter::new(rate, channels);
                    for frame in 0..rate / 5 {
                        let tone = amplitude
                            * (frame as f32 * 250.0 * std::f32::consts::TAU / rate as f32).sin();
                        for channel in 0..channels {
                            clean.push(tone);
                            biased.push(tone + if channel == 0 { 0.2 } else { -0.2 });
                        }
                    }
                    assert!((biased.level() - clean.level()).abs() < 1e-6);
                    assert!((clean.level() - amplitude / 2.0_f32.sqrt()).abs() < 1e-6);
                }
            }
        }
    }

    #[test]
    fn steady_tone_does_not_pulse_with_small_device_buffers() {
        let mut meter = RmsMeter::new(48_000, 1);
        let mut levels = Vec::new();
        for frame in 0..48_000 {
            meter.push(0.025 * (frame as f32 * 110.0 * std::f32::consts::TAU / 48_000.0).sin());
            if frame >= 1920 && frame % 64 == 63 {
                levels.push(meter.level());
            }
        }
        let low = levels.iter().copied().fold(f32::INFINITY, f32::min);
        let high = levels.iter().copied().fold(0.0, f32::max);
        // Latest-64-sample RMS varies by ~100% of true RMS on this fixture.
        assert!((high - low) / (0.025 / 2.0_f32.sqrt()) < 0.025);
    }
}
