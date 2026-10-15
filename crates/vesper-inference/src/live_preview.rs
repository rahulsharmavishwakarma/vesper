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

const SAMPLE_RATE: usize = ASR_SAMPLE_RATE as usize;
/// Open audio is closed into a committed segment once it reaches this length.
const SEGMENT_MAX: usize = 8 * SAMPLE_RATE;
/// Earliest causal boundary inside the open audio.
const SEGMENT_MIN: usize = 4 * SAMPLE_RATE;
/// Shortest open tail worth a provisional pass.
const MINIMUM: usize = 2 * SAMPLE_RATE;
/// Closed segments awaiting their single decode; older ones are dropped.
const PENDING_LIMIT: usize = 2;
const ENERGY_WINDOW: usize = SAMPLE_RATE / 10;
const ENERGY_STEP: usize = SAMPLE_RATE / 100;
pub(crate) const MAX_RETAINED_SAMPLES: usize = SEGMENT_MAX * (PENDING_LIMIT + 1);

struct Segment {
    start: u64,
    samples: Vec<f32>,
}

#[derive(Default)]
struct Input {
    converter: Option<StreamingResampler>,
    rate: Option<u32>,
    open: Vec<f32>,
    open_start: u64,
    pending: VecDeque<Segment>,
    dropped_segments: u64,
    finished: bool,
}

#[derive(Default)]
pub(crate) struct Mailbox {
    input: Mutex<Input>,
    pub cancelled: AtomicBool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum JobKind {
    /// A closed segment, decoded once; its text becomes committed.
    Committed,
    /// The still-open tail, re-decoded as more audio arrives.
    Provisional,
}

pub(crate) struct Snapshot {
    pub kind: JobKind,
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

    /// Closed segments first, in order; otherwise the open tail when it has
    /// grown past `tail_after` (the end of the last provisional pass).
    pub fn snapshot(&self, tail_after: u64) -> Result<Option<Snapshot>> {
        let mut input = self
            .input
            .lock()
            .map_err(|_| native("live audio", "mailbox unavailable"))?;
        if input.finished {
            return Ok(None);
        }
        if let Some(segment) = input.pending.pop_front() {
            return Ok(Some(Snapshot {
                kind: JobKind::Committed,
                end_sample: segment.start + segment.samples.len() as u64,
                audio: MonoPcm::new(segment.samples, ASR_SAMPLE_RATE).expect("validated PCM"),
            }));
        }
        let end = input.open_start + input.open.len() as u64;
        if input.open.len() < MINIMUM || end <= tail_after {
            return Ok(None);
        }
        Ok(Some(Snapshot {
            kind: JobKind::Provisional,
            end_sample: end,
            audio: MonoPcm::new(input.open.clone(), ASR_SAMPLE_RATE).expect("validated PCM"),
        }))
    }

    pub fn retained_samples(&self) -> usize {
        self.input.lock().map_or(0, |input| {
            input.open.len() + input.pending.iter().map(|s| s.samples.len()).sum::<usize>()
        })
    }

    pub fn dropped_segments(&self) -> u64 {
        self.input.lock().map_or(0, |input| input.dropped_segments)
    }
}

fn append(input: &mut Input, samples: &[f32]) {
    input.open.extend_from_slice(samples);
    while input.open.len() >= SEGMENT_MAX {
        let boundary = causal_boundary(&input.open);
        let samples = input.open.drain(..boundary).collect::<Vec<_>>();
        input.pending.push_back(Segment {
            start: input.open_start,
            samples,
        });
        input.open_start += boundary as u64;
        // Bounded memory: a slow decoder loses the oldest draft segment; the
        // recorder still retains that audio for the completed-audio final.
        while input.pending.len() > PENDING_LIMIT {
            input.pending.pop_front();
            input.dropped_segments += 1;
        }
    }
}

/// Centre of the quietest 100 ms window between `SEGMENT_MIN` and the end of
/// the received audio, preferring later boundaries on ties. Uses only audio
/// already received, so a closed segment never depends on future input.
/// Energy only chooses a boundary; no audio is removed.
fn causal_boundary(samples: &[f32]) -> usize {
    let half = ENERGY_WINDOW / 2;
    let mut prefix = Vec::with_capacity(samples.len() + 1);
    prefix.push(0.0_f64);
    for sample in samples {
        prefix.push(prefix.last().copied().unwrap_or_default() + f64::from(*sample).powi(2));
    }
    let last = samples.len() - half;
    let mut boundary = last;
    let mut lowest = f64::INFINITY;
    let mut center = SEGMENT_MIN;
    while center <= last {
        let energy = prefix[center + half] - prefix[center - half];
        if energy <= lowest {
            lowest = energy;
            boundary = center;
        }
        center += ENERGY_STEP;
    }
    boundary
}

/// Display text: committed segment text plus the latest provisional tail.
#[derive(Default)]
pub(crate) struct LiveText {
    committed: String,
    tail: String,
}

impl LiveText {
    /// Commit a closed segment. Any provisional text covered that segment's
    /// audio, so it is cleared until the next tail pass.
    pub fn commit(&mut self, text: &str) {
        join(&mut self.committed, text);
        self.tail.clear();
    }

