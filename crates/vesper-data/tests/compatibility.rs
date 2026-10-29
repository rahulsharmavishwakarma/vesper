//! Stored data must survive version changes in both directions (issue #362).
//!
//! - Upgrades: a dataset written by each released version still loads. The
//!   dumps in `fixtures/datasets/` come from `examples/compatibility_fixture.rs`
//!   run with that release's code, and every release in CHANGELOG.md needs one.
//! - Rollbacks: data written by a newer version, simulated by adding unknown
//!   fields everywhere and replacing enum values one at a time, still loads,
//!   so a build released now keeps working after its users try a later one.

#[path = "../examples/compatibility_fixture.rs"]
mod fixture;

use std::path::{Path, PathBuf};

use rusqlite::Connection;
use serde_json::Value;
use vesper_data::DatasetStore;

const DATABASE: &str = "sessions.sqlite3";

#[derive(Debug, PartialEq)]
struct Loaded {
    sessions: usize,
    words: usize,
    candidates: usize,
}

/// Every read the app makes at start, in History and when recording. Any
/// error here would block a user, so each must succeed.
fn load_everything(root: &Path) -> Result<Loaded, String> {
    let store = DatasetStore::open(root.to_path_buf()).map_err(|e| format!("open: {e}"))?;
    let sessions = store
        .list_sessions()
        .map_err(|e| format!("sessions: {e}"))?;
    let profile = store
        .personalization()
        .map_err(|e| format!("profile: {e}"))?;
    let learning = store
        .automatic_learning_state()
        .map_err(|e| format!("learning: {e}"))?;
    store
        .learning_dashboard()
        .map_err(|e| format!("dashboard: {e}"))?;
    store
        .inventory_terms()
        .map_err(|e| format!("inventory: {e}"))?;
    store
        .term_mining_runs(50)
        .map_err(|e| format!("mining runs: {e}"))?;
    store
        .semantic_shadow_records()
        .map_err(|e| format!("semantic: {e}"))?;
    store
        .policy_shadow_state()
        .map_err(|e| format!("policy: {e}"))?;
    store
        .list_workspaces()
        .map_err(|e| format!("workspaces: {e}"))?;
    store
        .dictation_personalization_snapshot()
        .map_err(|e| format!("dictation snapshot: {e}"))?;
    Ok(Loaded {
        sessions: sessions.len(),
        words: profile.vocabulary.len(),
        candidates: learning.candidates.len(),
    })
}

fn fixtures() -> Vec<(String, PathBuf)> {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/datasets");
    let mut found: Vec<_> = std::fs::read_dir(&directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "sql"))
        .map(|path| {
            (
                path.file_stem().unwrap().to_string_lossy().into_owned(),
                path,
            )
        })
        .collect();
    found.sort();
    found
}

#[test]
fn every_released_version_has_a_dataset_fixture() {
    let changelog =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../CHANGELOG.md"))
            .unwrap();
    let have: Vec<String> = fixtures().into_iter().map(|(name, _)| name).collect();
    for line in changelog.lines() {
        let Some(version) = line
            .strip_prefix("## [")
            .and_then(|rest| rest.split_once(']'))
            .map(|(version, _)| version)
            .filter(|version| *version != "Unreleased")
        else {
            continue;
        };
        assert!(
            have.contains(&format!("v{version}")),
            "add crates/vesper-data/tests/fixtures/datasets/v{version}.sql \
             (see examples/compatibility_fixture.rs)"
        );
    }
}

#[test]
fn datasets_written_by_released_versions_still_load() {
    let fixtures = fixtures();
    assert!(!fixtures.is_empty());
    for (name, dump) in fixtures {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("data");
        std::fs::create_dir_all(&root).unwrap();
        Connection::open(root.join(DATABASE))
            .unwrap()
            .execute_batch(&std::fs::read_to_string(dump).unwrap())
            .unwrap();
        let loaded = load_everything(&root).unwrap_or_else(|error| panic!("{name}: {error}"));
        assert_eq!(loaded.sessions, 1, "{name}");
        assert!(loaded.words >= 1, "{name}: the manual word is kept");
        // Upgraded data can be saved again and reread by this build.
        let store = DatasetStore::open(root.clone()).unwrap();
        let profile = store.dictation_personalization().unwrap();
        store.save_personalization(profile).unwrap();
        drop(store);
        assert_eq!(load_everything(&root).unwrap().sessions, 1, "{name}");
    }
}

