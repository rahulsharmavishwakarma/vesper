//! LFM2.5 Audio ASR via pinned llama.cpp/mtmd. See ADR-0013 and issue #40.
//! Frozen resident weights, fresh hybrid state for every completed-audio segment.
use crate::{
    check_cancelled, llama_runtime, native, path_string, require_file, segmented_audio,
    worker_threads, InferenceError, Result, TranscriptionProgress,
};
use llama_cpp_2::{
    context::params::LlamaContextParams,
    llama_backend::LlamaBackend,
    llama_batch::LlamaBatch,
    model::{AddBos, LlamaModel},
    mtmd::{mtmd_default_marker, MtmdBitmap, MtmdContext, MtmdContextParams, MtmdInputText},
    sampling::LlamaSampler,
    token::LlamaToken,
    token_type::LlamaTokenAttr,
};
use serde::Serialize;
use std::{
    cell::RefCell,
    num::NonZeroU32,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
    time::{Instant, SystemTime},
};
use transcribe_audio::{MonoPcm, ASR_SAMPLE_RATE};
use transcribe_core::{AdapterKind, AppConfig, ComputeBackend};

pub(crate) const REVISION: &str = "7d525f883a077e20afb782f2ff618edcae0e39e4";
pub(crate) const MODEL_FILE: &str = "LFM2.5-Audio-1.5B-Q4_0.gguf";
pub(crate) const PROJECTOR_FILE: &str = "mmproj-LFM2.5-Audio-1.5B-Q4_0.gguf";
pub(crate) const MAX_SEGMENT_SECONDS: usize = segmented_audio::MAX_SEGMENT_SECONDS;
pub(crate) const CONTEXT_TOKENS: u32 = 2048;
pub(crate) const MAX_OUTPUT_TOKENS: usize = 512;
const BATCH_TOKENS: u32 = 512;
const END_TURN: &str = "<|im_end|>";

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct RecognizerKey {
    model: PathBuf,
    projector: PathBuf,
    requested_backend: ComputeBackend,
    // Detect replacement/removal at the same installation path without hashing
    // weights on each warm request. Loading still performs full verification.
    stamps: [Option<(u64, SystemTime)>; 2],
}

