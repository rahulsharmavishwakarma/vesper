use std::{
    sync::{
        atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering},
        mpsc, Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    FromSample, Sample, SampleFormat, SizedSample,
};
use rtrb::{Consumer, Producer, RingBuffer};
use serde::Serialize;
use vesper_core::Result;

use crate::{
    audio_error, meter::RmsMeter, pcm::validate_format, pitch::estimate_pitch, validate_limit,
    CapturedAudio,
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioFeatures {
    pub level: f32,
    /// Reliable fundamental estimate; absent for silence or unvoiced input.
    pub pitch_hz: Option<f32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InputDevice {
    /// Enumeration-scoped identifier. Refresh the list after a device is unplugged.
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingStatus {
    pub recording: bool,
    pub limit_reached: bool,
    pub duration_seconds: f64,
    pub error: Option<String>,
}

/// Bounded, mono capture chunk for a stateful streaming recognizer.
/// The native device rate is preserved; NeMo-Speech.cpp resamples internally.
#[derive(Debug)]
pub struct AudioChunk {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
}

/// Receiver plus an out-of-band signal that makes queue saturation explicit.
/// If saturation occurs, callers must discard partial ASR and use the complete
/// archived capture instead.
pub struct AudioChunkStream {
    chunks: mpsc::Receiver<AudioChunk>,
    interrupted: Arc<AtomicBool>,
}

impl AudioChunkStream {
    pub fn into_parts(self) -> (mpsc::Receiver<AudioChunk>, Arc<AtomicBool>) {
        (self.chunks, self.interrupted)
    }
}

const STREAM_QUEUE_CHUNKS: usize = 32;
// Leave room for the WAV header within the existing 512 MiB archival limit.
const CAPTURE_BUFFER_SAMPLES: usize = (512 * 1024 * 1024 - 4096) / std::mem::size_of::<f32>();

pub fn list_input_devices() -> Result<Vec<InputDevice>> {
    let host = cpal::default_host();
    let default_name = host
        .default_input_device()
        .and_then(|device| device.name().ok());
    let devices = host
        .input_devices()
        .map_err(|err| audio_error("microphone", format!("cannot list input devices: {err}")))?;
    devices
        .enumerate()
        .map(|(index, device)| {
            let name = device
                .name()
                .map_err(|err| audio_error("microphone", err.to_string()))?;
            Ok(InputDevice {
                id: device_id(index, &name),
                is_default: default_name.as_ref() == Some(&name),
                name,
            })
        })
        .collect()
}

fn device_id(index: usize, name: &str) -> String {
    format!("{index}:{name}")
}

enum Command {
    Start {
        device: Option<String>,
        max_seconds: u32,
        chunk_sender: Option<mpsc::SyncSender<AudioChunk>>,
        chunk_interrupted: Option<Arc<AtomicBool>>,
        reply: mpsc::Sender<Result<()>>,
    },
    Stop(mpsc::Sender<Result<CapturedAudio>>),
    Cancel(mpsc::Sender<Result<()>>),
    Shutdown,
}

/// Thread-safe microphone controller. The device and CPAL stream are created,
/// played, and dropped on one dedicated thread (including on macOS).
///
/// Zero disables the duration cutoff. Hitting an adapter or memory limit stops
/// the stream and retains the original audio
/// until `stop` retrieves it or `cancel` explicitly discards it.
pub struct AudioRecorder {
    sender: mpsc::Sender<Command>,
    status: Arc<Mutex<RecordingStatus>>,
    level: Arc<AtomicU32>,
    pitch: Arc<AtomicU32>,
    worker: Option<JoinHandle<()>>,
}

impl AudioRecorder {
    pub fn new() -> Result<Self> {
        let (sender, receiver) = mpsc::channel();
        let status = Arc::new(Mutex::new(RecordingStatus::default()));
        let thread_status = Arc::clone(&status);
        let level = Arc::new(AtomicU32::new(0));
        let thread_level = Arc::clone(&level);
        let pitch = Arc::new(AtomicU32::new(0));
        let thread_pitch = Arc::clone(&pitch);
        let worker = thread::Builder::new()
            .name("transcribe-microphone".into())
            .spawn(move || recording_worker(receiver, thread_status, thread_level, thread_pitch))
            .map_err(|err| {
                audio_error("microphone", format!("cannot start capture worker: {err}"))
            })?;
        Ok(Self {
            sender,
            status,
            level,
            pitch,
            worker: Some(worker),
        })
    }

    pub fn start(&self, device: Option<String>, max_seconds: u32) -> Result<()> {
        self.start_inner(device, max_seconds, None)
    }

    /// Start capture and return a bounded receiver of 20 ms mono chunks.
    /// Archival capture remains independent and retains the original channels.
    pub fn start_streaming(
        &self,
        device: Option<String>,
        max_seconds: u32,
    ) -> Result<AudioChunkStream> {
        let (chunk_sender, chunks) = mpsc::sync_channel(STREAM_QUEUE_CHUNKS);
        let interrupted = Arc::new(AtomicBool::new(false));
        self.start_inner_with_signal(
            device,
            max_seconds,
            Some(chunk_sender),
            Some(Arc::clone(&interrupted)),
        )?;
        Ok(AudioChunkStream {
            chunks,
            interrupted,
        })
    }

    fn start_inner(
        &self,
        device: Option<String>,
        max_seconds: u32,
        chunk_sender: Option<mpsc::SyncSender<AudioChunk>>,
    ) -> Result<()> {
        self.start_inner_with_signal(device, max_seconds, chunk_sender, None)
    }

    fn start_inner_with_signal(
        &self,
        device: Option<String>,
        max_seconds: u32,
        chunk_sender: Option<mpsc::SyncSender<AudioChunk>>,
        chunk_interrupted: Option<Arc<AtomicBool>>,
    ) -> Result<()> {
        validate_limit(max_seconds)?;
        let (reply, response) = mpsc::channel();
        self.sender
            .send(Command::Start {
                device,
                max_seconds,
                chunk_sender,
                chunk_interrupted,
                reply,
            })
            .map_err(worker_unavailable)?;
        response.recv().map_err(worker_unavailable)?
    }

    pub fn stop(&self) -> Result<CapturedAudio> {
        let (reply, response) = mpsc::channel();
        let result = self
            .sender
            .send(Command::Stop(reply))
            .map_err(worker_unavailable)
            .and_then(|()| response.recv().map_err(worker_unavailable))
            .and_then(|result| result);
        self.level.store(0, Ordering::Relaxed);
        self.pitch.store(0, Ordering::Relaxed);
        result
    }

    pub fn cancel(&self) -> Result<()> {
        let (reply, response) = mpsc::channel();
        let result = self
            .sender
            .send(Command::Cancel(reply))
            .map_err(worker_unavailable)
            .and_then(|()| response.recv().map_err(worker_unavailable))
            .and_then(|result| result);
        self.level.store(0, Ordering::Relaxed);
        self.pitch.store(0, Ordering::Relaxed);
        result
    }

    pub fn status(&self) -> RecordingStatus {
        self.status
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    /// DC-free RMS of the most recent 40 ms of captured audio, normalized to [0, 1].
    /// Reading the meter never starts capture or takes a capture/status lock.
    pub fn current_level(&self) -> f32 {
        f32::from_bits(self.level.load(Ordering::Relaxed))
    }

    /// Latest visual telemetry. Never opens a device or waits on capture locks.
    pub fn current_features(&self) -> AudioFeatures {
        let level = self.current_level();
        let pitch = f32::from_bits(self.pitch.load(Ordering::Relaxed));
        AudioFeatures {
            level,
            pitch_hz: (level >= 0.002 && pitch.is_finite() && (60.0..=700.0).contains(&pitch))
                .then_some(pitch),
        }
    }
}

impl Drop for AudioRecorder {
    fn drop(&mut self) {
        let _ = self.sender.send(Command::Shutdown);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn worker_unavailable(error: impl std::fmt::Display) -> vesper_core::Error {
    audio_error(
        "microphone",
        format!("capture worker is unavailable: {error}"),
    )
}

#[derive(Default)]
struct CallbackState {
    halted: AtomicBool,
    limit_reached: AtomicBool,
    error_code: AtomicUsize,
    captured_samples: AtomicUsize,
    level: Arc<AtomicU32>,
}

struct Session {
    // This struct never leaves recording_worker: cpal::Stream need not be Send.
    stream: Option<cpal::Stream>,
    consumer: Consumer<f32>,
    state: Arc<CallbackState>,
    errors: mpsc::Receiver<String>,
    samples: Vec<f32>,
    chunk_sender: Option<mpsc::SyncSender<AudioChunk>>,
    chunk_interrupted: Option<Arc<AtomicBool>>,
    chunk_pending: Vec<f32>,
    sample_rate: u32,
    channels: u16,
    started: Instant,
    max_duration: Option<Duration>,
    error: Option<String>,
}

impl Session {
    fn drain(&mut self) {
        let first_new = self.samples.len();
        while let Ok(sample) = self.consumer.pop() {
            self.samples.push(sample);
        }
        if self.chunk_sender.is_some() {
            self.chunk_pending
                .extend_from_slice(&self.samples[first_new..]);
            self.emit_stream_chunks(false);
        }
    }

    fn emit_stream_chunks(&mut self, flush: bool) {
        let channels = usize::from(self.channels);
        let regular = (self.sample_rate as usize / 50).max(1) * channels;
        loop {
            let available = self.chunk_pending.len() / channels * channels;
            let count = if available >= regular {
                regular
            } else if flush && available > 0 {
                available
            } else {
                break;
            };
            let samples = self.chunk_pending[..count]
                .chunks_exact(channels)
                .map(|frame| {
                    (frame.iter().map(|&sample| f64::from(sample)).sum::<f64>() / channels as f64)
                        as f32
                })
                .collect();
            self.chunk_pending.drain(..count);
            let chunk = AudioChunk {
                samples,
                sample_rate: self.sample_rate,
            };
            match self
                .chunk_sender
                .as_ref()
                .map(|sender| sender.try_send(chunk))
            {
                Some(Ok(())) => {}
                Some(Err(mpsc::TrySendError::Full(_))) => {
                    if let Some(interrupted) = &self.chunk_interrupted {
                        interrupted.store(true, Ordering::Release);
                    }
                    self.chunk_sender = None;
                    self.chunk_pending.clear();
                    break;
                }
                Some(Err(mpsc::TrySendError::Disconnected(_))) | None => {
                    self.chunk_sender = None;
                    self.chunk_pending.clear();
                    break;
                }
            }
        }
    }

    fn refresh(&mut self) -> RecordingStatus {
        self.drain();
        if self.stream.is_some()
            && self
                .max_duration
                .is_some_and(|limit| self.started.elapsed() >= limit)
        {
            self.state.limit_reached.store(true, Ordering::Release);
            self.state.halted.store(true, Ordering::Release);
        }
        if self.state.halted.load(Ordering::Acquire) {
            self.stream.take();
            self.state.level.store(0, Ordering::Relaxed);
            self.drain();
        }
        if self.max_duration.is_none() && self.state.limit_reached.load(Ordering::Acquire) {
            self.error = Some("Recording stopped at the 512 MiB audio buffer limit. Captured audio has been retained.".into());
        }
        if let Ok(message) = self.errors.try_recv() {
            self.error = Some(message);
        }
        match self.state.error_code.load(Ordering::Acquire) {
            1 => {
                self.error = Some(
                    "microphone buffer overflowed; close busy applications and try again".into(),
                )
            }
            2 => self.error = Some("microphone supplied non-finite audio samples".into()),
            3 if self.error.is_none() => {
                self.error = Some(
                    "microphone stream failed; check device connection and microphone permission"
                        .into(),
                )
            }
            _ => {}
        }
        RecordingStatus {
            recording: self.stream.is_some(),
            limit_reached: self.state.limit_reached.load(Ordering::Acquire),
            duration_seconds: self.samples.len() as f64
                / f64::from(self.channels)
                / f64::from(self.sample_rate),
            error: self.error.clone(),
        }
    }

    fn finish(mut self) -> Result<CapturedAudio> {
        self.state.halted.store(true, Ordering::Release);
        self.stream.take();
        self.refresh();
        self.emit_stream_chunks(true);
        self.chunk_sender.take();
        // An overflow or invalid sample can interrupt an interleaved frame.
        // Preserve the valid prefix without passing a partial frame downstream.
        let complete_samples =
            self.samples.len() / usize::from(self.channels) * usize::from(self.channels);
        self.samples.truncate(complete_samples);
        if self.samples.is_empty() {
            return Err(audio_error(
                "microphone",
                self.error.unwrap_or_else(|| {
                    "no audio was captured; check microphone permission and input device".into()
                }),
            ));
        }
        CapturedAudio::new(self.samples, self.sample_rate, self.channels)
            .map(|audio| audio.with_capture_warning(self.error))
    }
}

fn recording_worker(
    receiver: mpsc::Receiver<Command>,
    status: Arc<Mutex<RecordingStatus>>,
    level: Arc<AtomicU32>,
    pitch: Arc<AtomicU32>,
) {
    let mut active: Option<Session> = None;
    let mut pitch_samples = 0;
    let mut pitch_updated = Instant::now();
    loop {
        // Sleep without polling while idle or while completed audio awaits stop.
        let command = if active
            .as_ref()
            .is_some_and(|session| session.stream.is_some())
        {
            receiver.recv_timeout(Duration::from_millis(5))
        } else {
            receiver
                .recv()
                .map_err(|_| mpsc::RecvTimeoutError::Disconnected)
        };
        match command {
            Ok(Command::Start {
                device,
                max_seconds,
                chunk_sender,
                chunk_interrupted,
                reply,
            }) => {
                let result = if active.is_some() {
                    Err(audio_error(
                        "microphone",
                        "finish or cancel the previous recording before starting another",
                    ))
                } else {
                    level.store(0, Ordering::Relaxed);
                    pitch.store(0, Ordering::Relaxed);
                    pitch_samples = 0;
                    pitch_updated = Instant::now();
                    match start_session(
                        device,
                        max_seconds,
                        Arc::clone(&level),
                        chunk_sender,
                        chunk_interrupted,
                    ) {
                        Ok(mut session) => {
                            *status.lock().unwrap_or_else(|p| p.into_inner()) = session.refresh();
                            active = Some(session);
                            Ok(())
                        }
                        Err(err) => {
                            level.store(0, Ordering::Relaxed);
                            pitch.store(0, Ordering::Relaxed);
                            *status.lock().unwrap_or_else(|p| p.into_inner()) = RecordingStatus {
                                error: Some(err.to_string()),
                                ..Default::default()
                            };
                            Err(err)
                        }
                    }
                };
                let _ = reply.send(result);
            }
            Ok(Command::Stop(reply)) => {
                let result = active
                    .take()
                    .ok_or_else(|| audio_error("microphone", "there is no recording to stop"))
                    .and_then(Session::finish);
                level.store(0, Ordering::Relaxed);
                pitch.store(0, Ordering::Relaxed);
                let mut latest = status.lock().unwrap_or_else(|p| p.into_inner());
                latest.recording = false;
                if let Ok(audio) = &result {
                    latest.duration_seconds = audio.duration_seconds();
                }
                if let Err(err) = &result {
                    latest.error = Some(err.to_string());
                }
                let _ = reply.send(result);
            }
            Ok(Command::Cancel(reply)) => {
                if let Some(mut session) = active.take() {
                    session.state.halted.store(true, Ordering::Release);
                    session.stream.take();
                }
                level.store(0, Ordering::Relaxed);
                pitch.store(0, Ordering::Relaxed);
                *status.lock().unwrap_or_else(|p| p.into_inner()) = RecordingStatus::default();
                let _ = reply.send(Ok(()));
            }
            Ok(Command::Shutdown) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        if let Some(session) = active.as_mut() {
            let latest = session.refresh();
            if !latest.recording {
                pitch.store(0, Ordering::Relaxed);
            } else if pitch_updated.elapsed() >= Duration::from_millis(40)
                && pitch_samples != session.samples.len()
            {
                // Analyze an already-drained PCM view, away from the callback.
                // Work is bounded and independent of ASR and archive ownership.
                let hz = estimate_pitch(&session.samples, session.sample_rate, session.channels);
                pitch.store(hz.unwrap_or(0.0).to_bits(), Ordering::Relaxed);
                pitch_samples = session.samples.len();
                pitch_updated = Instant::now();
            } else if pitch_updated.elapsed() > Duration::from_millis(200) {
                // A live bridge must not keep displaying old input after the
                // device stops delivering samples without an error callback.
                pitch.store(0, Ordering::Relaxed);
                level.store(0, Ordering::Relaxed);
            }
            *status.lock().unwrap_or_else(|p| p.into_inner()) = latest;
        }
    }
    // Explicitly stop before dropping the consumer and its backing storage.
    if let Some(mut session) = active {
        session.state.halted.store(true, Ordering::Release);
        session.stream.take();
    }
    level.store(0, Ordering::Relaxed);
    pitch.store(0, Ordering::Relaxed);
}

fn start_session(
    selected: Option<String>,
    max_seconds: u32,
    level: Arc<AtomicU32>,
    chunk_sender: Option<mpsc::SyncSender<AudioChunk>>,
    chunk_interrupted: Option<Arc<AtomicBool>>,
) -> Result<Session> {
    let host = cpal::default_host();
    let device = if let Some(selected) = selected {
        host.input_devices()
            .map_err(|err| audio_error("microphone", err.to_string()))?
            .enumerate()
            .find_map(|(index, device)| {
                let name = device.name().ok()?;
                (device_id(index, &name) == selected).then_some(device)
            })
            .ok_or_else(|| {
                audio_error(
                    "microphone",
                    "selected input device is no longer available; refresh the device list",
                )
            })?
    } else {
        host.default_input_device()
            .ok_or_else(|| audio_error("microphone", "no default microphone is available"))?
    };
    let supported = device.default_input_config().map_err(|err| {
        audio_error(
            "microphone",
            format!("cannot open microphone; check input device and microphone permission: {err}"),
        )
    })?;
    let config: cpal::StreamConfig = supported.clone().into();
    let rate = config.sample_rate.0;
    let channels = config.channels;
    validate_format(rate, usize::from(channels))?;
    let max_samples = if max_seconds == 0 {
        CAPTURE_BUFFER_SAMPLES
    } else {
        usize::try_from(u64::from(rate) * u64::from(channels) * u64::from(max_seconds))
            .map_err(|_| audio_error("microphone", "recording is too large for this platform"))?
    };
    if max_samples > CAPTURE_BUFFER_SAMPLES {
        return Err(audio_error("microphone", "selected device format exceeds the 512 MiB recording buffer limit; shorten the recording limit or use fewer channels"));
    }
    let mut samples = Vec::new();
    samples.try_reserve_exact(max_samples).map_err(|err| {
        audio_error(
            "microphone",
            format!("cannot allocate recording buffer: {err}"),
        )
    })?;
    let ring_capacity = (rate as usize * usize::from(channels) * 2).min(max_samples);
    let (producer, consumer) = RingBuffer::<f32>::new(ring_capacity);
    let state = Arc::new(CallbackState {
        level,
        ..Default::default()
    });
    let (error_sender, errors) = mpsc::sync_channel(1);
    let stream = match supported.sample_format() {
        SampleFormat::F32 => build_stream::<f32>(
            &device,
            &config,
            producer,
            &state,
            error_sender,
            max_samples,
        ),
        SampleFormat::F64 => build_stream::<f64>(
            &device,
            &config,
            producer,
            &state,
            error_sender,
            max_samples,
        ),
        SampleFormat::I8 => build_stream::<i8>(
            &device,
            &config,
            producer,
            &state,
            error_sender,
            max_samples,
        ),
        SampleFormat::I16 => build_stream::<i16>(
            &device,
            &config,
            producer,
            &state,
            error_sender,
            max_samples,
        ),
        SampleFormat::I32 => build_stream::<i32>(
            &device,
            &config,
            producer,
            &state,
            error_sender,
            max_samples,
        ),
        SampleFormat::I64 => build_stream::<i64>(
            &device,
            &config,
            producer,
            &state,
            error_sender,
            max_samples,
        ),
        SampleFormat::U8 => build_stream::<u8>(
            &device,
            &config,
            producer,
            &state,
            error_sender,
            max_samples,
        ),
        SampleFormat::U16 => build_stream::<u16>(
            &device,
            &config,
            producer,
            &state,
            error_sender,
            max_samples,
        ),
        SampleFormat::U32 => build_stream::<u32>(
            &device,
            &config,
            producer,
            &state,
            error_sender,
            max_samples,
        ),
        SampleFormat::U64 => build_stream::<u64>(
            &device,
            &config,
            producer,
            &state,
            error_sender,
            max_samples,
        ),
        other => {
            return Err(audio_error(
                "microphone",
                format!("unsupported microphone sample format: {other}"),
            ))
        }
    }?;
    stream.play().map_err(|err| {
        audio_error(
            "microphone",
            format!("cannot start microphone; check microphone permission: {err}"),
        )
    })?;
    Ok(Session {
        stream: Some(stream),
        consumer,
        state,
        errors,
        samples,
        chunk_sender,
        chunk_interrupted,
        chunk_pending: Vec::with_capacity(ring_capacity.min(rate as usize)),
        sample_rate: rate,
        channels,
        started: Instant::now(),
        max_duration: (max_seconds > 0).then(|| Duration::from_secs(u64::from(max_seconds))),
        error: None,
    })
}

fn build_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    mut producer: Producer<f32>,
    state: &Arc<CallbackState>,
    error_sender: mpsc::SyncSender<String>,
    max_samples: usize,
) -> Result<cpal::Stream>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let callback_state = Arc::clone(state);
    let error_state = Arc::clone(state);
    let mut meter = RmsMeter::new(config.sample_rate.0, config.channels);
    device
        .build_input_stream(
            config,
            move |data: &[T], _: &cpal::InputCallbackInfo| {
                capture_samples(
                    data,
                    &mut producer,
                    &callback_state,
                    max_samples,
                    &mut meter,
                );
            },
            move |err| {
                error_state.error_code.store(3, Ordering::Release);
                error_state.halted.store(true, Ordering::Release);
                error_state.level.store(0, Ordering::Relaxed);
                let _ = error_sender.try_send(format!("microphone stream failed: {err}"));
            },
            None,
        )
        .map_err(|err| {
            audio_error(
                "microphone",
                format!("cannot create microphone stream; check microphone permission: {err}"),
            )
        })
}

/// The audio callback performs no allocations, locks, resampling, or inference.
fn capture_samples<T>(
    data: &[T],
    producer: &mut Producer<f32>,
    state: &CallbackState,
    max_samples: usize,
    meter: &mut RmsMeter,
) where
    T: Sample,
    f32: FromSample<T>,
{
    if state.halted.load(Ordering::Acquire) {
        state.level.store(0, Ordering::Relaxed);
        return;
    }
    let mut count = state.captured_samples.load(Ordering::Relaxed);
    let mut metered_samples = 0_usize;
    for &sample in data.iter().take(max_samples.saturating_sub(count)) {
        let sample = sample.to_sample::<f32>();
        if !sample.is_finite() {
            state.error_code.store(2, Ordering::Release);
            state.halted.store(true, Ordering::Release);
            break;
        }
        if producer.push(sample).is_err() {
            state.error_code.store(1, Ordering::Release);
            state.halted.store(true, Ordering::Release);
            break;
        }
        meter.push(sample);
        metered_samples += 1;
        count += 1;
    }
    state.captured_samples.store(count, Ordering::Release);
    if count >= max_samples {
        state.limit_reached.store(true, Ordering::Release);
        state.halted.store(true, Ordering::Release);
    }
    let level = if metered_samples == 0 || state.halted.load(Ordering::Acquire) {
        0.0
    } else {
        meter.level()
    };
    state.level.store(level.to_bits(), Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn callback_meter_reports_rms_silence_and_clamps_without_modifying_pcm() {
        let (mut producer, mut consumer) = RingBuffer::new(32);
        let state = CallbackState::default();
        let mut meter = RmsMeter::new(50, 1); // Two-sample window for these callback boundary fixtures.
        let level = || f32::from_bits(state.level.load(Ordering::Relaxed));
        capture_samples(&[0.3_f32, 0.4], &mut producer, &state, 100, &mut meter);
        assert!((level() - 0.05).abs() < 0.00001); // Mean 0.35 is DC, not sound.
        assert_eq!(consumer.pop().unwrap(), 0.3);
        assert_eq!(consumer.pop().unwrap(), 0.4);
        capture_samples(&[0.0_f32, 0.0], &mut producer, &state, 100, &mut meter);
        assert_eq!(level(), 0.0);
        capture_samples(&[2.0_f32, -2.0], &mut producer, &state, 100, &mut meter);
        assert_eq!(level(), 1.0);
        assert_eq!(consumer.pop().unwrap(), 0.0);
        assert_eq!(consumer.pop().unwrap(), 0.0);
        assert_eq!(consumer.pop().unwrap(), 2.0);
        assert_eq!(consumer.pop().unwrap(), -2.0);
        capture_samples(&[16384_i16, -16384], &mut producer, &state, 100, &mut meter);
        assert_eq!(level(), 0.5);
        capture_samples::<f32>(&[], &mut producer, &state, 100, &mut meter);
        assert_eq!(level(), 0.0);
    }

    #[test]
    fn invalid_samples_and_halted_callbacks_clear_the_meter() {
        for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let (mut producer, mut consumer) = RingBuffer::new(8);
            let state = CallbackState::default();
            let mut meter = RmsMeter::new(50, 1); // Two-sample window for these callback boundary fixtures.
            state.level.store(0.5_f32.to_bits(), Ordering::Relaxed);
            capture_samples(
                &[0.25_f32, invalid, 0.75],
                &mut producer,
                &state,
                100,
                &mut meter,
            );
            assert_eq!(f32::from_bits(state.level.load(Ordering::Relaxed)), 0.0);
            assert_eq!(state.error_code.load(Ordering::Acquire), 2);
            assert_eq!(consumer.pop().unwrap(), 0.25);
            assert!(consumer.pop().is_err());
            capture_samples(&[0.75_f32], &mut producer, &state, 100, &mut meter);
            assert_eq!(f32::from_bits(state.level.load(Ordering::Relaxed)), 0.0);
        }
    }

    #[test]
    fn recorder_idle_stop_and_cancel_reset_meter_without_opening_a_microphone() {
        let recorder = AudioRecorder::new().unwrap();
        assert_eq!(recorder.current_level(), 0.0);
        assert_eq!(recorder.current_features().pitch_hz, None);
        // Seed telemetry only; the worker remains idle and no device is opened.
        recorder.level.store(0.5_f32.to_bits(), Ordering::Relaxed);
        recorder.pitch.store(220.0_f32.to_bits(), Ordering::Relaxed);
        assert_eq!(recorder.current_features().pitch_hz, Some(220.0));
        recorder.cancel().unwrap();
        assert_eq!(recorder.current_level(), 0.0);
        assert_eq!(recorder.current_features().pitch_hz, None);
        recorder.level.store(0.75_f32.to_bits(), Ordering::Relaxed);
        recorder.pitch.store(110.0_f32.to_bits(), Ordering::Relaxed);
        assert!(recorder.stop().is_err());
        assert_eq!(recorder.current_level(), 0.0);
        assert_eq!(recorder.current_features().pitch_hz, None);
    }

    #[test]
    fn callback_limits_samples_and_reports_overflow_without_silent_loss() {
        let (mut producer, mut consumer) = RingBuffer::new(8);
        let state = CallbackState::default();
        let mut meter = RmsMeter::new(50, 1); // Two-sample window for these callback boundary fixtures.
        capture_samples(
            &[i16::MIN, 0, i16::MAX, 123, 456],
            &mut producer,
            &state,
            4,
            &mut meter,
        );
        assert!(state.limit_reached.load(Ordering::Acquire));
        assert_eq!(state.level.load(Ordering::Relaxed), 0);
        assert_eq!(state.captured_samples.load(Ordering::Acquire), 4);
        capture_samples(&[999_i16], &mut producer, &state, 4, &mut meter);
        let mut captured = Vec::new();
        while let Ok(sample) = consumer.pop() {
            captured.push(sample);
        }
        assert_eq!(captured.len(), 4);
        assert_eq!(captured[0], -1.0);
        assert_eq!(captured[1], 0.0);
        assert!((captured[2] - 1.0).abs() < 0.0001);

        let (mut producer, _consumer) = RingBuffer::new(1);
        let state = CallbackState::default();
        let mut meter = RmsMeter::new(50, 1); // Two-sample window for these callback boundary fixtures.
        capture_samples(&[0.5_f32, 0.5], &mut producer, &state, 10, &mut meter);
        assert_eq!(state.error_code.load(Ordering::Acquire), 1);
        assert!(state.halted.load(Ordering::Acquire));
        assert_eq!(state.level.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn resource_stop_without_a_timer_preserves_audio_and_reports_why() {
        let (mut producer, consumer) = RingBuffer::new(8);
        let state = Arc::new(CallbackState::default());
        // Two-sample meter and a small budget exercise the callback boundary.
        let mut meter = RmsMeter::new(50, 1);
        capture_samples(
            &[0.25_f32, -0.5, 0.75, 1.0, 0.0],
            &mut producer,
            &state,
            4,
            &mut meter,
        );
        let (_sender, errors) = mpsc::channel();
        let session = Session {
            stream: None,
            consumer,
            state,
            errors,
            samples: Vec::new(),
            chunk_sender: None,
            chunk_interrupted: None,
            chunk_pending: Vec::new(),
            sample_rate: 48_000,
            channels: 2,
            started: Instant::now() - Duration::from_secs(601),
            max_duration: None,
            error: None,
        };
        let audio = session.finish().unwrap();
        assert_eq!(audio.samples(), &[0.25, -0.5, 0.75, 1.0]);
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("retained.wav");
        audio.write_wav(&path).unwrap();
        let header_bytes = std::fs::metadata(path).unwrap().len() as usize
            - std::mem::size_of_val(audio.samples());
        assert!(
            CAPTURE_BUFFER_SAMPLES * std::mem::size_of::<f32>() + header_bytes <= 512 * 1024 * 1024
        );
        assert!(audio
            .capture_warning()
            .unwrap()
            .contains("512 MiB audio buffer limit"));
    }

    #[test]
    fn failed_capture_retains_complete_frames_and_reports_early_end() {
        let (mut producer, consumer) = RingBuffer::new(8);
        for sample in [0.25, -0.5, 0.75] {
            producer.push(sample).unwrap();
        }
        let state = Arc::new(CallbackState::default());
        state.level.store(0.5_f32.to_bits(), Ordering::Relaxed);
        let meter = Arc::clone(&state.level);
        state.error_code.store(1, Ordering::Release);
        let (_sender, errors) = mpsc::channel();
        let session = Session {
            stream: None,
            consumer,
            state,
            errors,
            samples: Vec::new(),
            chunk_sender: None,
            chunk_interrupted: None,
            chunk_pending: Vec::new(),
            sample_rate: 48_000,
            channels: 2,
            started: Instant::now(),
            max_duration: Some(Duration::from_secs(1)),
            error: None,
        };
        let audio = session.finish().unwrap();
        assert_eq!(f32::from_bits(meter.load(Ordering::Relaxed)), 0.0);
        assert_eq!(audio.samples(), &[0.25, -0.5]);
        assert_eq!(audio.channels(), 2);
        assert_eq!(audio.sample_rate(), 48_000);
        assert!(audio.capture_warning().unwrap().contains("overflowed"));
        assert_eq!(audio.duration_seconds(), 1.0 / 48_000.0);
    }

    #[test]
    fn streaming_chunks_mix_channels_and_flush_the_tail() {
        let (mut producer, consumer) = RingBuffer::new(16);
        for sample in [1.0, -1.0, 0.5, 0.25, -0.5, -0.25] {
            producer.push(sample).unwrap();
        }
        let (chunk_sender, chunks) = mpsc::sync_channel(2);
        let interrupted = Arc::new(AtomicBool::new(false));
        let (_error_sender, errors) = mpsc::channel();
        let session = Session {
            stream: None,
            consumer,
            state: Arc::new(CallbackState::default()),
            errors,
            samples: Vec::new(),
            chunk_sender: Some(chunk_sender),
            chunk_interrupted: Some(Arc::clone(&interrupted)),
            chunk_pending: Vec::new(),
            sample_rate: 100,
            channels: 2,
            started: Instant::now(),
            max_duration: Some(Duration::from_secs(1)),
            error: None,
        };
        let audio = session.finish().unwrap();
        assert_eq!(audio.samples().len(), 6);
        let streamed = chunks.into_iter().collect::<Vec<_>>();
        assert_eq!(streamed.len(), 2);
        assert_eq!(streamed[0].samples, vec![0.0, 0.375]);
        assert_eq!(streamed[1].samples, vec![-0.375]);
        assert!(streamed.iter().all(|chunk| chunk.sample_rate == 100));
        assert!(!interrupted.load(Ordering::Acquire));
    }

    #[test]
    fn saturated_stream_queue_never_blocks_capture_and_requests_offline_fallback() {
        let (mut producer, consumer) = RingBuffer::new(16);
        for sample in [0.1, 0.2, 0.3, 0.4, 0.5, 0.6] {
            producer.push(sample).unwrap();
        }
        let (chunk_sender, chunks) = mpsc::sync_channel(1);
        let interrupted = Arc::new(AtomicBool::new(false));
        let (_error_sender, errors) = mpsc::channel();
        let session = Session {
            stream: None,
            consumer,
            state: Arc::new(CallbackState::default()),
            errors,
            samples: Vec::new(),
            chunk_sender: Some(chunk_sender),
            chunk_interrupted: Some(Arc::clone(&interrupted)),
            chunk_pending: Vec::new(),
            sample_rate: 100,
            channels: 1,
            started: Instant::now(),
            max_duration: Some(Duration::from_secs(1)),
            error: None,
        };
        let audio = session.finish().unwrap();
        assert_eq!(audio.samples().len(), 6, "archival audio remains complete");
        assert_eq!(chunks.into_iter().count(), 1);
        assert!(interrupted.load(Ordering::Acquire));
    }
}
