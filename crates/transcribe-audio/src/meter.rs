/// Recent 40 ms RMS, independent of the device's callback buffer length.
/// Storage is allocated before capture; `push` performs no allocation or locking.
pub(crate) struct RmsMeter {
    squares: Vec<f64>,
    position: usize,
    count: usize,
    sum: f64,
}

impl RmsMeter {
    pub(crate) fn new(sample_rate: u32, channels: u16) -> Self {
        let samples = (sample_rate as usize * channels as usize * 40 / 1000).max(1);
        Self {
            squares: vec![0.0; samples],
            position: 0,
            count: 0,
            sum: 0.0,
        }
    }

    pub(crate) fn push(&mut self, sample: f32) {
        // Clamp the display measurement, never the stored audio.
        let amplitude = f64::from(sample.clamp(-1.0, 1.0));
        let square = amplitude * amplitude;
        self.sum += square - self.squares[self.position];
        self.squares[self.position] = square;
        self.position = (self.position + 1) % self.squares.len();
        self.count = (self.count + 1).min(self.squares.len());
    }

    pub(crate) fn level(&self) -> f32 {
        if self.count == 0 {
            return 0.0;
        }
        (self.sum.max(0.0) / self.count as f64).sqrt() as f32
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
                        let expected = (recent.iter().map(|&x| f64::from(x).powi(2)).sum::<f64>()
                            / recent.len() as f64)
                            .sqrt();
                        assert!((f64::from(meter.level()) - expected).abs() < 1e-6, "rate={rate}, channels={channels}, callback={callback_frames}, end={end}");
                    }
                }
            }
        }
    }

    #[test]
    fn quiet_after_sound_clears_within_one_window_and_a_new_meter_is_empty() {
        let mut meter = RmsMeter::new(48_000, 1);
        for _ in 0..1920 {
            meter.push(0.5);
        }
        assert_eq!(meter.level(), 0.5);
        for _ in 0..1920 {
            meter.push(0.0);
        }
        assert_eq!(meter.level(), 0.0);
        assert_eq!(RmsMeter::new(48_000, 1).level(), 0.0);
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