/// Each JSON document stored in any table, by table, column and row.
fn stored_documents(database: &Path) -> Vec<(String, String, i64, Value)> {
    let connection = Connection::open(database).unwrap();
    let tables: Vec<String> = connection
        .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    let mut documents = Vec::new();
    for table in tables {
        let columns: Vec<String> = connection
            .prepare(&format!("SELECT name FROM pragma_table_info('{table}')"))
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        for column in columns {
            let mut statement = connection
                .prepare(&format!("SELECT rowid, \"{column}\" FROM \"{table}\""))
                .unwrap();
            let rows = statement
                .query_map([], |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, rusqlite::types::Value>(1)?,
                    ))
                })
                .unwrap();
            for row in rows {
                let (rowid, cell) = row.unwrap();
                if let rusqlite::types::Value::Text(text) = cell {
                    if let Ok(value @ (Value::Object(_) | Value::Array(_))) =
                        serde_json::from_str::<Value>(&text)
                    {
                        documents.push((table.clone(), column.clone(), rowid, value));
                    }
                }
            }
        }
    }
    documents
}

fn store_document(database: &Path, table: &str, column: &str, rowid: i64, value: &Value) {
    Connection::open(database)
        .unwrap()
        .execute(
            &format!("UPDATE \"{table}\" SET \"{column}\" = ?1 WHERE rowid = ?2"),
            rusqlite::params![value.to_string(), rowid],
        )
        .unwrap();
}

fn add_unknown_fields(value: &mut Value) {
    match value {
        Value::Object(map) => {
            map.values_mut().for_each(add_unknown_fields);
            map.insert("fieldFromANewerVersion".into(), Value::Bool(true));
        }
        Value::Array(items) => items.iter_mut().for_each(add_unknown_fields),
        _ => {}
    }
}

/// Paths to string values shaped like serialized enum variants.
fn enum_like_strings(value: &Value, path: &mut Vec<String>, out: &mut Vec<Vec<String>>) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                path.push(key.clone());
                enum_like_strings(child, path, out);
                path.pop();
            }
        }
        Value::Array(items) => {
            for (index, child) in items.iter().enumerate() {
                path.push(index.to_string());
                enum_like_strings(child, path, out);
                path.pop();
            }
        }
        Value::String(text)
            if (1..=40).contains(&text.len())
                && text.starts_with(|c: char| c.is_ascii_lowercase())
                && text
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') =>
        {
            out.push(path.clone());
        }
        _ => {}
    }
}

fn sample_dataset() -> (tempfile::TempDir, PathBuf, Vec<u8>) {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("data");
    std::fs::create_dir_all(&root).unwrap();
    fixture::write(&root);
    let pristine = std::fs::read(root.join(DATABASE)).unwrap();
    (directory, root, pristine)
}

#[test]
fn data_with_fields_from_a_newer_version_loads_without_losing_records() {
    let (_directory, root, _) = sample_dataset();
    let database = root.join(DATABASE);
    let before = load_everything(&root).unwrap();
    let documents = stored_documents(&database);
    assert!(
        documents.len() >= 3,
        "sessions, profile and learning are stored"
    );
    for (table, column, rowid, mut value) in documents {
        add_unknown_fields(&mut value);
        store_document(&database, &table, &column, rowid, &value);
    }
    assert_eq!(load_everything(&root).unwrap(), before);
}

#[test]
fn data_with_values_from_a_newer_version_never_blocks_loading() {
    let (_directory, root, pristine) = sample_dataset();
    let database = root.join(DATABASE);
    let mut checked = 0;
    for (table, column, rowid, value) in stored_documents(&database) {
        let mut paths = Vec::new();
        enum_like_strings(&value, &mut Vec::new(), &mut paths);
        for path in paths {
            std::fs::write(&database, &pristine).unwrap();
            let mut changed = value.clone();
            let pointer = format!("/{}", path.join("/"));
            *changed.pointer_mut(&pointer).unwrap() =
                Value::String("valueFromANewerVersion".into());
            store_document(&database, &table, &column, rowid, &changed);
            if let Err(error) = load_everything(&root) {
                panic!("{table}.{column}{pointer} = valueFromANewerVersion: {error}");
            }
            checked += 1;
        }
    }
    assert!(checked > 20, "only {checked} values were exercised");
}
