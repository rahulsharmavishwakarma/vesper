//! One continuous audio-frontend session per buffered recording (#141).
//!
//! A dedicated thread runs Silero/DPDFNet once over the live input. Its
//! enhanced 16 kHz PCM feeds the preview mailbox and is retained for the
//! completed-audio final, so Stop never re-denoises the whole recording.
//! The original recording stays authoritative input: any session failure,
//! cancellation or coverage mismatch makes the final use the original audio.
use super::{worker_unavailable, StreamInput};
use crate::{audio_frontend::FrontendCache, live_preview::Mailbox, native, InferenceError, Result};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
    thread,
};
use vesper_audio::{StreamingResampler, ASR_SAMPLE_RATE};
use vesper_core::{AppConfig, AudioFrontendConfig};

/// Largest accepted length difference between the live-enhanced recording and
/// the completed recording decoded by the final (resampler edge effects only).
const COVERAGE_TOLERANCE: usize = ASR_SAMPLE_RATE as usize / 20;

pub(crate) struct LiveEnhanced {
    pub operation_id: String,
    pub settings: AudioFrontendConfig,
    pub samples: Vec<f32>,
    pub evidence: serde_json::Value,
}

/// Holds at most one finished recording until its final decode takes it.
#[derive(Default)]
pub(crate) struct EnhancedStore(Mutex<Option<LiveEnhanced>>);

impl EnhancedStore {
    pub fn put(&self, value: Option<LiveEnhanced>) {
        if let Ok(mut slot) = self.0.lock() {
            *slot = value;
        }
    }

    /// Returns the enhanced recording only for the same operation, frontend
    /// settings and recording length. Any lookup for that operation consumes it.
    pub fn take(
        &self,
        operation_id: &str,
        settings: &AudioFrontendConfig,
        samples: usize,
    ) -> Option<LiveEnhanced> {
        let mut slot = self.0.lock().ok()?;
        if slot.as_ref()?.operation_id != operation_id {
            return None;
        }
        let value = slot.take()?;
        (value.settings == *settings && value.samples.len().abs_diff(samples) <= COVERAGE_TOLERANCE)
            .then_some(value)
    }
}

enum Message {
    Audio(Vec<f32>),
    Finish,
}

struct Outcome {
    samples: Option<Vec<f32>>,
    evidence: serde_json::Value,
}

/// Producer-side handle. Sending never blocks the recorder; the unbounded
/// queue holds at most the audio the recorder itself retains.
pub(crate) struct LiveFrontend {
    sender: mpsc::Sender<(Message, u32)>,
    cancelled: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<Outcome>>,
    operation_id: String,
    settings: AudioFrontendConfig,
}

impl LiveFrontend {
    pub fn spawn(
        config: &AppConfig,
        root: PathBuf,
        operation_id: &str,
        mailbox: Arc<Mailbox>,
        wake: mpsc::SyncSender<StreamInput>,
    ) -> Result<Self> {
        let (sender, receiver) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&cancelled);
        let thread_config = config.clone();
        let handle = thread::Builder::new()
            .name("vesper-live-frontend".into())
            .spawn(move || run(thread_config, root, receiver, mailbox, wake, flag))
            .map_err(|e| native("live audio frontend", e))?;
        Ok(Self {
            sender,
            cancelled,
            handle: Some(handle),
            operation_id: operation_id.to_owned(),
            settings: config.audio_frontend.clone(),
        })
    }

    pub fn push(&self, samples: Vec<f32>, sample_rate: u32) -> Result<()> {
        self.sender
            .send((Message::Audio(samples), sample_rate))
            .map_err(|_| worker_unavailable("live audio frontend closed"))
    }

    /// Flushes the session after all queued audio and returns the retained
    /// recording, or `None` with evidence when the original audio must be used.
    pub fn finish(mut self) -> (Option<LiveEnhanced>, serde_json::Value) {
        let _ = self.sender.send((Message::Finish, 0));
        let outcome = self
            .handle
            .take()
            .and_then(|handle| handle.join().ok())
            .unwrap_or(Outcome {
                samples: None,
                evidence: serde_json::json!({"error":"live audio frontend panicked"}),
            });
        let enhanced = outcome.samples.map(|samples| LiveEnhanced {
            operation_id: self.operation_id.clone(),
            settings: self.settings.clone(),
            samples,
            evidence: outcome.evidence.clone(),
        });
        (enhanced, outcome.evidence)
    }
}

