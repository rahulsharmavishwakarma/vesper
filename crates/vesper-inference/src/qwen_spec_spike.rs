//! #141 measurement-only spike: prompt-lookup speculative decoding with
//! llama.cpp's experimental recurrent-state rollback (`n_rs_seq`). Not used by
//! the application. Compares greedy outputs and decode time against the
//! existing one-token loop on public transcripts.
use super::*;

const DRAFT: usize = 8;

fn argmax(logits: &[f32]) -> LlamaToken {
    let mut best = 0;
    for (index, value) in logits.iter().enumerate() {
        if *value > logits[best] {
            best = index;
        }
    }
    LlamaToken::new(best as i32)
}

struct Run {
    tokens: Vec<LlamaToken>,
    decode_us: u64,
    decodes: usize,
    drafted: usize,
    accepted: usize,
}

fn prefill(context: &mut llama_cpp_2::context::LlamaContext<'_>, tokens: &[LlamaToken]) -> i32 {
    let mut batch = LlamaBatch::new(BATCH_TOKENS, 1);
    let mut offset = 0;
    while offset < tokens.len() {
        let end = (offset + BATCH_TOKENS).min(tokens.len());
        batch.clear();
        for (position, token) in tokens.iter().enumerate().take(end).skip(offset) {
            batch
                .add(*token, position as i32, &[0], position + 1 == tokens.len())
                .unwrap();
        }
        context.decode(&mut batch).unwrap();
        offset = end;
    }
    // Logits exist only for the final prompt token, at its index in the last batch.
    batch.n_tokens() - 1
}

/// The tokens that followed the latest output n-gram in the transcript,
/// searching forward from the previous match so the copy stays aligned.
fn lookup(raw: &[LlamaToken], output: &[LlamaToken], cursor: &mut usize) -> Vec<LlamaToken> {
    for n in (1..=3).rev() {
        if output.len() < n {
            continue;
        }
        let suffix = &output[output.len() - n..];
        let found = (*cursor..raw.len().saturating_sub(n))
            .chain(0..(*cursor).min(raw.len().saturating_sub(n)))
            .find(|&start| &raw[start..start + n] == suffix);
        if let Some(start) = found {
            let from = start + n;
            *cursor = from;
            return raw[from..(from + DRAFT).min(raw.len())].to_vec();
        }
    }
    Vec::new()
}

fn generate(
    model: &Rewriter,
    prompt: &[LlamaToken],
    raw: &[LlamaToken],
    limit: usize,
    rollback: u32,
    speculative: bool,
) -> Run {
    let context_tokens = CONTEXT_TOKENS.min(model.model.n_ctx_train());
    let mut context = model
        .model
        .new_context(
            model.backend,
            context_parameters(context_tokens, model.gpu_layers).with_n_rs_seq(rollback),
        )
        .unwrap();
    let last = prefill(&mut context, prompt);
    let mut next = argmax(context.get_logits_ith(last));
    let started = Instant::now();
    let mut run = Run {
        tokens: Vec::new(),
        decode_us: 0,
        decodes: 0,
        drafted: 0,
        accepted: 0,
    };
    let mut position = prompt.len();
    let mut cursor = 0;
    let mut batch = LlamaBatch::new(DRAFT + 1, 1);
    while run.tokens.len() < limit && !model.model.is_eog_token(next) {
        run.tokens.push(next);
        let draft = if speculative {
            lookup(raw, &run.tokens, &mut cursor)
        } else {
            Vec::new()
        };
        batch.clear();
        batch.add(next, position as i32, &[0], true).unwrap();
        for (offset, token) in draft.iter().enumerate() {
            batch
                .add(*token, (position + 1 + offset) as i32, &[0], true)
                .unwrap();
        }
        context.decode(&mut batch).unwrap();
        run.decodes += 1;
        run.drafted += draft.len();
        // Logits at batch index i predict the token after position + i.
        let mut accepted = 0;
        let mut predicted = argmax(context.get_logits_ith(0));
        while accepted < draft.len() && predicted == draft[accepted] && run.tokens.len() < limit {
            run.tokens.push(draft[accepted]);
            accepted += 1;
            predicted = argmax(context.get_logits_ith(accepted as i32));
        }
        run.accepted += accepted;
        if accepted < draft.len() {
            let first_rejected = (position + 1 + accepted) as u32;
            assert!(
                context
                    .clear_kv_cache_seq(Some(0), Some(first_rejected), None)
                    .unwrap(),
                "rollback of {} tokens failed",
                draft.len() - accepted
            );
        }
        position += 1 + accepted;
        next = predicted;
    }
    run.decode_us = micros(started);
    run
}

#[test]
#[ignore = "installed Qwen3.5 Q4 and VESPER_WRITER_INPUTS JSON array of public transcripts"]
fn measure_prompt_lookup_speculation() {
    let root = PathBuf::from(
        vesper_core::environment::var_os("VESPER_SMOKE_MODELS").expect("VESPER_SMOKE_MODELS"),
    );
    let inputs: Vec<String> = serde_json::from_slice(
        &std::fs::read(std::env::var("VESPER_WRITER_INPUTS").expect("inputs")).unwrap(),
    )
    .unwrap();
    let cancelled = AtomicBool::new(false);
    for backend in [ComputeBackend::Cpu, ComputeBackend::Auto] {
        let config = AppConfig {
            compute_backend: backend,
            ..Default::default()
        };
        let model = Rewriter::load(&config, &root, &cancelled).unwrap();
        for (index, raw) in inputs.iter().enumerate() {
            let prompt = nonthinking_prompt(raw, config.effective_cleanup_instruction()).unwrap();
            let prompt = model.model.str_to_token(&prompt, AddBos::Never).unwrap();
            let raw_tokens = model.model.str_to_token(raw, AddBos::Never).unwrap();
            let limit = (raw_tokens.len() * 2 + 32).clamp(64, MAX_OUTPUT_TOKENS);
            let baseline = generate(&model, &prompt, &raw_tokens, limit, 0, false);
            let rollback = generate(&model, &prompt, &raw_tokens, limit, DRAFT as u32, false);
            let speculative = generate(&model, &prompt, &raw_tokens, limit, DRAFT as u32, true);
            println!(
                "spec-spike-v1 {}",
                serde_json::json!({"backend": format!("{backend:?}"), "provider": model.initialized_provider,
                    "index": index, "tokens": baseline.tokens.len(),
                    "baselineUs": baseline.decode_us, "rollbackUs": rollback.decode_us,
                    "speculativeUs": speculative.decode_us, "decodes": speculative.decodes,
                    "drafted": speculative.drafted, "accepted": speculative.accepted,
                    "rollbackParity": rollback.tokens == baseline.tokens,
                    "speculativeParity": speculative.tokens == baseline.tokens})
            );
        }
    }
}