pub(crate) fn key(config: &AppConfig, root: &Path) -> Result<RecognizerKey> {
    config.validate()?;
    if config.speech_adapter().kind != AdapterKind::LfmAudio {
        return Err(InferenceError::UnsupportedAsr(config.models.asr.id()));
    }
    let model = root.join("lfm2.5-audio-1.5b").join(MODEL_FILE);
    let projector = root.join("lfm2.5-audio-1.5b").join(PROJECTOR_FILE);
    let stamp = |path: &Path| {
        let meta = std::fs::metadata(path).ok()?;
        Some((meta.len(), meta.modified().ok()?))
    };
    Ok(RecognizerKey {
        stamps: [stamp(&model), stamp(&projector)],
        model,
        projector,
        requested_backend: config.compute_backend,
    })
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Metrics {
    pub segments: usize,
    pub completed_segments: usize,
    pub input_positions: usize,
    pub generated_tokens: usize,
    pub frontend_us: u64,
    pub context_us: u64,
    /// The safe mtmd helper combines audio encoding and LM prefill.
    pub encode_prefill_us: u64,
    pub generation_us: u64,
    pub total_us: u64,
}

pub(crate) struct Recognizer {
    // Declaration order is significant: mtmd retains the model vocabulary.
    // Drop the projector BEFORE the backbone. Contexts below are scoped locals.
    projector: MtmdContext,
    model: LlamaModel,
    backend: &'static LlamaBackend,
    end_turn: LlamaToken,
    pub(crate) load_us: u64,
    pub(crate) metrics: RefCell<Metrics>,
    pub(crate) plan: crate::backend::Plan,
    pub(crate) fallback_reason: Option<String>,
}

fn micros(start: Instant) -> u64 {
    start.elapsed().as_micros().min(u64::MAX as u128) as u64
}

fn asr_prompt() -> MtmdInputText {
    // Official liquid-audio ChatState, not the text-only GGUF chat template.
    MtmdInputText {
        text: format!(
            "<|startoftext|><|im_start|>system\nPerform ASR.<|im_end|>\n<|im_start|>user\n{}<|im_end|>\n<|im_start|>assistant\n",
            mtmd_default_marker()
        ),
        add_special: false,
        parse_special: true,
    }
}

fn validate_budget(positions: i32, context: u32, output: usize) -> Result<()> {
    if positions <= 0 || positions as u64 + output as u64 > u64::from(context) {
        return Err(native(
            "LFM context budget",
            "audio and output exceed the model context; incomplete transcription was discarded",
        ));
    }
    Ok(())
}

fn validate_logits(logits: &[f32]) -> Result<()> {
    if !logits.iter().any(|x| x.is_finite())
        || logits.iter().any(|x| x.is_nan() || *x == f32::INFINITY)
    {
        return Err(native(
            "LFM sampling",
            "invalid logits; incomplete transcription was discarded",
        ));
    }
    Ok(())
}

impl Recognizer {
    pub(crate) fn load(config: &AppConfig, root: &Path, cancelled: &AtomicBool) -> Result<Self> {
        check_cancelled(cancelled)?;
        let key = key(config, root)?;
        require_file(&key.model)?;
        require_file(&key.projector)?;
        let started = Instant::now();
        let backend = llama_runtime::backend()?;
        let requested = crate::backend::resolve(config.compute_backend)?;
        let (mut recognizer, _, fallback_reason) =
            crate::backend::load(config.compute_backend, requested, cancelled, |plan| {
                Self::load_on(&key, backend, plan, cancelled)
            })?;
        recognizer.load_us = micros(started);
        recognizer.fallback_reason = fallback_reason;
        Ok(recognizer)
    }

    fn load_on(
        key: &RecognizerKey,
        backend: &'static LlamaBackend,
        plan: crate::backend::Plan,
        cancelled: &AtomicBool,
    ) -> Result<Self> {
        let parameters = plan.model_params()?;
        let model = LlamaModel::load_from_file(backend, &key.model, &parameters)
            .map_err(|e| native("LFM backbone load", e))?;
        if model.meta_val_str("general.architecture").ok().as_deref() != Some("lfm2") {
            return Err(native(
                "LFM backbone load",
                "expected the pinned LFM2 audio backbone",
            ));
        }
        check_cancelled(cancelled)?;
        let projector = MtmdContext::init_from_file(
            &path_string(&key.projector)?,
            &model,
            &MtmdContextParams {
                use_gpu: plan.device.is_some(),
                n_threads: worker_threads(),
                print_timings: false,
                ..Default::default()
            },
        )
        .map_err(|e| native("LFM audio projector load", e))?;
        if !projector.support_audio() || projector.get_audio_sample_rate() != Some(ASR_SAMPLE_RATE)
        {
            return Err(native(
                "LFM audio projector load",
                "expected a 16 kHz audio projector",
            ));
        }
        let end_tokens = model
            .str_to_token(END_TURN, AddBos::Never)
            .map_err(|e| native("LFM end token", e))?;
        let [end_turn] = end_tokens.as_slice() else {
            return Err(native(
                "LFM end token",
                "expected an atomic end-of-turn token",
            ));
        };
        check_cancelled(cancelled)?;
        let recognizer = Self {
            projector,
            model,
            backend,
            end_turn: *end_turn,
            load_us: 0,
            metrics: RefCell::default(),
            plan,
            fallback_reason: None,
        };
        // Include context allocation in the automatic fallback boundary. The
        // actual audio graph is checked by native qualification, not this probe.
        recognizer
            .model
            .new_context(backend, recognizer.context_parameters())
            .map_err(|error| native("LFM context probe", error))?;
        Ok(recognizer)
    }

    fn context_parameters(&self) -> LlamaContextParams {
        LlamaContextParams::default()
            .with_n_ctx(NonZeroU32::new(
                CONTEXT_TOKENS.min(self.model.n_ctx_train()),
            ))
            .with_n_batch(BATCH_TOKENS)
            .with_n_ubatch(128)
            .with_offload_kqv(self.plan.device.is_some())
            .with_op_offload(self.plan.device.is_some())
            .with_n_threads(worker_threads())
            .with_n_threads_batch(worker_threads())
    }

    pub(crate) fn transcribe(
        &self,
        audio: &MonoPcm,
        cancelled: &AtomicBool,
        progress: &dyn Fn(TranscriptionProgress),
    ) -> Result<String> {
        *self.metrics.borrow_mut() = Metrics::default();
        check_cancelled(cancelled)?;
        audio.require_sample_rate(ASR_SAMPLE_RATE)?;
        let started = Instant::now();
        let result = segmented_audio::decode_segments(
            audio.samples(),
            cancelled,
            |samples| self.decode(samples, cancelled),
            progress,
        );
        self.metrics.borrow_mut().total_us = micros(started);
        result
    }

    fn decode(&self, samples: &[f32], cancelled: &AtomicBool) -> Result<String> {
        self.decode_with_limit(samples, cancelled, MAX_OUTPUT_TOKENS)
    }

    fn decode_with_limit(
        &self,
        samples: &[f32],
        cancelled: &AtomicBool,
        output_limit: usize,
    ) -> Result<String> {
        check_cancelled(cancelled)?;
        self.metrics.borrow_mut().segments += 1;
        let started = Instant::now();
        let bitmap =
            MtmdBitmap::from_audio_data(samples).map_err(|e| native("LFM audio input", e))?;
        let chunks = self
            .projector
            .tokenize(asr_prompt(), &[&bitmap])
            .map_err(|e| native("LFM audio frontend", e))?;
        self.metrics.borrow_mut().frontend_us += micros(started);
        check_cancelled(cancelled)?;
        let positions = chunks.total_positions();
        let context_tokens = CONTEXT_TOKENS.min(self.model.n_ctx_train());
        validate_budget(positions, context_tokens, output_limit)?;
        self.metrics.borrow_mut().input_positions += positions as usize;
        let started = Instant::now();
        let params = self.context_parameters();
        let mut context = self
            .model
            .new_context(self.backend, params)
            .map_err(|e| native("LFM context creation", e))?;
        self.metrics.borrow_mut().context_us += micros(started);
        check_cancelled(cancelled)?;
        let started = Instant::now();
        let position = chunks
            .eval_chunks(&self.projector, &context, 0, 0, BATCH_TOKENS as i32, true)
            .map_err(|e| native("LFM audio encode and prefill", e))?;
        // Fetching logits synchronizes evaluation. mtmd bypasses the Rust
        // initialized_logits bookkeeping: get_logits_ith would panic here.
        validate_logits(context.get_logits())?;
        self.metrics.borrow_mut().encode_prefill_us += micros(started);
        check_cancelled(cancelled)?;
        if position != positions {
            return Err(native(
                "LFM prefill",
                "unexpected evaluated audio position count",
            ));
        }
        let started = Instant::now();
        let prior_generation_us = self.metrics.borrow().generation_us;
        let mut sampler = LlamaSampler::greedy();
        let mut decoder = encoding_rs::UTF_8.new_decoder();
        let mut batch = LlamaBatch::new(1, 1);
        let mut text = String::new();
        for position in (position..).take(output_limit) {
            check_cancelled(cancelled)?;
            validate_logits(context.get_logits())?;
            // sample() already accepts the token in this pinned wrapper.
            let token = sampler.sample(&context, -1);
            self.metrics.borrow_mut().generated_tokens += 1;
            self.metrics.borrow_mut().generation_us = prior_generation_us + micros(started);
            if token == self.end_turn {
                check_cancelled(cancelled)?;
                self.metrics.borrow_mut().completed_segments += 1;
                return Ok(text.trim().to_owned());
            }
            if self.model.is_eog_token(token)
                || self
                    .model
                    .token_attr(token)
                    .contains(LlamaTokenAttr::Control)
            {
                return Err(native(
                    "LFM output",
                    "unexpected control or audio token; incomplete transcription was discarded",
                ));
            }
            text.push_str(
                &self
                    .model
                    .token_to_piece(token, &mut decoder, true, None)
                    .map_err(|e| native("LFM text decoding", e))?,
            );
            batch.clear();
            batch
                .add(token, position, &[0], true)
                .map_err(|e| native("LFM token batch", e))?;
            context
                .decode(&mut batch)
                .map_err(|e| native("LFM token evaluation", e))?;
        }
        Err(native(
            "LFM output limit",
            "model did not finish; incomplete transcription was discarded",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> AppConfig {
        let mut config = AppConfig::default();
        config.models.asr = transcribe_core::AsrModel::Lfm25Audio15b;
        config.compute_backend = ComputeBackend::Cpu;
        config
    }

    #[test]
    fn missing_projector_and_changed_installation_invalidate_residency() {
        let root = tempfile::tempdir().unwrap();
        let config = config();
        let first = key(&config, root.path()).unwrap();
        std::fs::create_dir_all(first.model.parent().unwrap()).unwrap();
        std::fs::write(&first.model, b"placeholder").unwrap();
        let second = key(&config, root.path()).unwrap();
        assert_ne!(first, second);
        assert!(
            matches!(Recognizer::load(&config, root.path(), &AtomicBool::new(false)), Err(InferenceError::MissingModel(path)) if path == second.projector)
        );
        std::fs::write(&second.projector, b"projector").unwrap();
        assert_ne!(second, key(&config, root.path()).unwrap());
        let mut changed = config.clone();
        changed.compute_backend = ComputeBackend::Auto;
        assert_ne!(
            key(&changed, root.path()).unwrap(),
            key(&config, root.path()).unwrap()
        );
    }

    #[test]
    #[ignore = "requires pinned LFM/Qwen weights and two public audio fixtures; no downloads"]
    fn installed_lfm_reset_cancel_limits_and_two_stage_pipeline() {
        use std::sync::{atomic::Ordering, Arc};
        let root = PathBuf::from(
            std::env::var_os("TRANSCRIBE_SMOKE_MODELS").expect("TRANSCRIBE_SMOKE_MODELS"),
        );
        let read = |name| {
            transcribe_audio::decode_file(PathBuf::from(std::env::var_os(name).expect(name)), 0)
                .unwrap()
        };
        let a = read("TRANSCRIBE_SMOKE_AUDIO");
        let b = read("TRANSCRIBE_LFM_AUDIO_B");
        let mut config = config();
        if let Ok(backend) = std::env::var("TRANSCRIBE_LFM_TEST_BACKEND") {
            config.compute_backend = serde_json::from_value(serde_json::Value::String(backend))
                .expect("valid explicit test backend");
        }
        let cancelled = AtomicBool::new(false);
        transcribe_models::verify_asr(&config, &root, &cancelled).unwrap();
        {
            let model = Recognizer::load(&config, &root, &cancelled).unwrap();
            // Test the unsegmented short reference path and reject output exhaustion.
            let baseline = model.decode(a.samples(), &cancelled).unwrap();
            assert!(!baseline.is_empty());
            assert!(model.decode_with_limit(a.samples(), &cancelled, 1).is_err());
            assert_eq!(model.decode(a.samples(), &cancelled).unwrap(), baseline);
            assert!(model
                .model
                .token_attr(
                    model
                        .model
                        .str_to_token("<|audio_start|>", AddBos::Never)
                        .unwrap()[0]
                )
                .contains(LlamaTokenAttr::Control));
        }
        let manager = crate::InferenceManager::new().unwrap();
        let transcribe = |audio: &MonoPcm, id: &str| {
            manager
                .transcribe(audio, &config, &root, id, Arc::new(AtomicBool::new(false)))
                .unwrap()
        };
        let first = transcribe(&a, "lfm-a");
        let other = transcribe(&b, "lfm-b");
        assert!(!first.is_empty() && !other.is_empty());
        assert_ne!(first, other);
        assert_eq!(transcribe(&a, "lfm-a-repeat"), first);
        let silence = MonoPcm::new(vec![0.0; ASR_SAMPLE_RATE as usize], ASR_SAMPLE_RATE).unwrap();
        assert!(transcribe(&silence, "lfm-silence").is_empty());
        let wrong_rate = MonoPcm::new(vec![0.1; 8000], 8000).unwrap();
        assert!(manager
            .transcribe(
                &wrong_rate,
                &config,
                &root,
                "lfm-rate",
                Arc::new(AtomicBool::new(false))
            )
            .is_err());

        // Native audio evaluation cannot immediately abort. Measure the actual
        // cancellation return, then prove the next request has fresh state.
        let cancel = Arc::new(AtomicBool::new(false));
        let signal = Arc::clone(&cancel);
        let started = Instant::now();
        let handle = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(50));
            signal.store(true, Ordering::Release);
        });
        let result = manager.transcribe(&a, &config, &root, "lfm-cancel", cancel);
        handle.join().unwrap();
        assert!(matches!(result, Err(InferenceError::Cancelled)));
        let cancel_return_ms = started.elapsed().as_secs_f64() * 1000.0;
        assert_eq!(transcribe(&a, "lfm-after-cancel"), first);

        let cancel = Arc::new(AtomicBool::new(false));
        let signal = Arc::clone(&cancel);
        let long = MonoPcm::new(a.samples().repeat(4), ASR_SAMPLE_RATE).unwrap();
        assert!(matches!(
            manager.transcribe_with_progress(
                &long,
                &config,
                &root,
                "lfm-segment-cancel",
                cancel,
                Arc::new(move |_| signal.store(true, Ordering::Release))
            ),
            Err(InferenceError::Cancelled)
        ));
        assert_eq!(transcribe(&a, "lfm-after-segment-cancel"), first);
        assert!(manager
            .start_stream(
                &config,
                &root,
                "lfm-no-native-stream",
                Arc::new(AtomicBool::new(false)),
                Arc::new(|_| {})
            )
            .is_err());

        // Shared backend initialization must allow both actual model families.
        let cleaned = manager
            .rewrite(&first, &config, &root, Arc::new(AtomicBool::new(false)))
            .unwrap();
        assert!(!cleaned.is_empty());
        manager.unload().unwrap();
        assert_eq!(transcribe(&a, "lfm-after-unload"), first);
        println!(
            "{}",
            serde_json::json!({"case":"lfm-native-contract", "passed":true, "cancelReturnFromRequestStartMs":cancel_return_ms, "cancelSignalDelayMs":50, "execution":manager.execution_evidence("lfm-after-unload")})
        );
    }

    #[test]
    fn context_and_numerical_failures_cannot_be_success() {
        assert!(validate_budget(100, 1024, 512).is_ok());
        for positions in [-1, 0, 513, i32::MAX] {
            assert!(validate_budget(positions, 1024, 512).is_err());
        }
        assert!(validate_logits(&[f32::NEG_INFINITY, 0.0, 1.0]).is_ok());
        for values in [
            &[][..],
            &[f32::NAN],
            &[f32::INFINITY, 0.0],
            &[f32::NEG_INFINITY],
        ] {
            assert!(validate_logits(values).is_err());
        }
    }
}