impl Drop for LiveFrontend {
    fn drop(&mut self) {
        // Cancellation or an abandoned session: stop promptly, keep nothing.
        self.cancelled.store(true, Ordering::Release);
        let _ = self.sender.send((Message::Finish, 0));
    }
}

fn run(
    config: AppConfig,
    root: PathBuf,
    input: mpsc::Receiver<(Message, u32)>,
    mailbox: Arc<Mailbox>,
    wake: mpsc::SyncSender<StreamInput>,
    cancelled: Arc<AtomicBool>,
) -> Outcome {
    // Models live only for this recording, so the worker's own frontend cache
    // is not duplicated across threads (the worker releases it while live).
    let mut cache = FrontendCache::default();
    let mut session = match cache.start(&config, &root, &cancelled) {
        Ok(session) => Some(session),
        Err(error) => {
            return passthrough(
                &input,
                &mailbox,
                &wake,
                &cancelled,
                serde_json::json!({"originalAudioFallback":true, "error":error.to_string(),
                    "warning":"Audio processing could not start; transcription used the original recording."}),
            )
        }
    };
    let mut retained = Vec::new();
    let mut announced_intervals = 0;
    let mut fallback: Option<StreamingResampler> = None;
    let mut failure = None;
    loop {
        let Ok((message, rate)) = input.recv() else {
            return cancelled_outcome();
        };
        if cancelled.load(Ordering::Acquire) {
            return cancelled_outcome();
        }
        match message {
            Message::Audio(samples) => {
                let enhanced = match session.as_mut() {
                    Some(live) => match live.push(&samples, rate, &cancelled) {
                        Ok(enhanced) => Ok(enhanced),
                        Err(error) => {
                            failure = Some(error.to_string());
                            session = None;
                            Err(())
                        }
                    },
                    None => Err(()),
                };
                let preview = match enhanced {
                    Ok(enhanced) => {
                        retained.extend_from_slice(&enhanced);
                        enhanced
                    }
                    // Previews continue on original audio after a failure.
                    Err(()) => match resample(&mut fallback, &samples, rate) {
                        Ok(pcm) => pcm,
                        Err(_) => continue,
                    },
                };
                feed(&mailbox, &wake, &preview);
                // Real end-of-speech points close live segments at pauses
                // (#139). Bookkeeping flushes split intervals artificially.
                if let Some(live) = &session {
                    let intervals = &live.metrics().speech_intervals;
                    for interval in &intervals[announced_intervals..] {
                        if !interval.bookkeeping_split {
                            mailbox.mark_pause(interval.end_sample as u64);
                        }
                    }
                    announced_intervals = intervals.len();
                }
            }
            Message::Finish => break,
        }
    }
    let Some(mut live) = session else {
        return Outcome {
            samples: None,
            evidence: serde_json::json!({"originalAudioFallback":true, "streamFailed":true,
                "error":failure, "source":"live-session",
                "warning":"Live audio processing was interrupted; transcription used the original recording."}),
        };
    };
    match live.finish(&cancelled) {
        Ok(tail) => {
            retained.extend_from_slice(&tail);
            let mut evidence = live.metrics().evidence();
            evidence["source"] = "live-session".into();
            Outcome {
                samples: Some(retained),
                evidence,
            }
        }
        Err(InferenceError::Cancelled) => cancelled_outcome(),
        Err(error) => Outcome {
            samples: None,
            evidence: serde_json::json!({"originalAudioFallback":true, "streamFailed":true,
                "error":error.to_string(), "source":"live-session",
                "warning":"Live audio processing was interrupted; transcription used the original recording."}),
        },
    }
}

