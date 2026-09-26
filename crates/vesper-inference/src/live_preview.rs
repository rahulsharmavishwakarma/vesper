//! Bounded preview transport. The recorder, not this lossy preview mailbox,
//! retains authoritative audio. No model work runs on the producer.
use crate::{native, Result};
use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
};
use vesper_audio::{MonoPcm, StreamingResampler, ASR_SAMPLE_RATE};

const WINDOW: usize = 8 * ASR_SAMPLE_RATE as usize;
const MINIMUM: usize = 2 * ASR_SAMPLE_RATE as usize;

#[derive(Default)]
struct Window {
    index: u64,
    samples: Vec<f32>,
}

#[derive(Default)]
struct Input {
    converter: Option<StreamingResampler>,
    rate: Option<u32>,
    windows: VecDeque<Window>,
    total: u64,
    finished: bool,
}

#[derive(Default)]
pub(crate) struct Mailbox {
    input: Mutex<Input>,
    pub cancelled: AtomicBool,
}

pub(crate) struct Snapshot {
    pub index: u64,
    pub end_sample: u64,
    pub audio: MonoPcm,
}

impl Mailbox {
    pub fn push(&self, samples: &[f32], rate: u32) -> Result<()> {
        if samples.iter().any(|s| !s.is_finite()) || samples.len() > rate as usize {
            return Err(native(
                "live audio",
                "expected at most one second of finite PCM",
            ));
        }
        let mut input = self
            .input
            .lock()
            .map_err(|_| native("live audio", "mailbox unavailable"))?;
        if input.finished {
            return Err(native("live audio", "input is already finished"));
        }
        if input.rate.is_some_and(|previous| previous != rate) {
            return Err(native("live audio", "sample rate changed during recording"));
        }
        if input.converter.is_none() {
            input.converter = Some(StreamingResampler::new(rate)?);
            input.rate = Some(rate);
        }
        let converted = input
            .converter
            .as_mut()
            .expect("initialized resampler")
            .push(samples)?;
        append(&mut input, &converted);
        Ok(())
    }

    pub fn finish(&self) {
        self.cancelled.store(true, Ordering::Release);
        if let Ok(mut input) = self.input.lock() {
            input.finished = true;
        }
    }

    pub fn snapshot(&self, after: u64) -> Result<Option<Snapshot>> {
        let input = self
            .input
            .lock()
            .map_err(|_| native("live audio", "mailbox unavailable"))?;
        if input.finished {
            return Ok(None);
        }
        Ok(input
            .windows
            .iter()
            .find(|window| {
                window.samples.len() >= MINIMUM
                    && window.index * WINDOW as u64 + window.samples.len() as u64 > after
            })
            .map(|window| Snapshot {
                index: window.index,
                end_sample: window.index * WINDOW as u64 + window.samples.len() as u64,
                audio: MonoPcm::new(window.samples.clone(), ASR_SAMPLE_RATE)
                    .expect("validated PCM"),
            }))
    }

    pub fn retained_samples(&self) -> usize {
        self.input.lock().map_or(0, |input| {
            input.windows.iter().map(|w| w.samples.len()).sum()
        })
    }
}

fn append(input: &mut Input, mut samples: &[f32]) {
    while !samples.is_empty() {
        if input
            .windows
            .back()
            .is_none_or(|w| w.samples.len() == WINDOW)
        {
            input.windows.push_back(Window {
                index: input.total / WINDOW as u64,
                samples: Vec::with_capacity(WINDOW),
            });
            while input.windows.len() > 2 {
                input.windows.pop_front();
            }
        }
        let window = input.windows.back_mut().expect("current window");
        let count = samples.len().min(WINDOW - window.samples.len());
        window.samples.extend_from_slice(&samples[..count]);
        input.total += count as u64;
        samples = &samples[count..];
    }
}

#[derive(Default)]
pub(crate) struct PreviewText {
    prefix: String,
    current: String,
    index: Option<u64>,
}

impl PreviewText {
    pub fn replace(&mut self, index: u64, text: String) -> String {
        if self.index.is_some_and(|old| old != index) {
            join(&mut self.prefix, &self.current);
        }
        self.index = Some(index);
        self.current = text;
        let mut snapshot = self.prefix.clone();
        join(&mut snapshot, &self.current);
        snapshot
    }
}

fn join(left: &mut String, right: &str) {
    if right.trim().is_empty() {
        return;
    }
    if !left.is_empty() {
        left.push(' ');
    }
    left.push_str(right.trim());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn producer_remains_bounded_when_decoder_never_consumes() {
        let mailbox = Mailbox::default();
        for _ in 0..120 {
            mailbox.push(&vec![0.25; 16000], 16000).unwrap();
        }
        assert!(mailbox.retained_samples() <= 2 * WINDOW);
        let latest = mailbox.snapshot(0).unwrap().unwrap();
        assert!(latest.index >= 13);
        assert!(latest.audio.samples().iter().all(|s| *s == 0.25));
        mailbox.finish();
        assert!(mailbox.cancelled.load(Ordering::Acquire));
        assert!(mailbox.snapshot(0).unwrap().is_none());
        assert!(mailbox.push(&[0.0], 16000).is_err());
    }

    #[test]
    fn revisions_replace_without_deduplicating_real_repetitions() {
        let mut text = PreviewText::default();
        assert_eq!(text.replace(0, "go".into()), "go");
        assert_eq!(text.replace(0, "go go".into()), "go go");
        assert_eq!(text.replace(1, "go again".into()), "go go go again");
        assert_eq!(
            text.replace(1, "go again please".into()),
            "go go go again please"
        );
    }

    #[test]
    fn capture_packet_boundaries_preserve_window_samples() {
        let mailbox = Mailbox::default();
        for chunk in vec![0.5; WINDOW + MINIMUM].chunks(317) {
            mailbox.push(chunk, 16000).unwrap();
        }
        let first = mailbox.snapshot(0).unwrap().unwrap();
        assert_eq!(first.end_sample, WINDOW as u64);
        let second = mailbox.snapshot(first.end_sample).unwrap().unwrap();
        assert_eq!(second.audio.samples().len(), MINIMUM);
        assert!(mailbox.push(&[f32::NAN], 16000).is_err());
        assert!(mailbox.push(&[0.0], 48000).is_err());
    }
}
