//! Opt-in end-to-end check of the desktop pipeline on real audio with installed
//! models: `WorkContext::process` (managed audio frontend, recognizer, writer,
//! saved corrections, correction gate) over the frozen term-evaluation corpus,
//! with each workspace's terms in the inventory as a mined folder would put them.
//! Run with:
//!   VESPER_E2E_MODEL_ROOT=… VESPER_E2E_CORPUS=… VESPER_E2E_OUT=… \
//!   cargo test --release -p vesper-desktop --lib e2e -- --ignored --nocapture
use super::*;
use serde_json::{json, Value};
use vesper_core::{AsrModel, TermSource};
use vesper_data::term_mining::MinedTerm;

fn word_hit(text: &str, term: &str) -> bool {
    text.match_indices(term).any(|(i, _)| {
        let before = text[..i].chars().next_back();
        let after = text[i + term.len()..].chars().next();
        !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphanumeric)
    })
}

#[test]
#[ignore = "needs installed models and the term-evaluation corpus"]
fn e2e_desktop_pipeline_on_term_corpus() {
    let model_root = PathBuf::from(std::env::var("VESPER_E2E_MODEL_ROOT").unwrap());
    let corpus = PathBuf::from(std::env::var("VESPER_E2E_CORPUS").unwrap());
    let out = PathBuf::from(std::env::var("VESPER_E2E_OUT").unwrap());
    let asr = match std::env::var("VESPER_E2E_ASR").as_deref() {
        Ok("cohere") => AsrModel::CohereTranscribe,
        _ => AsrModel::ParakeetTdt06bV3,
    };
    let manifest: Value =
        serde_json::from_str(&std::fs::read_to_string(corpus.join("term-manifest.json")).unwrap())
            .unwrap();
    let scratch = tempfile::tempdir().unwrap();
    let inference = Arc::new(vesper_inference::InferenceManager::new().unwrap());
    let mut roots = std::collections::HashMap::new();
    for workspace in manifest["workspaces"].as_array().unwrap() {
        let id = workspace["id"].as_str().unwrap().to_owned();
        let root = scratch.path().join(&id);
        let store = DatasetStore::open(root.clone()).unwrap();
        // Spellings only: spoken forms come from the miner's identifier split, never
        // from the corpus's authored mishearings.
        let terms = workspace["terms"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| {
                let source = if t["kind"] == "identifier" {
                    TermSource::Identifier
                } else {
                    TermSource::Documentation
                };
                MinedTerm::new(t["spelling"].as_str().unwrap(), source, "", 3)
            })
            .collect::<Vec<_>>();
        store
            .replace_mined_sources(
                &[TermSource::Identifier, TermSource::Documentation],
                "",
                &terms,
                0,
            )
            .unwrap();
        roots.insert(id, root);
    }
    let mut rows = vec![];
    let mut n = 0;
    for sample in manifest["samples"].as_array().unwrap() {
        let category = sample["category"].as_str().unwrap();
        if sample["split"] != "test"
            || sample["noise"] != "clean"
            || !matches!(category, "positive" | "hard_negative" | "real_negative")
        {
            continue;
        }
        let path = corpus.join(sample["audioPath"].as_str().unwrap_or(""));
        if !path.is_file() {
            continue;
        }
        let workspaces: Vec<String> = match sample["workspaceId"].as_str() {
            Some(id) => vec![id.into()],
            None => match sample["relatedTermId"].as_str() {
                Some(term) => vec![term.split('-').next().unwrap().into()],
                None => roots.keys().cloned().collect(),
            },
        };
        let audio = vesper_audio::decode_file(&path, 0).unwrap();
        for workspace in workspaces {
            let Some(root) = roots.get(&workspace) else {
                continue;
            };
            n += 1;
            let runtime = Arc::new(Runtime::default());
            let mut config = crate::settings::managed_audio(AppConfig {
                language: Language::English,
                cleanup_enabled: true,
                collect_training_data: false,
                ..Default::default()
            });
            config.models.asr = asr;
            let context = WorkContext {
                operation_id: format!("e2e-{n}"),
                config,
                model_root: model_root.clone(),
                dataset_root: root.clone(),
                runtime,
                inference: Arc::clone(&inference),
                streaming_result: Arc::default(),
                personalization: Arc::default(),
                defer_speakers: Default::default(),
                pending_speakers: Default::default(),
            };
            let started = std::time::Instant::now();
            let result = context.process(&audio, None);
            let elapsed = started.elapsed().as_secs_f64() * 1000.0;
            let row = match result {
                Ok(result) => {
                    let before = result
                        .cleaned_transcript
                        .clone()
                        .unwrap_or_else(|| result.raw_transcript.clone());
                    let after = result
                        .personalized_transcript
                        .clone()
                        .unwrap_or_else(|| before.clone());
                    let evidence = result.personalization_evidence.clone().unwrap_or_default();
                    json!({"id":sample["id"],"category":category,"workspace":workspace,
                        "reference":sample["reference"],"targets":sample["targetTerms"],
                        "raw":result.raw_transcript,"before":before,"after":after,
                        "gate":evidence.gate,"aliases":evidence.applied,"ms":elapsed,
                        "warnings":result.warnings})
                }
                Err(error) => json!({"id":sample["id"],"category":category,"workspace":workspace,
                    "error":error,"ms":elapsed}),
            };
            rows.push(row);
        }
    }
    let mut summary = std::collections::BTreeMap::<String, [f64; 6]>::new();
    for row in &rows {
        let entry = summary
            .entry(row["category"].as_str().unwrap().to_owned())
            .or_default();
        entry[0] += 1.0;
        if row.get("error").is_some() {
            entry[5] += 1.0;
            continue;
        }
        let reference = row["reference"].as_str().unwrap();
        for target in row["targets"].as_array().unwrap() {
            let term = target.as_str().unwrap();
            entry[1] += 1.0;
            entry[2] += f64::from(u8::from(word_hit(row["before"].as_str().unwrap(), term)));
            entry[3] += f64::from(u8::from(word_hit(row["after"].as_str().unwrap(), term)));
        }
        entry[4] += row["gate"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| !word_hit(reference, c["spelling"].as_str().unwrap()))
            .count() as f64;
    }
    let report = json!({
        "asr": asr.name(),
        "summary": summary.iter().map(|(k, v)| (k.clone(), json!({"decodes":v[0],"targets":v[1],
            "hitsBeforeGate":v[2],"hitsAfterGate":v[3],"harmfulGateChanges":v[4],"failures":v[5]})))
            .collect::<serde_json::Map<_, _>>(),
        "rows": rows,
    });
    std::fs::write(&out, serde_json::to_string_pretty(&report).unwrap()).unwrap();
    println!(
        "{}",
        serde_json::to_string_pretty(&report["summary"]).unwrap()
    );
}