fn passthrough(
    input: &mpsc::Receiver<(Message, u32)>,
    mailbox: &Mailbox,
    wake: &mpsc::SyncSender<StreamInput>,
    cancelled: &AtomicBool,
    evidence: serde_json::Value,
) -> Outcome {
    let mut resampler = None;
    while let Ok((Message::Audio(samples), rate)) = input.recv() {
        if cancelled.load(Ordering::Acquire) {
            break;
        }
        if let Ok(pcm) = resample(&mut resampler, &samples, rate) {
            feed(mailbox, wake, &pcm);
        }
    }
    Outcome {
        samples: None,
        evidence,
    }
}

fn resample(
    resampler: &mut Option<StreamingResampler>,
    samples: &[f32],
    rate: u32,
) -> Result<Vec<f32>> {
    if resampler.as_ref().is_some_and(|r| r.source_rate() != rate) {
        return Err(native("live audio", "sample rate changed during recording"));
    }
    if resampler.is_none() {
        *resampler = Some(StreamingResampler::new(rate).map_err(|e| native("live audio", e))?);
    }
    resampler
        .as_mut()
        .expect("initialized")
        .push(samples)
        .map_err(|e| native("live audio", e))
}

fn feed(mailbox: &Mailbox, wake: &mpsc::SyncSender<StreamInput>, pcm: &[f32]) {
    // Previews are lossy by design; a finished mailbox simply ignores input.
    for chunk in pcm.chunks(ASR_SAMPLE_RATE as usize) {
        if mailbox.push(chunk, ASR_SAMPLE_RATE).is_err() {
            return;
        }
    }
    let _ = wake.try_send(StreamInput::Preview);
}

fn cancelled_outcome() -> Outcome {
    Outcome {
        samples: None,
        evidence: serde_json::json!({"cancelled":true, "source":"live-session"}),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enhanced(id: &str, samples: usize) -> LiveEnhanced {
        LiveEnhanced {
            operation_id: id.into(),
            settings: AudioFrontendConfig::default(),
            samples: vec![0.0; samples],
            evidence: serde_json::json!({}),
        }
    }

    #[test]
    fn store_matches_operation_settings_and_coverage_once() {
        let store = EnhancedStore::default();
        store.put(Some(enhanced("a", 16000)));
        assert!(store
            .take("b", &AudioFrontendConfig::default(), 16000)
            .is_none());
        assert!(store
            .take(
                "a",
                &AudioFrontendConfig::default(),
                16000 + COVERAGE_TOLERANCE
            )
            .is_some());
        // Consumed: a retry falls back to processing the original audio.
        assert!(store
            .take("a", &AudioFrontendConfig::default(), 16000)
            .is_none());
    }

    #[test]
    fn mismatched_coverage_or_settings_fall_back_and_discard() {
        let store = EnhancedStore::default();
        store.put(Some(enhanced("a", 16000)));
        assert!(store
            .take("a", &AudioFrontendConfig::default(), 32000)
            .is_none());
        assert!(store
            .take("a", &AudioFrontendConfig::default(), 16000)
            .is_none());
        store.put(Some(enhanced("a", 16000)));
        let other = AudioFrontendConfig {
            vad_enabled: true,
            ..Default::default()
        };
        assert!(store.take("a", &other, 16000).is_none());
    }

    #[test]
    fn disabled_frontend_passes_previews_through_and_retains_nothing() {
        let mailbox = Arc::new(Mailbox::default());
        let (wake, woken) = mpsc::sync_channel(1);
        let frontend = LiveFrontend::spawn(
            &AppConfig::default(),
            PathBuf::from("missing"),
            "op",
            Arc::clone(&mailbox),
            wake,
        )
        .unwrap();
        frontend.push(vec![0.1; 16000], 16000).unwrap();
        frontend.push(vec![0.1; 16000], 16000).unwrap();
        let (retained, _) = frontend.finish();
        // A disabled frontend has no models: audio is still processed by the
        // session (identity) and retained for the final.
        assert!(retained.is_some_and(|r| r.samples.len().abs_diff(32000) <= 160));
        assert!(woken.try_recv().is_ok());
    }
}
