//! Authored, isolated-store policy-shadow overhead; no ASR or user data.
use serde_json::json;
use std::{sync::atomic::AtomicBool, time::Instant};
use vesper_core::LearningContext;
use vesper_data::DatasetStore;
fn stats(mut values: Vec<f64>) -> serde_json::Value {
    values.sort_by(f64::total_cmp);
    json!({"n":values.len(),"meanMs":values.iter().sum::<f64>()/values.len() as f64,"p95Ms":values[(values.len()*95).div_ceil(100)-1]})
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let baseline = dir.path().join("baseline");
    let shadow = dir.path().join("shadow");
    for root in [&baseline, &shadow] {
        DatasetStore::open(root.clone())?.dictation_personalization()?;
    }
    let mut base = vec![];
    let mut foreground = vec![];
    let mut background = vec![];
    for i in 0..128 {
        let before = "cluster kubernets".to_owned();
        let after = "cluster kubernetes".to_owned();
        for enabled in if i % 2 == 0 {
            [false, true]
        } else {
            [true, false]
        } {
            let root = if enabled { &shadow } else { &baseline };
            let profile = DatasetStore::open(root.clone())?.personalization()?;
            let context = LearningContext {
                result_id: format!("r{i}"),
                source_key: format!("authored-source-{i}"),
                episode_id: format!("episode-{i}"),
                learning_epoch: profile.learning_epoch,
                learning_enabled: true,
                language: vesper_core::Language::English,
                ..Default::default()
            };
            let start = Instant::now();
            let job = if enabled {
                DatasetStore::open(root.clone())?
                    .prepare_policy_shadow(&context, 1, None, &before, &after)?
            } else {
                None
            };
            DatasetStore::open(root.clone())?.learn_from_edit(&context, 0, &before, &after)?;
            if let Some(job) = job {
                assert!(DatasetStore::open(root.clone())?.reserve_policy_shadow(&job)?);
                foreground.push(start.elapsed().as_secs_f64() * 1000.0);
                let t = Instant::now();
                assert!(DatasetStore::open_shadow_completion(root.clone())?
                    .finish_policy_shadow(job, &AtomicBool::new(false))?);
                background.push(t.elapsed().as_secs_f64() * 1000.0);
            } else {
                base.push(start.elapsed().as_secs_f64() * 1000.0);
            }
        }
    }
    let added: Vec<_> = foreground.iter().zip(&base).map(|(a, b)| a - b).collect();
    let b = DatasetStore::open(baseline)?.personalization()?;
    let s = DatasetStore::open(shadow.clone())?.personalization()?;
    // UUIDs differ, so compare application output rather than record identities.
    assert_eq!(
        b.personalize("cluster kubernets", vesper_core::Language::English)
            .text,
        s.personalize("cluster kubernets", vesper_core::Language::English)
            .text
    );
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"schemaVersion":1,"source":"128 authored explicit spelling corrections; independent source and episode IDs; alternating baseline/shadow order", "latencyDefinition":"Sequential store open + draft learning + pre-edit capture/reservation; background six-policy scoring and persistence measured separately. Excludes ASR, UI, worker queue and native editor.", "baseline":stats(base),"shadowForeground":stats(foreground),"pairedAddedForeground":stats(added),"shadowBackground":stats(background),"activeOutputEqual":true,"decisionsRetained":DatasetStore::open(shadow)?.policy_shadow_state()?.decisions.len()})
        )?
    );
    Ok(())
}
