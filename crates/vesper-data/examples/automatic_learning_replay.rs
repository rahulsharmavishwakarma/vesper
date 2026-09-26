//! Chronological text/event replay. Never runs ASR or treats simulated edits as real users.
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{env, fs, time::Instant};
use vesper_core::*;
use vesper_data::DatasetStore;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Episode {
    id: String,
    source_key: String,
    at_ms: u64,
    project: String,
    language: Language,
    model_id: String,
    input: String,
    reference: String,
    edited: Option<String>,
    /// Independent oracle annotation for the manual-confirmation comparator only.
    #[serde(default)]
    manually_remember: bool,
    expected_automatic: Option<String>,
}

fn timing(values: &[f64]) -> Value {
    if values.is_empty() {
        return Value::Null;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let percentile = |p: f64| {
        sorted[((sorted.len() as f64 * p).ceil() as usize)
            .saturating_sub(1)
            .min(sorted.len() - 1)]
    };
    json!({"count":values.len(),"meanMs":values.iter().sum::<f64>()/values.len() as f64,"p50Ms":percentile(0.5),"p95Ms":percentile(0.95),"p99Ms":percentile(0.99)})
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = env::args().skip(1).collect();
    if args.len() != 2 {
        return Err("usage: automatic_learning_replay EPISODES_JSON RESULTS_JSON".into());
    }
    let bytes = fs::read(&args[0])?;
    let fixture: Value = serde_json::from_slice(&bytes)?;
    let episodes: Vec<Episode> = serde_json::from_value(fixture["episodes"].clone())?;
    if episodes.windows(2).any(|w| w[1].at_ms < w[0].at_ms) {
        return Err("episodes must be chronological".into());
    }
    let mut modes = vec![];
    for mode in [
        "off",
        "manual_oracle",
        "automatic_edits",
        "automatic_usage_retry",
    ] {
        let temp = tempfile::tempdir()?;
        let store = DatasetStore::open(temp.path().join("data"))?;
        store.save_personalization(PersonalizationProfile {
            automatic_learning: mode.starts_with("automatic"),
            ..Default::default()
        })?;
        let mut rows = vec![];
        let mut apply_times = vec![];
        let mut edit_times = vec![];
        let mut observation_times = vec![];
        let mut previous: Option<(&Episode, String)> = None;
        for episode in &episodes {
            let mut profile = store.personalization()?;
            profile.active_project = episode.project.clone();
            let start = Instant::now();
            let prediction = profile.personalize(&episode.input, episode.language);
            apply_times.push(start.elapsed().as_secs_f64() * 1000.0);
            // Withheld labels are scored here and never sent to candidate construction.
            let raw_errors = word_edits(&episode.reference, &episode.input);
            let errors = word_edits(&episode.reference, &prediction.text);
            let passed = episode
                .expected_automatic
                .as_ref()
                .map(|s| !mode.starts_with("automatic") || s == &prediction.text);
            let mut context = LearningContext {
                result_id: episode.id.clone(),
                source_key: episode.source_key.clone(),
                episode_id: episode.id.clone(),
                language: episode.language,
                project: episode.project.clone(),
                model_id: episode.model_id.clone(),
                learning_epoch: profile.learning_epoch,
                learning_enabled: profile.automatic_learning,
            };
            let mut retry = None;
            if let Some((prior, id)) = &previous {
                let elapsed = episode.at_ms.saturating_sub(prior.at_ms);
                let same = prior.project == episode.project && prior.language == episode.language;
                let relation = retry_relation(&prior.input, &episode.input, elapsed, same);
                if correlated_attempts(&prior.input, &episode.input, elapsed, same)
                    || relation == RetryRelation::PossibleRepair
                {
                    context.episode_id = id.clone();
                }
                retry = Some(relation);
            }
            previous = Some((episode, context.episode_id.clone()));
            let mut failures = vec![];
            if mode == "automatic_usage_retry" {
                let start = Instant::now();
                if let Err(e) = DatasetStore::open(temp.path().join("data"))
                    .and_then(|s| s.observe_learning(&context, &episode.input, retry))
                {
                    failures.push(e.to_string());
                }
                observation_times.push(start.elapsed().as_secs_f64() * 1000.0);
            }
            let mut outcome = None;
            if let Some(edited) = &episode.edited {
                if mode.starts_with("automatic") {
                    let start = Instant::now();
                    // The edit is relative to the text the user actually sees.
                    match DatasetStore::open(temp.path().join("data"))
                        .and_then(|s| s.learn_from_edit(&context, 0, &prediction.text, edited))
                    {
                        Ok(result) => outcome = Some(result),
                        Err(e) => failures.push(e.to_string()),
                    }
                    edit_times.push(start.elapsed().as_secs_f64() * 1000.0);
                } else if mode == "manual_oracle" && episode.manually_remember {
                    let mut p = store.personalization()?;
                    for pair in correction_suggestions(&prediction.text, edited) {
                        if p.vocabulary.len() == 128 {
                            failures.push("Manual profile full".into());
                            break;
                        }
                        p.vocabulary.push(VocabularyEntry {
                            id: uuid::Uuid::new_v4().to_string(),
                            phrase: pair.intended,
                            aliases: vec![pair.heard],
                            language: episode.language,
                            project: episode.project.clone(),
                            enabled: true,
                            ..Default::default()
                        });
                    }
                    if let Err(e) = store.save_personalization(p) {
                        failures.push(e.to_string());
                    }
                }
            }
            rows.push(json!({"id":episode.id,"language":episode.language,"modelId":episode.model_id,"rawErrors":raw_errors,"memoryErrors":errors,"referenceWords":episode.reference.split_whitespace().count(),"actual":prediction.text,"expectedCheck":passed,"applied":prediction.evidence.applied.len(),"outcome":outcome,"failures":failures}));
        }
        let state = store.automatic_learning_state()?;
        modes.push(json!({"mode":mode,"memoryCount":store.personalization()?.vocabulary.len(),"candidateCount":state.candidates.len(),"possibleRepairSignals":state.events.iter().filter(|e|e.retry==Some(RetryRelation::PossibleRepair)).count(),"timingDefinitions":{"application":"in-memory profile.personalize only","feedback":"open SQLite, learn_from_edit, commit and close; excludes frontend IPC","observation":"open SQLite, observe_learning, commit and close"},"applicationTiming":timing(&apply_times),"feedbackTiming":timing(&edit_times),"observationTiming":timing(&observation_times),"rows":rows}));
    }
    fs::write(
        &args[1],
        serde_json::to_vec_pretty(
            &json!({"schemaVersion":1,"policyVersion":LEARNING_POLICY_VERSION,"kind":"chronological text/event replay; no new ASR execution or real-user measurement","fixtureSha256":format!("{:x}",Sha256::digest(&bytes)),"source":fixture["source"],"modes":modes}),
        )?,
    )?;
    Ok(())
}
