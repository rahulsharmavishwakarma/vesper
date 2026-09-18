//! Constructed context guard controls and component timings using the app's real selector.
//! Never opens a user database or invokes a model. References are reporting-only.
use serde_json::json;
use std::{env, fs, time::Instant};
use transcribe_core::{word_edits, Language, LearningContext};
use transcribe_data::DatasetStore;

fn summary(values: &mut [f64]) -> serde_json::Value {
    values.sort_by(f64::total_cmp);
    let percentile = |p: f64| values[((values.len() as f64 * p).ceil() as usize).saturating_sub(1)];
    json!({"count":values.len(),"meanMs":values.iter().sum::<f64>() / values.len() as f64,
        "p50Ms":percentile(0.5),"p95Ms":percentile(0.95),"p99Ms":percentile(0.99)})
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = env::args().nth(1).ok_or("supply a new output JSON path")?;
    if std::path::Path::new(&output).exists() {
        return Err("output already exists".into());
    }
    let directory = tempfile::tempdir()?;
    let store = DatasetStore::open(directory.path().join("dataset"))?;
    let mut feedback_ms = vec![];
    let mut read_ms = vec![];
    let mut select_ms = vec![];
    let mut apply_ms = vec![];
    let mut rows = vec![];
    let mut baseline_edits = 0;
    let mut guarded_edits = 0;
    let mut improved = 0;
    let mut regressed = 0;
    for (i, topic) in [
        "deployment",
        "database",
        "network",
        "compiler",
        "logging",
        "storage",
        "routing",
        "packaging",
    ]
    .iter()
    .enumerate()
    {
        let mut profile = store.dictation_personalization()?;
        profile.active_project = format!("project-{i}");
        let profile = store.save_dictation_personalization(profile)?;
        let before = format!("Open read me for {topic}");
        let after = format!("Open README.md for {topic}");
        let ctx = LearningContext {
            result_id: format!("teach-{i}"),
            source_key: format!("audio-{i}"),
            episode_id: format!("episode-{i}"),
            language: Language::English,
            project: profile.active_project.clone(),
            model_id: "constructed-output".into(),
            learning_epoch: profile.learning_epoch,
            learning_enabled: true,
        };
        let start = Instant::now();
        store.learn_from_edit(&ctx, 0, &before, &after)?;
        feedback_ms.push(start.elapsed().as_secs_f64() * 1000.0);
        let cases = [
            (
                "matching",
                format!("Review read me for {topic}"),
                format!("Review README.md for {topic}"),
            ),
            (
                "other_context",
                "Open read me for shopping".into(),
                "Open read me for shopping".into(),
            ),
            (
                "project_only",
                format!("Open read me in project-{i}"),
                format!("Open read me in project-{i}"),
            ),
            (
                "quoted_alias",
                format!("Open 'read me' for {topic}"),
                format!("Open 'read me' for {topic}"),
            ),
            (
                "quoted_context",
                format!("Open read me for '{topic}'"),
                format!("Open read me for '{topic}'"),
            ),
            // Deliberately retains the lexical guard's expected recall cost.
            (
                "unseen_paraphrase",
                "Review read me for release".into(),
                "Review README.md for release".into(),
            ),
            (
                "protected_meaning",
                format!("Do not open read me for {topic} on 21 June"),
                format!("Do not open README.md for {topic} on 21 June"),
            ),
            (
                "no_alias",
                format!("The {topic} is ready"),
                format!("The {topic} is ready"),
            ),
        ];
        for (kind, raw, reference) in cases {
            let mut first = None;
            for _ in 0..5 {
                let start = Instant::now();
                let snapshot = store.dictation_personalization_snapshot()?;
                read_ms.push(start.elapsed().as_secs_f64() * 1000.0);
                let baseline = snapshot.profile.personalize(&raw, Language::English).text;
                let start = Instant::now();
                let selected = snapshot.post_asr_profile(&raw, Language::English);
                select_ms.push(start.elapsed().as_secs_f64() * 1000.0);
                let start = Instant::now();
                let guarded = selected.personalize(&raw, Language::English).text;
                apply_ms.push(start.elapsed().as_secs_f64() * 1000.0);
                let pair = (baseline, guarded);
                if let Some(previous) = &first {
                    assert_eq!(previous, &pair, "repeat changed output");
                } else {
                    first = Some(pair);
                }
            }
            let (baseline, guarded) = first.unwrap();
            let a = word_edits(&reference, &baseline).ok_or("unscorable baseline")?;
            let b = word_edits(&reference, &guarded).ok_or("unscorable guarded output")?;
            baseline_edits += a;
            guarded_edits += b;
            improved += usize::from(b < a);
            regressed += usize::from(b > a);
            rows.push(
                json!({"project":i,"case":kind,"raw":raw,"reference":reference,"baseline":baseline,
                "guarded":guarded,"baselineWordEdits":a,"guardedWordEdits":b}),
            );
        }
    }
    let report = json!({"schemaVersion":1,"corpus":"64 authored text controls, eight lexical topics; no acoustic inference or real-user feedback",
        "baseline":"same learned profile without the added post-ASR eligibility guard","guarded":"production DictationPersonalizationSnapshot::post_asr_profile",
        "totals":{"cases":rows.len(),"baselineWordEdits":baseline_edits,"guardedWordEdits":guarded_edits,"improved":improved,"regressed":regressed,"tied":rows.len()-improved-regressed},
        "rows":rows,"timings":{"feedback":summary(&mut feedback_ms),"snapshot":summary(&mut read_ms),"selection":summary(&mut select_ms),"application":summary(&mut apply_ms)},
        "timingDefinitions":{"feedback":"existing SQLite connection, learner transaction including commit; excludes connection open",
        "snapshot":"app snapshot API, SQLite reads and decode, includes idempotent desktop profile check; excludes connection open",
        "selection":"clone/filter profile with raw hypothesis and persisted terms; excludes SQLite and final text application",
        "application":"deterministic final replacement only; excludes SQLite, ASR, cleanup, UI and insertion"},
        "limitations":["Authored mechanism controls; aggregate is not a held-out real-user accuracy estimate.","Unseen paraphrase misses are retained as regressions.","No native scheduling, acoustic latency, battery or thermal qualification."]});
    fs::write(output, serde_json::to_string_pretty(&report)? + "\n")?;
    println!("{}", report["totals"]);
    Ok(())
}
