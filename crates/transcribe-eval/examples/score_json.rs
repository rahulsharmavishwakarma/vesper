//! Score externally executed development probes with the canonical ASR metrics.
use serde_json::{json, Value};
use std::io::{self, Read};
use transcribe_eval::{score, score_non_speech};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let pairs: Vec<Value> = serde_json::from_str(&input)?;
    let results = pairs
        .iter()
        .map(|pair| {
            let reference = pair["reference"].as_str().ok_or("missing reference")?;
            let hypothesis = pair["hypothesis"].as_str().ok_or("missing hypothesis")?;
            if reference.is_empty() {
                let seconds = pair["durationSeconds"]
                    .as_f64()
                    .ok_or("missing non-speech duration")?;
                Ok(json!({"nonSpeech": score_non_speech(hypothesis, seconds)
                    .ok_or("invalid non-speech duration")?}))
            } else {
                Ok(json!({"speech": score(reference, hypothesis)}))
            }
        })
        .collect::<Result<Vec<Value>, &str>>()?;
    println!("{}", serde_json::to_string(&results)?);
    Ok(())
}
