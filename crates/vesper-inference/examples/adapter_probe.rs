//! Synthetic checkpoint-specific LoRA/runtime proof; no app data or activation.
use llama_cpp_2::{
    context::{
        params::{KvCacheType, LlamaContextParams},
        LlamaContext,
    },
    llama_backend::LlamaBackend,
    llama_batch::LlamaBatch,
    model::{params::LlamaModelParams, AddBos, LlamaModel},
    token::LlamaToken,
};
use sha2::{Digest, Sha256};
use std::{
    error::Error,
    io::{Read, Write},
    num::NonZeroU32,
    path::Path,
    time::Instant,
};

fn file_hash(path: &str) -> Result<String, Box<dyn Error>> {
    let mut file = std::fs::File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = vec![0_u8; 1024 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn logits(
    context: &mut LlamaContext<'_>,
    tokens: &[LlamaToken],
) -> Result<Vec<f32>, Box<dyn Error>> {
    context.clear_kv_cache();
    let mut batch = LlamaBatch::new(128, 1);
    for (index, token) in tokens.iter().enumerate() {
        batch.add(
            *token,
            i32::try_from(index)?,
            &[0],
            index + 1 == tokens.len(),
        )?;
    }
    context.decode(&mut batch)?;
    let values = context.get_logits();
    if !values.iter().all(|value| value.is_finite()) {
        return Err("Nonfinite native logits".into());
    }
    Ok(values.to_vec())
}
fn difference(a: &[f32], b: &[f32]) -> Result<f32, Box<dyn Error>> {
    if a.len() != b.len() || a.is_empty() {
        return Err("Mismatched logits".into());
    }
    Ok(a.iter()
        .zip(b)
        .map(|(a, b)| (a - b).abs())
        .fold(0.0, f32::max))
}
fn save(path: &Path, values: &[f32]) -> Result<(), Box<dyn Error>> {
    let mut output = std::io::BufWriter::new(std::fs::File::create_new(path)?);
    for value in values {
        output.write_all(&value.to_le_bytes())?;
    }
    output.flush()?;
    Ok(())
}
fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 5 || !matches!(args[3].as_str(), "cpu" | "metal" | "cpu-f32-cache") {
        return Err(
            "Usage: adapter_probe MODEL ZERO_ADAPTER NONZERO_ADAPTER cpu|metal|cpu-f32-cache NEW_OUTPUT_DIR"
                .into(),
        );
    }
    let output = Path::new(&args[4]);
    std::fs::create_dir(output)?;
    let artifacts = serde_json::json!({"modelSha256":file_hash(&args[0])?, "zeroAdapterSha256":file_hash(&args[1])?, "nonzeroAdapterSha256":file_hash(&args[2])?, "probeSourceSha256":format!("{:x}", Sha256::digest(include_bytes!("adapter_probe.rs")))});
    let backend = LlamaBackend::init()?;
    let gpu = args[3] == "metal";
    if gpu && !backend.supports_gpu_offload() {
        return Err("Native GPU offload unavailable".into());
    }
    let devices = if gpu {
        vec![
            llama_cpp_2::list_llama_ggml_backend_devices()
                .into_iter()
                .find(|device| device.backend == "MTL" && device.memory_total > 0)
                .ok_or("Metal device unavailable")?
                .index,
        ]
    } else {
        vec![]
    };
    let started = Instant::now();
    let model = LlamaModel::load_from_file(
        &backend,
        &args[0],
        &LlamaModelParams::default()
            .with_n_gpu_layers(if gpu { 999 } else { 0 })
            .with_devices(&devices)?,
    )?;
    let load_ms = started.elapsed().as_secs_f64() * 1000.0;
    let mut zero = model.lora_adapter_init(&args[1])?;
    let mut nonzero = model.lora_adapter_init(&args[2])?;
    let mut params = LlamaContextParams::default()
        .with_n_ctx(NonZeroU32::new(512))
        .with_n_batch(128)
        .with_n_ubatch(128)
        .with_n_threads(4)
        .with_n_threads_batch(4)
        .with_offload_kqv(gpu)
        .with_op_offload(gpu);
    if args[3] == "cpu-f32-cache" {
        params = params
            .with_type_k(KvCacheType::F32)
            .with_type_v(KvCacheType::F32);
    }
    let mut records = Vec::new();
    for (index, prompt) in [
        "Fix punctuation: tomorrow we ship the build",
        "Open README in the repository.",
        "कल हम नई योजना पर काम करेंगे।",
    ]
    .iter()
    .enumerate()
    {
        let tokens = model.str_to_token(prompt, AddBos::Never)?;
        let mut context = model.new_context(&backend, params.clone())?;
        let started = Instant::now();
        let baseline = logits(&mut context, &tokens)?;
        context.lora_adapter_set(&mut zero, 1.0)?;
        let zero_values = logits(&mut context, &tokens)?;
        context.lora_adapter_set(&mut nonzero, 0.0)?;
        let scale_zero = logits(&mut context, &tokens)?;
        context.lora_adapter_set(&mut nonzero, 1.0)?;
        let changed = logits(&mut context, &tokens)?;
        let repeated = logits(&mut context, &tokens)?;
        context.lora_adapter_remove(&mut nonzero)?;
        let removed = logits(&mut context, &tokens)?;
        let zero_error = difference(&baseline, &zero_values)?;
        let scale_zero_error = difference(&baseline, &scale_zero)?;
        let removal_error = difference(&baseline, &removed)?;
        let repeat_error = difference(&changed, &repeated)?;
        let effect = difference(&baseline, &changed)?;
        save(&output.join(format!("{index}-off.f32")), &baseline)?;
        save(&output.join(format!("{index}-on.f32")), &changed)?;
        records.push(serde_json::json!({"prompt":prompt,"tokenIds":tokens.iter().map(|t| t.0).collect::<Vec<_>>(),"vocabulary":baseline.len(),"zeroMaxAbs":zero_error,"scaleZeroMaxAbs":scale_zero_error,"removedMaxAbs":removal_error,"repeatMaxAbs":repeat_error,"nonzeroEffectMaxAbs":effect,"sixPassMs":started.elapsed().as_secs_f64()*1000.0}));
        if zero_error > 0.0005
            || scale_zero_error > 0.0005
            || removal_error > 0.0005
            || repeat_error > 0.0005
            || effect <= 0.00001
        {
            std::fs::write(
                output.join("failed.json"),
                serde_json::to_vec_pretty(&records)?,
            )?;
            return Err("Native adapter parity/control gate failed".into());
        }
    }
    let report = serde_json::json!({"schemaVersion":1,"artifacts":artifacts,"backend":if gpu {"metal"} else {"cpu"},"kvCache":if args[3] == "cpu-f32-cache" {"f32"} else {"f16"},"threads":4,"nativeRevision":"e79e4bf660e19f2ad851e06c6913f7a8c5852621","contextTokens":512,"batchTokens":128,"loadMs":load_ms,"toleranceMaxAbs":0.0005,"prompts":records,"passed":true});
    std::fs::write(
        output.join("report.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
