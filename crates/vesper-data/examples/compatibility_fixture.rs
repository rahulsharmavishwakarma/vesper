//! Writes the synthetic dataset behind the stored-data compatibility tests
//! (`tests/compatibility.rs`). Run it at each release and commit the dump:
//!
//!     cargo run -p vesper-data --example compatibility_fixture -- <empty dir>
//!     sqlite3 <dir>/sessions.sqlite3 .dump > tests/fixtures/datasets/vX.Y.Z.sql
//!
//! It uses only store APIs that v0.1.1 already had, so the same source writes
//! every release's fixture. All text is synthetic; no audio is kept.

use std::path::{Path, PathBuf};

use serde_json::json;
use vesper_core::{
    AppConfig, Language, LearningContext, MemoryEvidence, OutputIntent, VocabularyEntry,
};
use vesper_data::DatasetStore;

#[allow(dead_code)]
fn main() {
    let root = PathBuf::from(
        std::env::args()
            .nth(1)
            .expect("usage: compatibility_fixture <dir>"),
    );
    write(&root);
}

/// Also used directly by `tests/compatibility.rs`.
pub fn write(root: &Path) {
    let store = DatasetStore::open(root.to_path_buf()).expect("open store");

    // A reviewed recording with a remembered cleanup example.
    let audio = root.join("synthetic.wav");
    write_silence(&audio);
    let config = AppConfig {
        collect_training_data: true,
        ..Default::default()
    };
    let session = store.import_audio(&audio, &config).expect("import");
    store
        .set_transcripts(
            &session.id,
            "send the atlas notes tomorrow".into(),
            Some("Send the Atlas notes tomorrow.".into()),
            json!({"asr": "synthetic"}),
        )
        .expect("transcripts");
    store
        .complete_personalization(
            &session.id,
            "Send the Atlas notes tomorrow.".into(),
            MemoryEvidence::default(),
            "atlas".into(),
        )
        .expect("personalization");
    store
        .approve_correction(&session.id, "Send the Atlas notes tomorrow.".into())
        .expect("review");
    store
        .remember_cleanup(&session.id, OutputIntent::Note)
        .expect("remember");

    // A manual word, project scope, and learning from edits.
    let mut profile = store.dictation_personalization().expect("profile");
    profile.active_project = "atlas".into();
    profile.vocabulary.push(VocabularyEntry {
        id: "fixture-word".into(),
        phrase: "Vesper".into(),
        enabled: true,
        ..Default::default()
    });
    let profile = store.save_personalization(profile).expect("save profile");
    for id in ["one", "two"] {
        let context = LearningContext {
            result_id: id.into(),
            source_key: format!("audio-{id}"),
            episode_id: id.into(),
            language: Language::English,
            learning_epoch: profile.learning_epoch,
            learning_enabled: true,
            ..Default::default()
        };
        store
            .learn_from_edit(
                &context,
                0,
                "Deploy kubernets to the cluster",
                "Deploy kubernetes to the cluster",
            )
            .expect("learn");
    }
    drop(store);
    std::fs::remove_file(&audio).ok();
}

fn write_silence(path: &Path) {
    let samples = 1600u32;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + samples * 2).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&16000u32.to_le_bytes());
    bytes.extend_from_slice(&32000u32.to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&(samples * 2).to_le_bytes());
    for index in 0..samples {
        let sample = ((index % 32) as i16 - 16) * 64;
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    std::fs::write(path, bytes).expect("write synthetic audio");
}