    pub fn set_tail(&mut self, text: String) {
        self.tail = text.trim().to_owned();
    }

    /// `(committed, provisional)`; the provisional part carries its own
    /// leading space because the UI concatenates both fields directly.
    pub fn parts(&self) -> (String, String) {
        let provisional = if self.committed.is_empty() || self.tail.is_empty() {
            self.tail.clone()
        } else {
            format!(" {}", self.tail)
        };
        (self.committed.clone(), provisional)
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

    fn feed(mailbox: &Mailbox, samples: &[f32]) {
        for chunk in samples.chunks(317) {
            mailbox.push(chunk, 16000).unwrap();
        }
    }

    #[test]
    fn producer_remains_bounded_when_decoder_never_consumes() {
        let mailbox = Mailbox::default();
        for _ in 0..120 {
            mailbox.push(&vec![0.25; 16000], 16000).unwrap();
        }
        assert!(mailbox.retained_samples() <= MAX_RETAINED_SAMPLES);
        assert!(mailbox.dropped_segments() > 0);
        let first = mailbox.snapshot(0).unwrap().unwrap();
        assert_eq!(first.kind, JobKind::Committed);
        // Only the newest closed segments survive a stalled decoder.
        assert!(first.end_sample >= 100 * SAMPLE_RATE as u64);
        assert!(first.audio.samples().iter().all(|s| *s == 0.25));
        mailbox.finish();
        assert!(mailbox.cancelled.load(Ordering::Acquire));
        assert!(mailbox.snapshot(0).unwrap().is_none());
        assert!(mailbox.push(&[0.0], 16000).is_err());
    }

    #[test]
    fn segments_close_causally_at_the_quietest_point_and_keep_every_sample() {
        let mailbox = Mailbox::default();
        // Loud audio with one quiet 200 ms gap centred at 6 s.
        let mut audio = vec![0.5_f32; SEGMENT_MAX + MINIMUM];
        let gap = 6 * SAMPLE_RATE;
        audio[gap - 1600..gap + 1600].fill(0.0);
        feed(&mailbox, &audio);
        let committed = mailbox.snapshot(0).unwrap().unwrap();
        assert_eq!(committed.kind, JobKind::Committed);
        let length = committed.audio.samples().len();
        assert!((gap - 800..=gap + 800).contains(&length), "{length}");
        let tail = mailbox.snapshot(0).unwrap().unwrap();
        assert_eq!(tail.kind, JobKind::Provisional);
        assert_eq!(length + tail.audio.samples().len(), audio.len());
        assert_eq!(tail.end_sample, audio.len() as u64);
        // An unchanged tail is not re-decoded.
        assert!(mailbox.snapshot(tail.end_sample).unwrap().is_none());
        assert!(mailbox.push(&[f32::NAN], 16000).is_err());
        assert!(mailbox.push(&[0.0], 48000).is_err());
    }

    #[test]
    fn uniform_audio_closes_at_the_latest_boundary() {
        let mailbox = Mailbox::default();
        feed(&mailbox, &vec![0.5; SEGMENT_MAX]);
        let committed = mailbox.snapshot(0).unwrap().unwrap();
        assert_eq!(committed.kind, JobKind::Committed);
        assert!(committed.audio.samples().len() >= SEGMENT_MAX - ENERGY_WINDOW);
    }

    #[test]
    fn committed_text_never_deduplicates_and_clears_stale_tail() {
        let mut text = LiveText::default();
        text.set_tail("go".into());
        assert_eq!(text.parts(), (String::new(), "go".into()));
        text.commit("go go");
        assert_eq!(text.parts(), ("go go".into(), String::new()));
        text.set_tail("go again".into());
        assert_eq!(text.parts(), ("go go".into(), " go again".into()));
        text.commit("");
        assert_eq!(text.parts(), ("go go".into(), String::new()));
    }
}
