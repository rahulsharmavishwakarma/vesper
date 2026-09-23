//! Explicit local qualification tool; never downloads or changes app defaults.
use std::{path::Path, sync::atomic::AtomicBool, time::Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() < 6 {
        return Err("usage: probe LIBRARY MODEL_DIRECTORY ARCHITECTURE LANGUAGE AUDIO...".into());
    }
    let start = Instant::now();
    let mut model =
        transcribe_mlx::Model::load(Path::new(&args[1]), Path::new(&args[2]), &args[3])?;
    eprintln!(
        "model_load_ms={:.3}",
        start.elapsed().as_secs_f64() * 1000.0
    );
    let cancelled = AtomicBool::new(false);
    for input in &args[5..] {
        let audio = transcribe_audio::decode_file(Path::new(input), 0)?;
        let start = Instant::now();
        let mut text = String::new();
        let mut results = Vec::new();
        let mut metrics = Vec::new();
        let maximum = match args[3].as_str() {
            "whisper" => 29 * 16_000,
            // The CPU Parakeet adapter consumes the whole recording. This
            // bounded probe can match that geometry only for <=30 s inputs.
            "parakeet" => 30 * 16_000,
            _ => 10 * 16_000,
        };
        let mut failure = None;
        // Share the application's energy boundary selection. Parakeet's
        // qualification-only 30 s cap must be reported for longer inputs.
        let mut offset = 0;
        while offset < audio.samples().len() {
            let end = transcribe_audio::completed_segment_end(audio.samples(), offset, maximum);
            let samples = &audio.samples()[offset..end];
            offset = end;
            if samples.iter().all(|sample| *sample == 0.0) {
                continue;
            }
            let result = match model.transcribe(samples, Some(&args[4]), 512, &cancelled) {
                Ok(result) => result,
                Err(error) => {
                    failure = Some(error.to_string());
                    text.clear();
                    break;
                }
            };
            if !text.is_empty() {
                text.push(' ');
            }
            text.push_str(result.text.trim());
            metrics.push(serde_json::json!({"nativeSeconds":result.native_seconds,"generationTokens":result.generation_tokens,"peakMemoryGB":result.peak_memory_gb,"whisperScores":result.whisper_scores}));
            results.push(result.stop_reason);
        }
        println!(
            "{}",
            serde_json::json!({"file":Path::new(input).file_name().unwrap().to_string_lossy(),"audioSeconds":audio.duration_seconds(),"asrMs":start.elapsed().as_secs_f64()*1000.0,"text":text,"error":failure,"completion":results,"metrics":metrics,"provider":"metal","runtimeRevision":transcribe_mlx::RUNTIME_REVISION})
        );
    }
    Ok(())
}
