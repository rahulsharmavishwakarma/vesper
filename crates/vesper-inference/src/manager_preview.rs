use super::*;
use crate::{
    live_preview::{JobKind, LiveText, Mailbox, MAX_RETAINED_SAMPLES},
    resources,
};
use std::time::{Duration, Instant};

const MAX_PREVIEW_RELOADS: usize = 1;

#[allow(clippy::too_many_arguments)]
pub(super) fn run_buffered(
    state: &mut WorkerState,
    config: &AppConfig,
    root: &Path,
    id: &str,
    cancelled: &AtomicBool,
    stream_cancelled: &AtomicBool,
    updates: &StreamingUpdateSink,
    profile: &PersonalizationProfile,
    evidence: &Mutex<serde_json::Value>,
    input: mpsc::Receiver<StreamInput>,
    started: mpsc::SyncSender<Result<bool>>,
    mailbox: Arc<Mailbox>,
) -> Result<StreamingFinal> {
    let mut memory = resources::Memory::read();
    release_writer_under_pressure(state, memory.pressured(), evidence, id);
    let initialized = (|| {
        let mut selection = vesper_models::resolve_speech(config, root)
            .map_err(crate::resident_sherpa::verification_error)?;
        let mut fallback = std::mem::take(&mut state.automatic_fallback);
        let result = fallback.load(&mut selection, root, cancelled, |effective| {
            state.ensure_speech(effective, root, cancelled).map(|_| ())
        });
        state.automatic_fallback = fallback;
        result?;
        Ok(selection)
    })();
    let selection = match initialized {
        Ok(selection) => selection,
        Err(error) => {
            let _ = started.send(Err(error));
            return Err(InferenceError::Cancelled);
        }
    };
    if started.send(Ok(true)).is_err() {
        return Err(InferenceError::Cancelled);
    }
    let mut tail_after = 0;
    let mut sequence = 0;
    let mut text = LiveText::default();
    let mut committed_segments = 0;
    let mut committed_failures = 0;
    let mut tail_passes = 0;
    let mut decoded_samples = 0_u64;
    let mut processed_samples = 0_u64;
    let mut due = Instant::now();
    let mut memory_at = Instant::now();
    let mut failures = 0;
    let mut incomplete_drafts = 0;
    let mut reloads = 0;
    let mut last_error = None;
    let mut decode_ms = 0.0;
    let mut pressure_deferrals = 0;
    loop {
        check_cancelled(cancelled)?;
        check_cancelled(stream_cancelled)?;
        if mailbox.cancelled.load(Ordering::Acquire) {
            record_evidence(
                evidence,
                id,
                "live",
                serde_json::json!({
                    "strategy":"buffered", "authoritativeFinal":"completed-audio",
                    "previewPasses":sequence,"previewFailures":failures,"lastPreviewError":last_error,
                    "previewIncompleteDrafts":incomplete_drafts,"previewReloads":reloads,
                    "previewPolicy":"causal-committed-segments-v1",
                    "committedSegments":committed_segments,"committedSegmentFailures":committed_failures,
                    "provisionalPasses":tail_passes,"previewSegmentsDropped":mailbox.dropped_segments(),"pauseClosedSegments":mailbox.pause_segments(),
                    "previewDecodedAudioSeconds":decoded_samples as f64 / 16000.0,
                    "previewDecodeMs":decode_ms,
                    "pressureDeferrals":pressure_deferrals,"retainedPreviewSamples":mailbox.retained_samples(),
                    "maxPreviewSamples":MAX_RETAINED_SAMPLES,"memory":memory,
                    "previewFrontend":if mailbox.enhanced.load(Ordering::Acquire) {"live-frontend"} else {"fresh-bounded-segment"},
                    "selection":selection.evidence()
                }),
            );
            return Ok(StreamingFinal::CompletedAudioRequired);
        }
        match input.recv_timeout(Duration::from_millis(50)) {
            Ok(StreamInput::Cancel) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(InferenceError::Cancelled)
            }
            Ok(StreamInput::Finish) => {
                mailbox.finish();
                continue;
            }
            Ok(StreamInput::Preview) | Err(mpsc::RecvTimeoutError::Timeout) => {}
            Ok(StreamInput::Audio { .. }) => {
                return Err(native("live preview", "unexpected native audio transport"))
            }
        }
        if Instant::now() < due {
            continue;
        }
        if state.speech.is_none() {
            // One reload per recording after a genuine owner failure. The final
            // pass needs the owner anyway, so this moves that cost before Stop.
            if reloads >= MAX_PREVIEW_RELOADS {
                continue;
            }
            reloads += 1;
            if let Err(error) = state.ensure_speech(&selection.config, root, cancelled) {
                check_cancelled(cancelled)?;
                failures += 1;
                last_error = Some(error.to_string());
                continue;
            }
        }
        if memory_at.elapsed() >= Duration::from_secs(5) {
            memory = resources::Memory::read();
            memory_at = Instant::now();
        }
        if memory.pressured() {
            pressure_deferrals += 1;
            due = Instant::now() + Duration::from_secs(2);
            continue;
        }
        let Some(snapshot) = mailbox.snapshot(tail_after)? else {
            continue;
        };
        if snapshot.kind == JobKind::Provisional {
            tail_after = snapshot.end_sample;
        }
        decoded_samples += snapshot.audio.samples().len() as u64;
        let begin = Instant::now();
        let live_frontend = mailbox.enhanced.load(Ordering::Acquire);
        if live_frontend {
            // The recording's frontend thread owns its own models; release
            // the worker's copy instead of holding both while recording.
            state.frontend.unload();
        }
        let result = (|| {
            let processed = if live_frontend {
                None
            } else {
                prepare_audio(
                    &mut state.frontend,
                    &snapshot.audio,
                    &selection.config,
                    root,
                    id,
                    &mailbox.cancelled,
                    evidence,
                )?
            };
            let model = &state
                .speech
                .as_ref()
                .expect("initialized preview recognizer")
                .1;
            let result = model.preview(
                processed.as_ref().unwrap_or(&snapshot.audio),
                &selection.config,
                root,
                id,
                &mailbox.cancelled,
                profile,
            );
            let mut observed = model.evidence(&selection.config, profile);
            observed["selection"] = selection.evidence();
            record_evidence(evidence, id, "asr", observed);
            result
        })();
        let elapsed = begin.elapsed();
        decode_ms += elapsed.as_secs_f64() * 1000.0;
        // A closed segment is decoded once and the tail follows promptly; only
        // repeated tail passes wait for the adaptive interval.
        due = match snapshot.kind {
            JobKind::Committed => Instant::now(),
            JobKind::Provisional => {
                Instant::now() + resources::preview_delay(elapsed, memory.pressured())
            }
        };
        check_cancelled(cancelled)?;
        check_cancelled(stream_cancelled)?;
        // Stop suppresses an in-flight draft, including callbacks from a native
        // call which can only observe cancellation after it returns.
        if mailbox.cancelled.load(Ordering::Acquire) {
            continue;
        }
        let committed = snapshot.kind == JobKind::Committed;
        let value = match result {
            Ok(value) => Some(value),
            Err(InferenceError::IncompletePreview(reason)) => {
                // Keep the previous draft and the healthy owner; a later pass
                // over more audio replaces this tail.
                incomplete_drafts += 1;
                last_error = Some(reason);
                None
            }
            Err(error) => {
                failures += 1;
                last_error = Some(error.to_string());
                // Preserve recording and final retry, but never reuse an owner
                // after a failed native/completeness/identity check.
                state.speech = None;
                None
            }
        };
        match (committed, value) {
            (true, value) => {
                // A closed segment gets exactly one decode. Without usable
                // text it stays absent from the draft; the final is unaffected.
                committed_segments += 1;
                if value.is_none() {
                    committed_failures += 1;
                }
                text.commit(value.as_deref().unwrap_or_default());
            }
            (false, Some(value)) => {
                tail_passes += 1;
                text.set_tail(value);
            }
            (false, None) => continue,
        }
        sequence += 1;
        processed_samples = processed_samples.max(snapshot.end_sample);
        let (committed_text, provisional_text) = text.parts();
        updates(StreamingTranscript {
            operation_id: id.into(),
            sequence,
            committed_text,
            provisional_text,
            audio_processed_seconds: processed_samples as f32 / 16000.0,
            emitted_at_unix_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            is_final: false,
        });
    }
}
