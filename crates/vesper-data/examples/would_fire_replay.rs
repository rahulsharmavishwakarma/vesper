//! Would-fire replay: run the correction gate over every stored transcript with a
//! frozen working set and report what it would change. Opens the dataset directory
//! it is given (use a copy); never runs a model and never writes to the source.
//!
//! Usage: would_fire_replay DATASET_COPY_DIR WORKING_SET_JSON OUTPUT_JSON
use serde_json::json;
use std::{env, fs, time::Instant};
use vesper_core::{correction_gate_with, Language, NearMiss, WorkingSet};
use vesper_data::DatasetStore;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    let [dataset, set, output] = args.as_slice() else {
        return Err(
            "usage: would_fire_replay DATASET_COPY_DIR WORKING_SET_JSON OUTPUT_JSON".into(),
        );
    };
    if std::path::Path::new(output).exists() {
        return Err("output already exists".into());
    }
    let set: WorkingSet = serde_json::from_str(&fs::read_to_string(set)?)?;
    let store = DatasetStore::open(dataset.into())?;
    let mut rows = vec![];
    let (mut texts, mut words, mut exact, mut near, mut micros) = (0, 0, 0, 0, vec![]);
    for record in store.list_sessions()? {
        let language = record.config_snapshot.language;
        if language == Language::Auto {
            continue;
        }
        for (stage, text) in [
            ("raw", record.raw_transcript.as_deref()),
            ("cleaned", record.cleaned_transcript.as_deref()),
        ] {
            let Some(text) = text.filter(|t| !t.trim().is_empty()) else {
                continue;
            };
            texts += 1;
            words += text.split_whitespace().count();
            let started = Instant::now();
            let result = correction_gate_with(text, &set, language, NearMiss::Apply);
            micros.push(started.elapsed().as_micros() as u64);
            let near_applied = result
                .applied
                .iter()
                .filter(|c| c.term_id.starts_with("reference:"))
                .count();
            exact += result.applied.len() - near_applied;
            near += near_applied;
            if !result.applied.is_empty() {
                rows.push(json!({"session":record.id,"stage":stage,
                    "changes":result.applied.iter().map(|c| json!({"heard":c.heard,"spelling":c.spelling,
                        "kind":if c.term_id.starts_with("reference:") {"near_miss"} else {"exact"},
                        "context":&text[c.start.saturating_sub(40)..(c.end + 40).min(text.len())]
                            .chars().collect::<String>()})).collect::<Vec<_>>()}));
            }
        }
    }
    micros.sort_unstable();
    let pct = |p: f64| {
        micros
            .get(((micros.len() as f64 * p) as usize).min(micros.len().saturating_sub(1)))
            .copied()
    };
    fs::write(
        output,
        serde_json::to_string_pretty(&json!({
            "texts":texts,"words":words,"exactChanges":exact,"nearMissChanges":near,
            "textsChanged":rows.len(),"gateMicrosP50":pct(0.5),"gateMicrosP95":pct(0.95),
            "workingSet":{"normalizations":set.normalizations.len(),"references":set.references.len()},
            "rows":rows}))?,
    )?;
    Ok(())
}
