use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::Serialize;
use transcribe_audio::MonoPcm;
use transcribe_core::{AppConfig, PersonalizationProfile};

use crate::{
    adapters, check_cancelled, native, qwen, InferenceError, Result, TranscriptionProgressSink,
};

const COMMAND_QUEUE: usize = 2;
const STREAM_QUEUE: usize = 32;

pub type StreamingUpdateSink = Arc<dyn Fn(StreamingTranscript) + Send + Sync + 'static>;

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StreamingTranscript {
    pub operation_id: String,
    pub sequence: u64,
    pub committed_text: String,
    pub provisional_text: String,
    pub audio_processed_seconds: f32,
    pub emitted_at_unix_ms: u64,
    pub is_final: bool,
}

enum Command {
    Transcribe {
        audio: MonoPcm,
        config: AppConfig,
        profile: PersonalizationProfile,
        model_root: PathBuf,
        operation_id: String,
        cancelled: Arc<AtomicBool>,
        progress: TranscriptionProgressSink,
        reply: mpsc::SyncSender<Result<String>>,
    },
    Rewrite {
        operation_id: String,
        raw: String,
        config: AppConfig,
        profile: PersonalizationProfile,
        model_root: PathBuf,
        cancelled: Arc<AtomicBool>,
        reply: mpsc::SyncSender<Result<String>>,
    },
    StartStream {
        config: AppConfig,
        profile: PersonalizationProfile,
        model_root: PathBuf,
        operation_id: String,
        cancelled: Arc<AtomicBool>,
        stream_cancelled: Arc<AtomicBool>,
        updates: StreamingUpdateSink,
        input: mpsc::Receiver<StreamInput>,
        started: mpsc::SyncSender<Result<()>>,
        completed: mpsc::SyncSender<Result<String>>,
    },
    Unload(mpsc::SyncSender<()>),
    Shutdown,
}

enum StreamInput {
    Audio { samples: Vec<f32>, sample_rate: u32 },
    Finish,
    Cancel,
}

/// The only owner of resident native model handles.
pub struct InferenceManager {
    sender: mpsc::SyncSender<Command>,
    worker: Mutex<Option<JoinHandle<()>>>,
    evidence: Arc<Mutex<serde_json::Value>>,
}

impl InferenceManager {
    pub fn new() -> std::io::Result<Self> {
        let (sender, receiver) = mpsc::sync_channel(COMMAND_QUEUE);
        let evidence = Arc::new(Mutex::new(serde_json::json!({})));
        let worker_evidence = Arc::clone(&evidence);
        let worker = thread::Builder::new()
            .name("transcribe-inference".into())
            .spawn(move || worker_loop(receiver, worker_evidence))?;
        Ok(Self {
            sender,
            worker: Mutex::new(Some(worker)),
            evidence,
        })
    }

    pub fn transcribe(
        &self,
        audio: &MonoPcm,
        config: &AppConfig,
        model_root: &Path,
        operation_id: &str,
        cancelled: Arc<AtomicBool>,
    ) -> Result<String> {
        self.transcribe_with_progress(
            audio,
            config,
            model_root,
            operation_id,
            cancelled,
            Arc::new(|_| {}),
        )
    }

