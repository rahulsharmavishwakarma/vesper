//! Would-fire replay: run the correction gate over every stored transcript and report
//! what it would change. Opens the dataset directory it is given (use a copy); never
//! runs a model and never writes to the source.
//!
//! Usage:
//!   would_fire_replay DATASET_COPY_DIR WORKING_SET_JSON OUTPUT_JSON
//!   would_fire_replay DATASET_COPY_DIR TERMS_JSON OUTPUT_JSON WORKSPACE_KEY CATEGORY
//! The second form selects a working set per transcript from an `InventoryTerm` list,
//! exactly as dictation does after recognition (selection reads the raw text).
use serde_json::json;
use std::{env, fs, time::Instant};
use vesper_core::{
    correction_gate_with, select_working_set, AppCategory, InventoryTerm, Language, NearMiss,
    SelectionContext, WorkingSet, WorkspaceContext,
};
use vesper_data::DatasetStore;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    let (dataset, input, output) = match args.as_slice() {
        [d, i, o] | [d, i, o, _, _] => (d, i, o),
        _ => return Err("see the usage in the file header".into()),
    };
    if std::path::Path::new(output).exists() {
        return Err("output already exists".into());
    }
    let source = fs::read_to_string(input)?;
    let selection = match args.get(3..5) {
        Some([key, category]) => Some((
            serde_json::from_str::<Vec<InventoryTerm>>(&source)?,
            WorkspaceContext {
                workspace_key: key.clone(),
                workspace_label: key.clone(),
                category: serde_json::from_value::<AppCategory>(json!(category))?,
                ..WorkspaceContext::default()
            },
        )),
        _ => None,
    };
    let fixed: WorkingSet = match selection {
        Some(_) => WorkingSet::default(),
        None => serde_json::from_str(&source)?,
    };
    let store = DatasetStore::open(dataset.into())?;
    let mut rows = vec![];
    let (mut texts, mut words, mut exact, mut near) = (0, 0, 0, 0);
    let (mut gate_micros, mut select_micros) = (vec![], vec![]);
    for record in store.list_sessions()? {
        let language = record.config_snapshot.language;
        let Some(raw) = record
            .raw_transcript
            .as_deref()
            .filter(|t| !t.trim().is_empty())
        else {
            continue;
        };
        if language == Language::Auto {
            continue;
        }
        let started = Instant::now();
        let set = match &selection {
            Some((terms, workspace)) => select_working_set(
                terms,
                &SelectionContext {
                    workspace: workspace.clone(),
                    project: workspace.workspace_label.clone(),
                    language,
                    now_ms: chrono::Utc::now().timestamp_millis(),
                },
                Some(raw),
            ),
            None => fixed.clone(),
        };
        select_micros.push(started.elapsed().as_micros() as u64);
        for (stage, text) in [
            ("raw", Some(raw)),
            ("cleaned", record.cleaned_transcript.as_deref()),
        ] {
            let Some(text) = text.filter(|t| !t.trim().is_empty()) else {
                continue;
            };
            texts += 1;
            words += text.split_whitespace().count();
            let started = Instant::now();
            let result = correction_gate_with(text, &set, language, NearMiss::Apply);
            gate_micros.push(started.elapsed().as_micros() as u64);
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
                        "context":text.get(c.start.saturating_sub(40)..(c.end + 40).min(text.len()))
                            .unwrap_or(&c.heard)})).collect::<Vec<_>>()}));
            }
        }
    }
    let percentile = |values: &mut Vec<u64>, p: f64| {
        values.sort_unstable();
        values
            .get(((values.len() as f64 * p) as usize).min(values.len().saturating_sub(1)))
            .copied()
    };
    fs::write(
        output,
        serde_json::to_string_pretty(&json!({
            "texts":texts,"words":words,"exactChanges":exact,"nearMissChanges":near,
            "textsChanged":rows.len(),
            "gateMicros":{"p50":percentile(&mut gate_micros,0.5),"p95":percentile(&mut gate_micros,0.95)},
            "selectMicros":{"p50":percentile(&mut select_micros,0.5),"p95":percentile(&mut select_micros,0.95)},
            "perTranscriptSelection":selection.is_some(),
            "rows":rows}))?,
    )?;
    Ok(())
}
