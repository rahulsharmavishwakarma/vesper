//! Policy 6 replay: teach ordinary-word → term corrections three times each in a
//! scratch store, then run every local transcript through the real learned-alias
//! path and report what would change. Prints user text; keep the output local.
//! Usage: `policy6_replay HISTORY.jsonl TEACH.tsv` (TSV: before<TAB>after per edit).
use std::io::BufRead;

use vesper_core::{Language, LearningContext};
use vesper_data::DatasetStore;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [history, teach] = args.as_slice() else {
        return Err("usage: policy6_replay HISTORY.jsonl TEACH.tsv".into());
    };
    let dir = tempfile::tempdir()?;
    let store = DatasetStore::open(dir.path().join("dataset"))?;
    let mut profile = store.dictation_personalization()?;
    profile.active_project = "repo".into();
    let profile = store.save_dictation_personalization(profile)?;
    for (n, line) in std::fs::read_to_string(teach)?.lines().enumerate() {
        let Some((before, after)) = line.split_once('\t') else {
            continue;
        };
        let context = LearningContext {
            result_id: format!("t{n}"),
            source_key: format!("audio-t{n}"),
            episode_id: format!("t{n}"),
            language: Language::English,
            project: "repo".into(),
            model_id: "Fermion Phonon-2".into(),
            learning_epoch: profile.learning_epoch,
            learning_enabled: true,
            ..Default::default()
        };
        let outcome = store.learn_from_edit(&context, 0, before, after)?;
        eprintln!("teach {n}: activated {:?}", outcome.activated);
    }
    for c in store.automatic_learning_state()?.candidates {
        eprintln!(
            "candidate {:?} -> {:?}: {:?}, support {}, {}",
            c.alias,
            c.phrase,
            c.state,
            c.support.len(),
            c.reason
        );
        for support in &c.support {
            eprintln!("  cues {:?}", support.teaching_terms);
        }
    }
    let snapshot = store.dictation_personalization_snapshot()?;
    let (mut total, mut changed) = (0, 0);
    for line in std::io::BufReader::new(std::fs::File::open(history)?).lines() {
        let Ok(text) = serde_json::from_str::<String>(&line?) else {
            continue;
        };
        total += 1;
        let mut profile =
            snapshot.post_asr_profile_for(&text, Language::English, "Fermion Phonon-2");
        profile.active_project = "repo".into();
        let out = profile.personalize(&text, Language::English);
        if out.text != text {
            changed += 1;
            println!("{}", serde_json::json!({"before": text, "after": out.text}));
        }
    }
    eprintln!("{changed} of {total} transcripts changed");
    Ok(())
}