    pub fn transcribe_with_progress(
        &self,
        audio: &MonoPcm,
        config: &AppConfig,
        model_root: &Path,
        operation_id: &str,
        cancelled: Arc<AtomicBool>,
        progress: TranscriptionProgressSink,
    ) -> Result<String> {
        self.transcribe_personalized(
            audio,
            config,
            model_root,
            operation_id,
            cancelled,
            progress,
            &PersonalizationProfile::default(),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn transcribe_personalized(
        &self,
        audio: &MonoPcm,
        config: &AppConfig,
        model_root: &Path,
        operation_id: &str,
        cancelled: Arc<AtomicBool>,
        progress: TranscriptionProgressSink,
        profile: &PersonalizationProfile,
    ) -> Result<String> {
        check_cancelled(&cancelled)?;
        profile
            .validate()
            .map_err(|e| native("personalization", e))?;
        let (reply, response) = mpsc::sync_channel(1);
        self.send(Command::Transcribe {
            audio: audio.clone(),
            config: config.clone(),
            profile: profile.clone(),
            model_root: model_root.to_owned(),
            operation_id: operation_id.to_owned(),
            cancelled,
            progress,
            reply,
        })?;
        response.recv().map_err(worker_unavailable)?
    }

    pub fn rewrite(
        &self,
        raw: &str,
        config: &AppConfig,
        model_root: &Path,
        cancelled: Arc<AtomicBool>,
    ) -> Result<String> {
        self.rewrite_personalized(
            raw,
            config,
            model_root,
            "compat-rewrite",
            cancelled,
            &PersonalizationProfile::default(),
        )
    }

    pub fn rewrite_personalized(
        &self,
        raw: &str,
        config: &AppConfig,
        model_root: &Path,
        operation_id: &str,
        cancelled: Arc<AtomicBool>,
        profile: &PersonalizationProfile,
    ) -> Result<String> {
        check_cancelled(&cancelled)?;
        profile
            .validate()
            .map_err(|e| native("personalization", e))?;
        if !config.cleanup_enabled || raw.trim().is_empty() {
            return Ok(raw.to_owned());
        }
        let (reply, response) = mpsc::sync_channel(1);
        self.send(Command::Rewrite {
            operation_id: operation_id.to_owned(),
            raw: raw.to_owned(),
            config: config.clone(),
            profile: profile.clone(),
            model_root: model_root.to_owned(),
            cancelled,
            reply,
        })?;
        response.recv().map_err(worker_unavailable)?
    }

    pub fn start_stream(
        &self,
        config: &AppConfig,
        model_root: &Path,
        operation_id: &str,
        cancelled: Arc<AtomicBool>,
        updates: StreamingUpdateSink,
    ) -> Result<StreamingSession> {
        self.start_stream_personalized(
            config,
            model_root,
            operation_id,
            cancelled,
            updates,
            &PersonalizationProfile::default(),
        )
    }

    pub fn start_stream_personalized(
        &self,
        config: &AppConfig,
        model_root: &Path,
        operation_id: &str,
        cancelled: Arc<AtomicBool>,
        updates: StreamingUpdateSink,
        profile: &PersonalizationProfile,
    ) -> Result<StreamingSession> {
        check_cancelled(&cancelled)?;
        profile
            .validate()
            .map_err(|e| native("personalization", e))?;
        if !config.speech_adapter().capabilities.native_streaming {
            return Err(InferenceError::UnsupportedAsr(config.models.asr.id()));
        }
        let (input_sender, input) = mpsc::sync_channel(STREAM_QUEUE);
        let (started_sender, started) = mpsc::sync_channel(1);
        let (completed, completion) = mpsc::sync_channel(1);
        let stream_cancelled = Arc::new(AtomicBool::new(false));
        self.send(Command::StartStream {
            config: config.clone(),
            profile: profile.clone(),
            model_root: model_root.to_owned(),
            operation_id: operation_id.to_owned(),
            cancelled,
            stream_cancelled: Arc::clone(&stream_cancelled),
            updates,
            input,
            started: started_sender,
            completed,
        })?;
        started.recv().map_err(worker_unavailable)??;
        Ok(StreamingSession {
            sender: input_sender,
            completion: Some(completion),
            finished: false,
            cancelled: stream_cancelled,
        })
    }

    pub fn execution_evidence(&self, operation_id: &str) -> serde_json::Value {
        self.evidence
            .lock()
            .ok()
            .filter(|e| e["operationId"] == operation_id)
            .map(|e| e.clone())
            .unwrap_or_else(|| serde_json::json!({}))
    }

    /// Bounded, content-free evidence for the diagnostic panel, including when
    /// recording collection is disabled. The operation ID identifies its scope.
    pub fn latest_execution_evidence(&self) -> serde_json::Value {
        self.evidence
            .lock()
            .map(|e| e.clone())
            .unwrap_or_else(|_| serde_json::json!({}))
    }

    pub fn unload(&self) -> Result<()> {
        let (reply, response) = mpsc::sync_channel(1);
        self.send(Command::Unload(reply))?;
        response.recv().map_err(worker_unavailable)?;
        Ok(())
    }

    fn send(&self, command: Command) -> Result<()> {
        self.sender.send(command).map_err(worker_unavailable)
    }
}

impl Drop for InferenceManager {
    fn drop(&mut self) {
        let _ = self.sender.send(Command::Shutdown);
        if let Ok(worker) = self.worker.get_mut() {
            if let Some(worker) = worker.take() {
                let _ = worker.join();
            }
        }
    }
}

/// Handle for one recording. Dropping it requests cancellation and never
/// preserves stream state for the next utterance.
pub struct StreamingSession {
    sender: mpsc::SyncSender<StreamInput>,
    completion: Option<mpsc::Receiver<Result<String>>>,
    finished: bool,
    // Independent of application cancellation: cancelling partials must still
    // permit complete-audio fallback with the caller's original operation flag.
    cancelled: Arc<AtomicBool>,
}

impl StreamingSession {
    pub fn push(&self, samples: Vec<f32>, sample_rate: u32) -> Result<()> {
        check_cancelled(&self.cancelled)?;
        self.sender
            .send(StreamInput::Audio {
                samples,
                sample_rate,
            })
            .map_err(worker_unavailable)
    }

    pub fn finish(mut self) -> Result<String> {
        self.sender
            .send(StreamInput::Finish)
            .map_err(worker_unavailable)?;
        self.finished = true;
        self.completion
            .take()
            .expect("stream completion is consumed once")
            .recv()
            .map_err(worker_unavailable)?
    }

    pub fn cancel(mut self) -> Result<()> {
        self.cancelled.store(true, Ordering::Release);
        // Wake an idle receiver; a full queue already wakes it. The independent
        // flag bypasses queued audio rather than waiting behind it.
        let _ = self.sender.try_send(StreamInput::Cancel);
        self.finished = true;
        let result = self
            .completion
            .take()
            .expect("stream completion is consumed once")
            .recv()
            .map_err(worker_unavailable)?;
        match result {
            Err(InferenceError::Cancelled) | Ok(_) => Ok(()),
            Err(error) => Err(error),
        }
    }
}

impl Drop for StreamingSession {
    fn drop(&mut self) {
        if !self.finished {
            self.cancelled.store(true, Ordering::Release);
            let _ = self.sender.try_send(StreamInput::Cancel);
        }
    }
}

struct WorkerState {
    speech: Option<(adapters::SpeechKey, adapters::SpeechModel)>,
    rewriter: Option<(qwen::RewriterKey, adapters::TextModel)>,
    sherpa_load_count: u64,
    frontend: crate::audio_frontend::FrontendCache,
}

impl WorkerState {
    fn new() -> Self {
        Self {
            speech: None,
            rewriter: None,
            sherpa_load_count: 0,
            frontend: Default::default(),
        }
    }

    fn ensure_speech(
        &mut self,
        config: &AppConfig,
        model_root: &Path,
        cancelled: &AtomicBool,
    ) -> Result<&adapters::SpeechModel> {
        check_cancelled(cancelled)?;
        let key = adapters::speech_key(config, model_root)?;
        if self
            .speech
            .as_ref()
            .is_none_or(|(loaded, _)| loaded != &key)
        {
            check_cancelled(cancelled)?;
            self.speech = None;
            let mut model = adapters::SpeechModel::load(config, model_root, cancelled)?;
            if let adapters::SpeechModel::Sherpa(resident) = &mut model {
                if key != adapters::speech_key(config, model_root)? {
                    return Err(native(
                        "resident ASR identity",
                        "model files changed while loading",
                    ));
                }
                self.sherpa_load_count += 1;
                resident.load_ordinal = self.sherpa_load_count;
            }
            self.speech = Some((key, model));
        }
        Ok(&self.speech.as_ref().expect("recognizer was initialized").1)
    }

    /// Preserve the existing cold exact-zero path: verify, report completion,
    /// and avoid constructing ONNX sessions or caching verification success.
    fn sherpa_silence_without_model(
        &mut self,
        audio: &MonoPcm,
        config: &AppConfig,
        root: &Path,
        cancelled: &AtomicBool,
        progress: &TranscriptionProgressSink,
    ) -> Result<bool> {
        if !matches!(
            config.speech_adapter().kind,
            transcribe_core::AdapterKind::CohereAsr | transcribe_core::AdapterKind::Whisper
        ) {
            return Ok(false);
        }
        crate::offline_asr::validate_audio(audio, config, cancelled)?;
        if audio.samples().iter().any(|sample| *sample != 0.0) {
            return Ok(false);
        }
        let key = adapters::speech_key(config, root)?;
        if self
            .speech
            .as_ref()
            .is_some_and(|(loaded, _)| loaded == &key)
        {
            return Ok(false);
        }
        check_cancelled(cancelled)?;
        self.speech = None;
        transcribe_models::verify_asr(config, root, cancelled)
            .map_err(crate::resident_sherpa::verification_error)?;
        progress(crate::TranscriptionProgress {
            processed_samples: audio.samples().len(),
            total_samples: audio.samples().len(),
        });
        check_cancelled(cancelled)?;
        Ok(true)
    }

    fn ensure_rewriter(
        &mut self,
        config: &AppConfig,
        model_root: &Path,
        cancelled: &AtomicBool,
    ) -> Result<&adapters::TextModel> {
        check_cancelled(cancelled)?;
        let key = qwen::key(config, model_root)?;
        if self
            .rewriter
            .as_ref()
            .is_none_or(|(loaded, _)| loaded != &key)
        {
            check_cancelled(cancelled)?;
            self.rewriter = None;
            self.rewriter = Some((
                key,
                adapters::TextModel::load(config, model_root, cancelled)?,
            ));
        }
        Ok(&self.rewriter.as_ref().expect("rewriter was initialized").1)
    }

    fn unload(&mut self) {
        self.frontend.unload();
        self.rewriter = None;
        self.speech = None;
    }
}

fn record_evidence(
    evidence: &Mutex<serde_json::Value>,
    id: &str,
    role: &str,
    value: serde_json::Value,
) {
    if let Ok(mut evidence) = evidence.lock() {
        if evidence["operationId"] != id {
            *evidence = serde_json::json!({"operationId":id});
        }
        evidence[role] = value;
    }
}

fn prepare_audio(
    cache: &mut crate::audio_frontend::FrontendCache,
    audio: &MonoPcm,
    config: &AppConfig,
    root: &Path,
    id: &str,
    cancelled: &AtomicBool,
    evidence: &Mutex<serde_json::Value>,
) -> Result<Option<MonoPcm>> {
    check_cancelled(cancelled)?;
    let failed_stream = evidence
        .lock()
        .ok()
        .is_some_and(|v| v["operationId"] == id && v["audioFrontend"]["streamFailed"] == true);
    if !config.audio_frontend.enabled() || failed_stream {
        cache.unload();
        record_evidence(
            evidence,
            id,
            "audioFrontend",
            serde_json::json!({"enabled":false,
            "originalAudioFallback":failed_stream, "warning":if failed_stream { Some("Live audio processing was interrupted; transcription used the original recording.") } else { None }}),
        );
        return Ok(None);
    }
    match crate::audio_frontend::process_completed(cache, audio, config, root, cancelled) {
        Ok((pcm, metrics)) => {
            record_evidence(evidence, id, "audioFrontend", metrics.evidence());
            Ok(Some(pcm))
        }
        Err(InferenceError::Cancelled) => {
            cache.unload();
            Err(InferenceError::Cancelled)
        }
        Err(error) => {
            cache.unload();
            check_cancelled(cancelled)?;
            record_evidence(
                evidence,
                id,
                "audioFrontend",
                serde_json::json!({"originalAudioFallback":true,
                "error":error.to_string(), "warning":"Audio processing failed; transcription used the original recording."}),
            );
            Ok(None)
        }
    }
}

fn worker_loop(receiver: mpsc::Receiver<Command>, evidence: Arc<Mutex<serde_json::Value>>) {
    let mut state = WorkerState::new();
    while let Ok(command) = receiver.recv() {
        match command {
            Command::Transcribe {
                audio,
                config,
                profile,
                model_root,
                operation_id,
                cancelled,
                progress,
                reply,
            } => {
                let prepared = prepare_audio(
                    &mut state.frontend,
                    &audio,
                    &config,
                    &model_root,
                    &operation_id,
                    &cancelled,
                    &evidence,
                );
                let result = prepared.and_then(|processed| {
                let audio = processed.as_ref().unwrap_or(&audio);
                state.sherpa_silence_without_model(audio, &config, &model_root, &cancelled, &progress)
                    .and_then(|handled| {
                        if handled {
                            record_evidence(&evidence, &operation_id, "asr", serde_json::json!({
                                "requestedBackend":config.compute_backend, "initializedProvider":null,
                                "residentWeights":false,"evidence":"verified-exact-zero-without-native-load"}));
                            return Ok(String::new());
                        }
                        state.ensure_speech(&config, &model_root, &cancelled)
                    .and_then(|model| {
                        let result = model.transcribe(
                            audio,
                            &config,
                            &model_root,
                            &operation_id,
                            &cancelled,
                            &progress,
                            &profile,
                        );
                        record_evidence(
                            &evidence,
                            &operation_id,
                            "asr",
                            model.evidence(&config, &profile),
                        );
                        result
                    })})});
                // A reported native/identity failure does not establish safe ORT
                // reuse. Drop resident Sherpa sessions on the owner after the synchronous call.
                if state.speech.as_ref().is_some_and(|(_, model)| matches!(model,
                    adapters::SpeechModel::Sherpa(resident) if !resident.healthy.get() || matches!(result, Err(InferenceError::Native { .. }))))
                {
                    state.speech = None;
                }
                let _ = reply.send(result);
            }
            Command::Rewrite {
                operation_id,
                raw,
                config,
                profile,
                model_root,
                cancelled,
                reply,
            } => {
                let result = state
                    .ensure_rewriter(&config, &model_root, &cancelled)
                    .and_then(|rewriter| {
                        let result = rewriter.rewrite(&raw, &config, &cancelled, &profile);
                        record_evidence(&evidence, &operation_id, "rewriter", rewriter.evidence());
                        result
                    });
                let _ = reply.send(result);
            }
            Command::StartStream {
                config,
                profile,
                model_root,
                operation_id,
                cancelled,
                stream_cancelled,
                updates,
                input,
                started,
                completed,
            } => {
                let result = run_stream(
                    &mut state,
                    &config,
                    &model_root,
                    &operation_id,
                    &cancelled,
                    &stream_cancelled,
                    &updates,
                    &profile,
                    &evidence,
                    input,
                    started,
                );
                if result.is_err() {
                    state.frontend.unload();
                    if config.audio_frontend.enabled() {
                        if let Ok(mut value) = evidence.lock() {
                            if value["operationId"] != operation_id {
                                *value = serde_json::json!({"operationId":operation_id});
                            }
                            if !value["audioFrontend"].is_object() {
                                value["audioFrontend"] = serde_json::json!({});
                            }
                            value["audioFrontend"]["streamFailed"] = true.into();
                        }
                    }
                }
                let _ = completed.send(result);
            }
            Command::Unload(reply) => {
                state.unload();
                let _ = reply.send(());
            }
            Command::Shutdown => break,
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn run_stream(
    state: &mut WorkerState,
    config: &AppConfig,
    model_root: &Path,
    operation_id: &str,
    cancelled: &AtomicBool,
    stream_cancelled: &AtomicBool,
    updates: &StreamingUpdateSink,
    profile: &PersonalizationProfile,
    evidence: &Mutex<serde_json::Value>,
    input: mpsc::Receiver<StreamInput>,
    started: mpsc::SyncSender<Result<()>>,
) -> Result<String> {
    if let Err(error) = state.ensure_speech(config, model_root, cancelled) {
        let _ = started.send(Err(error));
        return Err(InferenceError::Cancelled);
    }
    let mut frontend = if config.audio_frontend.enabled() {
        match state.frontend.start(config, model_root, cancelled) {
            Ok(session) => Some(session),
            Err(InferenceError::Cancelled) => {
                let _ = started.send(Err(InferenceError::Cancelled));
                return Err(InferenceError::Cancelled);
            }
            Err(error) => {
                // No audio has been consumed: this session can safely retain
                // its live ASR path using the original capture waveform.
                record_evidence(
                    evidence,
                    operation_id,
                    "audioFrontend",
                    serde_json::json!({"originalAudioFallback":true,
                        "error":error.to_string(),
                        "warning":"Audio processing could not start; transcription used the original recording."}),
                );
                None
            }
        }
    } else {
        state.frontend.unload();
        None
    };
    if let Err(error) = check_cancelled(cancelled) {
        let _ = started.send(Err(error));
        return Err(InferenceError::Cancelled);
    }
    let recognizer = &state.speech.as_ref().expect("initialized speech model").1;
    record_evidence(
        evidence,
        operation_id,
        "asr",
        recognizer.evidence(config, profile),
    );
    let mut stream = match recognizer.stream(config, operation_id, cancelled, profile) {
        Ok(stream) => stream,
        Err(error) => {
            let message = error.to_string();
            let _ = started.send(Err(error));
            return Err(native("NeMo-Speech.cpp streaming start", message));
        }
    };
    if started.send(Ok(())).is_err() {
        return Err(InferenceError::Cancelled);
    }
    let mut sequence = 0_u64;
    let mut final_text = String::new();
    let result = (|| loop {
        check_cancelled(cancelled)?;
        check_cancelled(stream_cancelled)?;
        match input.recv() {
            Ok(StreamInput::Audio {
                samples,
                sample_rate,
            }) => {
                check_cancelled(cancelled)?;
                check_cancelled(stream_cancelled)?;
                let processed = match &mut frontend {
                    Some(session) => Some(session.push(&samples, sample_rate, cancelled)?),
                    None => None,
                };
                let (samples, sample_rate) = processed
                    .as_ref()
                    .map_or((samples.as_slice(), sample_rate), |p| (p.as_slice(), 16000));
                stream
                    .push(samples, sample_rate)
                    .map_err(|error| native("NeMo-Speech.cpp streaming audio", error))?;
                check_cancelled(cancelled)?;
                check_cancelled(stream_cancelled)?;
                drain_results(
                    &mut stream,
                    operation_id,
                    updates,
                    &mut sequence,
                    &mut final_text,
                    cancelled,
                    stream_cancelled,
                )?;
            }
            Ok(StreamInput::Finish) => {
                check_cancelled(cancelled)?;
                check_cancelled(stream_cancelled)?;
                if let Some(session) = &mut frontend {
                    let tail = session.finish(cancelled)?;
                    check_cancelled(stream_cancelled)?;
                    if !tail.is_empty() {
                        stream
                            .push(&tail, 16000)
                            .map_err(|e| native("streaming frontend tail", e))?;
                    }
                }
                stream
                    .finish()
                    .map_err(|error| native("NeMo-Speech.cpp streaming finish", error))?;
                check_cancelled(cancelled)?;
                check_cancelled(stream_cancelled)?;
                drain_results(
                    &mut stream,
                    operation_id,
                    updates,
                    &mut sequence,
                    &mut final_text,
                    cancelled,
                    stream_cancelled,
                )?;
                check_cancelled(cancelled)?;
                check_cancelled(stream_cancelled)?;
                return Ok(final_text.trim().to_owned());
            }
            Ok(StreamInput::Cancel) | Err(_) => return Err(InferenceError::Cancelled),
        }
    })();
    if let Some(session) = &frontend {
        record_evidence(
            evidence,
            operation_id,
            "audioFrontend",
            session.metrics().evidence(),
        );
    }
    result
}

fn drain_results(
    stream: &mut transcribe_nemo::Stream<'_>,
    operation_id: &str,
    updates: &StreamingUpdateSink,
    sequence: &mut u64,
    final_text: &mut String,
    cancelled: &AtomicBool,
    stream_cancelled: &AtomicBool,
) -> Result<()> {
    loop {
        check_cancelled(cancelled)?;
        check_cancelled(stream_cancelled)?;
        let result = stream
            .poll_result()
            .map_err(|error| native("NeMo-Speech.cpp streaming decode", error))?;
        check_cancelled(cancelled)?;
        check_cancelled(stream_cancelled)?;
        let Some(result) = result else { break };
        *sequence += 1;
        if result.is_final {
            *final_text = result.text.clone();
        }
        updates(StreamingTranscript {
            operation_id: operation_id.to_owned(),
            sequence: *sequence,
            committed_text: if result.is_final {
                result.text.clone()
            } else {
                String::new()
            },
            provisional_text: if result.is_final {
                String::new()
            } else {
                result.text
            },
            audio_processed_seconds: result.audio_processed_seconds,
            emitted_at_unix_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            is_final: result.is_final,
        });
        check_cancelled(cancelled)?;
        check_cancelled(stream_cancelled)?;
    }
    Ok(())
}

fn worker_unavailable(error: impl std::fmt::Display) -> InferenceError {
    native("inference worker", error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancelled_work_cannot_evict_a_selected_speech_model_or_load_a_writer() {
        let root = Path::new("missing-models");
        let mut original = AppConfig::default();
        original.models.asr = transcribe_core::AsrModel::ParakeetTdt06bV3;
        let mut state = WorkerState::new();
        state.speech = Some((
            adapters::speech_key(&original, root).unwrap(),
            adapters::SpeechModel::PerJob(transcribe_core::AdapterKind::SherpaTransducer),
        ));
        let cancelled = AtomicBool::new(true);
        let different = AppConfig::default();
        assert!(matches!(
            state.ensure_speech(&different, root, &cancelled),
            Err(InferenceError::Cancelled)
        ));
        assert!(state.speech.as_ref().unwrap().0 == adapters::speech_key(&original, root).unwrap());
        assert!(matches!(
            state.ensure_rewriter(&different, root, &cancelled),
            Err(InferenceError::Cancelled)
        ));
        assert!(state.rewriter.is_none());
    }

    #[test]
    fn cancelled_submissions_never_enter_the_native_worker() {
        let manager = InferenceManager::new().unwrap();
        let config = AppConfig::default();
        let audio = MonoPcm::new(vec![0.25; 320], 16000).unwrap();
        let flag = Arc::new(AtomicBool::new(true));
        let root = Path::new("missing-models");
        assert!(matches!(
            manager.transcribe(&audio, &config, root, "cancelled", Arc::clone(&flag)),
            Err(InferenceError::Cancelled)
        ));
        assert!(matches!(
            manager.rewrite("hello", &config, root, Arc::clone(&flag)),
            Err(InferenceError::Cancelled)
        ));
        assert!(matches!(
            manager.start_stream(&config, root, "cancelled", flag, Arc::new(|_| {})),
            Err(InferenceError::Cancelled)
        ));
        assert_eq!(manager.latest_execution_evidence(), serde_json::json!({}));
    }

    #[test]
    fn stream_cancel_and_drop_signal_even_when_the_input_queue_is_full() {
        for explicit in [false, true] {
            let (sender, queued) = mpsc::sync_channel(STREAM_QUEUE);
            for _ in 0..STREAM_QUEUE {
                sender
                    .send(StreamInput::Audio {
                        samples: vec![0.0; 320],
                        sample_rate: 16000,
                    })
                    .unwrap();
            }
            let (completed, completion) = mpsc::sync_channel(1);
            completed.send(Err(InferenceError::Cancelled)).unwrap();
            let cancelled = Arc::new(AtomicBool::new(false));
            let session = StreamingSession {
                sender,
                completion: Some(completion),
                finished: false,
                cancelled: Arc::clone(&cancelled),
            };
            if explicit {
                session.cancel().unwrap();
            } else {
                drop(session);
            }
            assert!(cancelled.load(Ordering::Acquire));
            // Cancellation needs no free queue slot and does not first consume audio.
            assert_eq!(queued.into_iter().count(), STREAM_QUEUE);
        }
    }

    #[test]
    fn streaming_event_serializes_stable_contract() {
        let event = StreamingTranscript {
            operation_id: "one".into(),
            sequence: 2,
            committed_text: "hello".into(),
            provisional_text: " world".into(),
            audio_processed_seconds: 1.25,
            emitted_at_unix_ms: 3,
            is_final: false,
        };
        let json = serde_json::to_value(event).unwrap();
        assert_eq!(json["operationId"], "one");
        assert_eq!(json["audioProcessedSeconds"], 1.25);
        assert_eq!(json["isFinal"], false);
    }

    #[test]
    fn frontend_failure_preserves_original_and_never_swallows_cancellation() {
        let root = tempfile::tempdir().unwrap();
        let config = AppConfig {
            audio_frontend: transcribe_core::AudioFrontendConfig {
                vad_enabled: true,
                ..Default::default()
            },
            ..Default::default()
        };
        let audio = MonoPcm::new(vec![0.01, -0.3, 0.5], 16000).unwrap();
        let mut cache = crate::audio_frontend::FrontendCache::default();
        let evidence = Mutex::new(serde_json::json!({}));
        let cancelled = AtomicBool::new(false);
        assert!(prepare_audio(
            &mut cache,
            &audio,
            &config,
            root.path(),
            "missing-model",
            &cancelled,
            &evidence
        )
        .unwrap()
        .is_none());
        assert_eq!(audio.samples(), [0.01, -0.3, 0.5]);
        assert_eq!(
            evidence.lock().unwrap()["audioFrontend"]["originalAudioFallback"],
            true
        );
        cancelled.store(true, Ordering::Release);
        assert!(matches!(
            prepare_audio(
                &mut cache,
                &audio,
                &config,
                root.path(),
                "cancelled",
                &cancelled,
                &evidence
            ),
            Err(InferenceError::Cancelled)
        ));
        cancelled.store(false, Ordering::Release);
        *evidence.lock().unwrap() =
            serde_json::json!({"operationId":"failed-live", "audioFrontend":{"streamFailed":true}});
        assert!(prepare_audio(
            &mut cache,
            &audio,
            &config,
            root.path(),
            "failed-live",
            &cancelled,
            &evidence
        )
        .unwrap()
        .is_none());
        assert_eq!(
            evidence.lock().unwrap()["audioFrontend"]["originalAudioFallback"],
            true
        );
    }

    #[test]
    fn cancelling_an_idle_stream_wakes_its_waiting_receiver() {
        for explicit in [false, true] {
            let (sender, input) = mpsc::sync_channel(1);
            let (completed, completion) = mpsc::sync_channel(1);
            let cancelled = Arc::new(AtomicBool::new(false));
            let observed = Arc::clone(&cancelled);
            let receiver = thread::spawn(move || {
                assert!(matches!(
                    input.recv_timeout(std::time::Duration::from_secs(1)),
                    Ok(StreamInput::Cancel)
                ));
                assert!(observed.load(Ordering::Acquire));
                let _ = completed.send(Err(InferenceError::Cancelled));
            });
            let session = StreamingSession {
                sender,
                completion: Some(completion),
                finished: false,
                cancelled,
            };
            if explicit {
                session.cancel().unwrap();
            } else {
                drop(session);
            }
            receiver.join().unwrap();
        }
    }

    #[test]
    fn successful_finish_preserves_audio_order_without_cancelling_on_drop() {
        let (sender, input) = mpsc::sync_channel(3);
        let (completed, completion) = mpsc::sync_channel(1);
        let cancelled = Arc::new(AtomicBool::new(false));
        let observed = Arc::clone(&cancelled);
        let receiver = thread::spawn(move || {
            for expected in [0.25, 0.5] {
                match input
                    .recv_timeout(std::time::Duration::from_secs(1))
                    .unwrap()
                {
                    StreamInput::Audio {
                        samples,
                        sample_rate,
                    } => {
                        assert_eq!(samples, [expected]);
                        assert_eq!(sample_rate, 16000);
                    }
                    _ => panic!("audio must precede finalization"),
                }
            }
            assert!(matches!(
                input.recv_timeout(std::time::Duration::from_secs(1)),
                Ok(StreamInput::Finish)
            ));
            assert!(!observed.load(Ordering::Acquire));
            completed.send(Ok("complete".into())).unwrap();
        });
        let session = StreamingSession {
            sender,
            completion: Some(completion),
            finished: false,
            cancelled: Arc::clone(&cancelled),
        };
        session.push(vec![0.25], 16000).unwrap();
        session.push(vec![0.5], 16000).unwrap();
        assert_eq!(session.finish().unwrap(), "complete");
        receiver.join().unwrap();
        assert!(!cancelled.load(Ordering::Acquire));
    }

    #[test]
    #[ignore = "requires pinned Nemotron CPU and frontend models, and public speech"]
    fn live_frontend_cancel_fallback_and_reset_isolation() {
        let root = PathBuf::from(std::env::var_os("TRANSCRIBE_SMOKE_MODELS").expect("models"));
        let path = PathBuf::from(std::env::var_os("TRANSCRIBE_SMOKE_AUDIO").expect("public audio"));
        let audio = transcribe_audio::decode_file(path, 0).unwrap();
        let manager = InferenceManager::new().unwrap();
        let mut config = AppConfig::default();
        config.models.asr = transcribe_core::AsrModel::Nemotron35AsrStreaming06b;
        config.compute_backend = transcribe_core::ComputeBackend::Cpu;
        config.cleanup_enabled = false;
        config.audio_frontend.vad_enabled = true;
        for denoiser in [
            transcribe_core::NoiseReduction::Off,
            transcribe_core::NoiseReduction::Dpdfnet4,
            transcribe_core::NoiseReduction::Dpdfnet8,
        ] {
            config.audio_frontend.noise_reduction = denoiser;
            let run = |id: &str| {
                let session = manager
                    .start_stream(
                        &config,
                        &root,
                        id,
                        Arc::new(AtomicBool::new(false)),
                        Arc::new(|_| {}),
                    )
                    .unwrap();
                for block in audio.samples().chunks(320) {
                    session.push(block.to_vec(), 16000).unwrap();
                }
                let result = session.finish().unwrap();
                let evidence = manager.execution_evidence(id);
                assert_eq!(
                    evidence["audioFrontend"]["metrics"]["outputSamples16k"],
                    audio.samples().len()
                );
                assert_eq!(evidence["audioFrontend"]["metrics"]["finalized"], true);
                result
            };
            let a = run("first-a");
            assert!(!a.is_empty());
            let cancelled = Arc::new(AtomicBool::new(false));
            let b = manager
                .start_stream(
                    &config,
                    &root,
                    "interrupted-b",
                    Arc::clone(&cancelled),
                    Arc::new(|_| {}),
                )
                .unwrap();
            b.push(vec![0.01; 320], 16000).unwrap();
            b.cancel().unwrap();
            assert!(!cancelled.load(Ordering::Acquire));
            let fallback = manager
                .transcribe(&audio, &config, &root, "interrupted-b", cancelled)
                .unwrap();
            assert!(!fallback.is_empty());
            assert_eq!(
                manager.execution_evidence("interrupted-b")["audioFrontend"]
                    ["originalAudioFallback"],
                true
            );
            assert_eq!(run("reset-a"), a);
            manager.unload().unwrap();
            assert_eq!(run("reload-a"), a);
        }
    }

    #[test]
    #[cfg(unix)]
    #[ignore = "requires pinned Nemotron CPU models and public speech"]
    fn live_frontend_load_failure_uses_original_audio_and_retains_asr_errors() {
        fn link_tree(source: &Path, target: &Path) {
            std::fs::create_dir(target).unwrap();
            for entry in std::fs::read_dir(source).unwrap() {
                let entry = entry.unwrap();
                let target = target.join(entry.file_name());
                if entry.path().is_dir() {
                    link_tree(&entry.path(), &target);
                } else {
                    // No descendant symlinks: production intentionally rejects
                    // them. These read-only hard links avoid duplicating weights.
                    std::fs::hard_link(entry.path().canonicalize().unwrap(), target).unwrap();
                }
            }
        }
        let source = PathBuf::from(std::env::var_os("TRANSCRIBE_SMOKE_MODELS").expect("models"))
            .canonicalize()
            .unwrap();
        let path = PathBuf::from(std::env::var_os("TRANSCRIBE_SMOKE_AUDIO").expect("audio"));
        let audio = transcribe_audio::decode_file(path, 0).unwrap();
        let root = tempfile::tempdir().unwrap();
        for name in ["nemotron-3.5-asr-streaming-0.6b", "nemo-speech-runtime"] {
            link_tree(&source.join(name), &root.path().join(name));
        }
        let mut config = AppConfig::default();
        config.models.asr = transcribe_core::AsrModel::Nemotron35AsrStreaming06b;
        config.compute_backend = transcribe_core::ComputeBackend::Cpu;
        config.cleanup_enabled = false;
        let manager = InferenceManager::new().unwrap();
        let run = |config: &AppConfig, id: &str| {
            let session = manager
                .start_stream(
                    config,
                    root.path(),
                    id,
                    Arc::new(AtomicBool::new(false)),
                    Arc::new(|_| {}),
                )
                .unwrap();
            for block in audio.samples().chunks(320) {
                session.push(block.to_vec(), 16000).unwrap();
            }
            session.finish().unwrap()
        };
        let original = run(&config, "original");
        assert!(!original.is_empty());
        config.audio_frontend.vad_enabled = true;
        assert!(transcribe_models::inspect(&config, root.path()).ready);
        assert_eq!(run(&config, "missing-frontend"), original);
        assert_eq!(
            manager.execution_evidence("missing-frontend")["audioFrontend"]
                ["originalAudioFallback"],
            true
        );
        let frontend_dir = root.path().join("audio-frontends");
        std::fs::create_dir(&frontend_dir).unwrap();
        // Correct length but wrong hash must be rejected before native loading.
        std::fs::File::create(frontend_dir.join("silero_v6.2_16k_op15.onnx"))
            .unwrap()
            .set_len(1_289_603)
            .unwrap();
        assert_eq!(run(&config, "corrupt-frontend"), original);
        let evidence = manager.execution_evidence("corrupt-frontend");
        assert_eq!(evidence["audioFrontend"]["originalAudioFallback"], true);
        assert!(evidence["audioFrontend"]["warning"].is_string());
        assert!(matches!(
            manager.start_stream(
                &config,
                root.path(),
                "cancelled",
                Arc::new(AtomicBool::new(true)),
                Arc::new(|_| {})
            ),
            Err(InferenceError::Cancelled)
        ));
        let no_models = tempfile::tempdir().unwrap();
        assert!(manager
            .start_stream(
                &config,
                no_models.path(),
                "missing-asr",
                Arc::new(AtomicBool::new(false)),
                Arc::new(|_| {})
            )
            .is_err());
    }

    #[test]
    #[ignore = "requires pinned Nemotron CPU/frontend models and public speech"]
    fn live_frontend_device_rates_match_completed_processing() {
        let root = PathBuf::from(std::env::var_os("TRANSCRIBE_SMOKE_MODELS").expect("models"));
        let path = PathBuf::from(std::env::var_os("TRANSCRIBE_SMOKE_AUDIO").expect("audio"));
        let audio = transcribe_audio::decode_file(path, 0).unwrap();
        let manager = InferenceManager::new().unwrap();
        let mut config = AppConfig::default();
        config.models.asr = transcribe_core::AsrModel::Nemotron35AsrStreaming06b;
        config.compute_backend = transcribe_core::ComputeBackend::Cpu;
        config.cleanup_enabled = false;
        config.audio_frontend.vad_enabled = true;
        let mut cache = crate::audio_frontend::FrontendCache::default();
        let flag = AtomicBool::new(false);
        for rate in [44100, 48000] {
            // Deterministic synthetic upsampling of public speech. This checks
            // application conversion/packet contracts, not microphone quality.
            let length = (audio.samples().len() * rate as usize).div_ceil(16000);
            let samples = (0..length)
                .map(|i| {
                    audio.samples()[(i * 16000 / rate as usize).min(audio.samples().len() - 1)]
                })
                .collect::<Vec<_>>();
            let device_audio = MonoPcm::new(samples, rate).unwrap();
            for denoiser in [
                transcribe_core::NoiseReduction::Off,
                transcribe_core::NoiseReduction::Dpdfnet4,
                transcribe_core::NoiseReduction::Dpdfnet8,
            ] {
                config.audio_frontend.noise_reduction = denoiser;
                let (canonical, _) = crate::audio_frontend::process_completed(
                    &mut cache,
                    &device_audio,
                    &config,
                    &root,
                    &flag,
                )
                .unwrap();
                let mut bypass = config.clone();
                bypass.audio_frontend = Default::default();
                let expected = manager
                    .start_stream(
                        &bypass,
                        &root,
                        "canonical-reference",
                        Arc::new(AtomicBool::new(false)),
                        Arc::new(|_| {}),
                    )
                    .unwrap();
                for block in canonical.samples().chunks(320) {
                    expected.push(block.to_vec(), 16000).unwrap();
                }
                let expected = expected.finish().unwrap();
                assert!(!expected.is_empty());
                let actual = manager
                    .start_stream(
                        &config,
                        &root,
                        "device-rate-stream",
                        Arc::new(AtomicBool::new(false)),
                        Arc::new(|_| {}),
                    )
                    .unwrap();
                let mut offset = 0;
                for size in [1, 319, 1053, 2000].into_iter().cycle() {
                    if offset == length {
                        break;
                    }
                    let end = (offset + size).min(length);
                    actual
                        .push(device_audio.samples()[offset..end].to_vec(), rate)
                        .unwrap();
                    offset = end;
                }
                assert_eq!(actual.finish().unwrap(), expected, "{rate}Hz {denoiser:?}");
                let metrics =
                    &manager.execution_evidence("device-rate-stream")["audioFrontend"]["metrics"];
                assert_eq!(metrics["outputSamples16k"], canonical.samples().len());
                assert_eq!(metrics["inputSamples16k"], canonical.samples().len());
                assert_eq!(metrics["finalized"], true);
            }
        }
    }

    /// Real-runtime lifecycle smoke: one resident recognizer, isolated stream
    /// state and final-tail draining across consecutive utterances.
    #[test]
    #[ignore = "requires downloaded Nemotron files and the public speech fixture"]
    fn real_nemotron_stream_reuses_weights_and_resets_utterance_state() {
        let model_root = PathBuf::from(
            std::env::var_os("TRANSCRIBE_SMOKE_MODELS").expect("TRANSCRIBE_SMOKE_MODELS"),
        );
        let audio_path = PathBuf::from(
            std::env::var_os("TRANSCRIBE_SMOKE_AUDIO").expect("TRANSCRIBE_SMOKE_AUDIO"),
        );
        let audio = transcribe_audio::decode_file(&audio_path, 30).unwrap();
        let manager = InferenceManager::new().unwrap();
        let mut config = AppConfig::default();
        config.models.asr = transcribe_core::AsrModel::Nemotron35AsrStreaming06b;
        config.compute_backend = transcribe_core::ComputeBackend::Cpu;
        let expected =
            "Ask not what your country can do for you. Ask what you can do for your country.";

        for pass in 0..3 {
            let profile = if pass == 1 {
                PersonalizationProfile {
                    revision: 1,
                    speech_bias_enabled: true,
                    vocabulary: vec![transcribe_core::VocabularyEntry {
                        id: "fixture-country".into(),
                        phrase: "country".into(),
                        language: transcribe_core::Language::Auto,
                        context: String::new(),
                        enabled: true,
                        ..transcribe_core::VocabularyEntry::default()
                    }],
                    ..Default::default()
                }
            } else {
                PersonalizationProfile::default()
            };
            let operation_id = format!("stream-{pass}");
            let events = Arc::new(Mutex::new(Vec::<StreamingTranscript>::new()));
            let event_copy = Arc::clone(&events);
            let updates: StreamingUpdateSink = Arc::new(move |update| {
                event_copy.lock().unwrap().push(update);
            });
            let session = manager
                .start_stream_personalized(
                    &config,
                    &model_root,
                    &operation_id,
                    Arc::new(AtomicBool::new(false)),
                    updates,
                    &profile,
                )
                .unwrap();
            for chunk in audio.samples().chunks(320) {
                session.push(chunk.to_vec(), audio.sample_rate()).unwrap();
            }
            assert_eq!(session.finish().unwrap(), expected);
            let evidence = manager.execution_evidence(&operation_id);
            assert_eq!(
                evidence["asr"]["contextualPhraseCount"],
                if pass == 1 { 1 } else { 0 }
            );
            assert_eq!(evidence["asr"]["profileRevision"], profile.revision);

            let events = events.lock().unwrap();
            assert!(!events.is_empty());
            assert!(events
                .iter()
                .all(|event| event.operation_id == operation_id));
            assert!(events
                .windows(2)
                .all(|pair| pair[0].sequence < pair[1].sequence));
            assert!(
                events
                    .iter()
                    .any(|event| { !event.is_final && !event.provisional_text.trim().is_empty() }),
                "streaming must emit visible text before finish/Stop"
            );
            assert!(events.last().is_some_and(|event| event.is_final));
            assert_eq!(events.last().unwrap().committed_text, expected);
        }
        let operation_cancelled = Arc::new(AtomicBool::new(false));
        let partial = manager
            .start_stream(
                &config,
                &model_root,
                "cancel-mid-stream",
                Arc::clone(&operation_cancelled),
                Arc::new(|_| {}),
            )
            .unwrap();
        for chunk in audio.samples()[..audio.samples().len() / 2].chunks(320) {
            partial.push(chunk.to_vec(), audio.sample_rate()).unwrap();
        }
        partial.cancel().unwrap();
        assert!(
            !operation_cancelled.load(Ordering::Acquire),
            "stream-only cancellation must preserve complete-audio fallback"
        );
        // Switching from a cancelled stream to a complete-file request reuses
        // weights, but cannot inherit its decoder state or contextual phrases.
        assert_eq!(
            manager
                .transcribe(
                    &audio,
                    &config,
                    &model_root,
                    "offline-after-cancel",
                    operation_cancelled
                )
                .unwrap(),
            expected
        );
        assert_eq!(
            manager.execution_evidence("offline-after-cancel")["asr"]["contextualPhraseCount"],
            0
        );
        assert_eq!(
            manager.execution_evidence("cancel-mid-stream"),
            serde_json::json!({})
        );
        let callback_cancel = Arc::new(AtomicBool::new(false));
        let signal = Arc::clone(&callback_cancel);
        let final_cancel = manager
            .start_stream(
                &config,
                &model_root,
                "cancel-final-callback",
                callback_cancel,
                Arc::new(move |event| {
                    if event.is_final {
                        signal.store(true, Ordering::Release);
                    }
                }),
            )
            .unwrap();
        for chunk in audio.samples().chunks(320) {
            final_cancel
                .push(chunk.to_vec(), audio.sample_rate())
                .unwrap();
        }
        assert!(matches!(
            final_cancel.finish(),
            Err(InferenceError::Cancelled)
        ));
        assert_eq!(
            manager
                .transcribe(
                    &audio,
                    &config,
                    &model_root,
                    "after-final-cancel",
                    Arc::new(AtomicBool::new(false))
                )
                .unwrap(),
            expected
        );
    }
}
