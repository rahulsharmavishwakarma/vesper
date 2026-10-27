// Synthetic paths and names only.
use super::*;
use vesper_core::{ProjectMode, ProjectSource, VocabularyEntry};

const HOME: &str = "/Users/tester";
const CODE: &str = "com.microsoft.VSCode";

fn store() -> (tempfile::TempDir, DatasetStore) {
    let dir = tempfile::tempdir().unwrap();
    let store = DatasetStore::open(dir.path().into()).unwrap();
    let mut profile = store.personalization().unwrap();
    profile.automatic_workspaces = true;
    store.save_personalization(profile).unwrap();
    (dir, store)
}

#[test]
fn detection_is_off_until_enabled() {
    let dir = tempfile::tempdir().unwrap();
    let store = DatasetStore::open(dir.path().into()).unwrap();
    assert!(!store.personalization().unwrap().automatic_workspaces);
    let s = dictate(&store, CODE, "/Users/tester/work/alpha/a.rs", 1);
    assert!(s.workspace.workspace_key.is_empty());
    assert_eq!(store.workspace_state().unwrap(), WorkspaceState::default());
}

#[test]
fn detection_coverage_counts_results_and_clears_when_turned_off() {
    let (_dir, store) = store();
    store
        .record_detection("com.apple.Safari", DetectedDocument::Web, 0.4)
        .unwrap();
    store
        .record_detection("com.apple.Safari", DetectedDocument::None, 12.0)
        .unwrap();
    store
        .record_detection(CODE, DetectedDocument::File, 80.0)
        .unwrap();
    let coverage = store.detection_coverage().unwrap();
    assert_eq!(
        coverage[0],
        DetectionCoverage {
            application: "com.apple.Safari".into(),
            dictations: 2,
            web: 1,
            none: 1,
            latency: [1, 0, 1, 0],
            ..Default::default()
        }
    );
    assert_eq!((coverage[1].file, coverage[1].latency), (1, [0, 0, 0, 1]));
    for n in 0..MAX_DETECTION_APPS {
        store
            .record_detection(&format!("app.{n}"), DetectedDocument::None, 0.1)
            .unwrap();
    }
    assert_eq!(
        store.detection_coverage().unwrap().len(),
        MAX_DETECTION_APPS
    );
    let mut profile = store.personalization().unwrap();
    profile.automatic_workspaces = false;
    store.save_personalization(profile).unwrap();
    assert!(store.detection_coverage().unwrap().is_empty());
}

/// One dictation through the real snapshot path, persisted like the desktop does.
fn dictate(
    store: &DatasetStore,
    app: &str,
    path: &str,
    day: u32,
) -> DictationPersonalizationSnapshot {
    let mut snapshot = store.dictation_personalization_snapshot().unwrap();
    let observation = snapshot.resolve_context(
        Some(app),
        DocumentSignal::File {
            path,
            directory: false,
        },
        Some(HOME),
        day,
    );
    if let Some(o) = observation {
        store.record_workspace_observation(&o, day).unwrap();
    }
    snapshot
}

fn make_visible(store: &DatasetStore, repo: &str) -> String {
    let a = format!("/Users/tester/work/{repo}/a.rs");
    let b = format!("/Users/tester/work/{repo}/src/b.rs");
    for day in 0..3 {
        dictate(store, CODE, &a, day);
        dictate(store, CODE, &b, day);
    }
    store
        .list_workspaces()
        .unwrap()
        .into_iter()
        .find(|w| w.label == repo)
        .expect("visible")
        .key
}

fn word(id: &str, phrase: &str, project: &str) -> VocabularyEntry {
    VocabularyEntry {
        id: id.into(),
        phrase: phrase.into(),
        language: vesper_core::Language::Auto,
        context: String::new(),
        enabled: true,
        aliases: vec![],
        project: project.into(),
        pronunciation: false,
        sources: vec![],
        learned_at: String::new(),
        automatic_source: None,
        confirmed: true,
        usage_count: 0,
    }
}

#[test]
fn dictations_resolve_to_a_visible_workspace_and_freeze_it() {
    let (_dir, store) = store();
    let first = dictate(&store, CODE, "/Users/tester/work/alpha/a.rs", 1);
    assert_eq!(first.project_selection.source, ProjectSource::Category);
    assert_eq!(first.workspace.category, vesper_core::AppCategory::Code);
    assert!(first.workspace.workspace_key.is_empty());
    let key = make_visible(&store, "alpha");
    let snapshot = dictate(&store, CODE, "/Users/tester/work/alpha/src/b.rs", 4);
    assert_eq!(snapshot.project_selection.source, ProjectSource::Workspace);
    assert_eq!(snapshot.project_selection.project, "alpha");
    assert_eq!(snapshot.profile.active_project, "alpha");
    assert_eq!(snapshot.workspace.workspace_key, key);
    // The saved profile keeps the user's own (blank) project.
    assert!(store.personalization().unwrap().active_project.is_empty());
    // Imports never resolve a workspace.
    let import = store.dictation_personalization_snapshot().unwrap();
    assert_eq!(import.project_selection.source, ProjectSource::Shared);
    assert!(import.workspace.workspace_key.is_empty());
}

#[test]
fn stored_state_has_no_paths_and_latent_rows_have_no_names() {
    let (_dir, store) = store();
    dictate(&store, CODE, "/Users/tester/work/secret-name/a.rs", 1);
    dictate(&store, CODE, "/Users/tester/work/secret-name/b.rs", 2);
    let json: String = store
        .connection
        .query_row("SELECT state_json FROM workspace_store", [], |r| r.get(0))
        .unwrap();
    assert!(!json.contains("secret-name") && !json.contains("/Users") && !json.contains(".rs"));
    let state = store.workspace_state().unwrap();
    assert_eq!(state.workspaces.len(), 1);
    assert!(state.workspaces[0].label.is_empty());
}

#[test]
fn manual_project_and_rules_win_and_the_toggle_turns_detection_off() {
    let (_dir, store) = store();
    make_visible(&store, "alpha");
    let mut profile = store.personalization().unwrap();
    profile.active_project = "Manual".into();
    let profile = store.save_personalization(profile).unwrap();
    let s = dictate(&store, CODE, "/Users/tester/work/alpha/a.rs", 5);
    assert_eq!(s.project_selection.source, ProjectSource::Manual);
    let mut profile = profile;
    profile.active_project.clear();
    profile.project_mode = ProjectMode::Automatic;
    profile.project_rules.push(vesper_core::ProjectRule {
        id: "rule".into(),
        application_id: CODE.into(),
        application_name: "Code".into(),
        project: "Rule project".into(),
        folder: None,
        enabled: true,
    });
    let mut profile = store.save_personalization(profile).unwrap();
    let s = dictate(&store, CODE, "/Users/tester/work/alpha/a.rs", 5);
    assert_eq!(s.project_selection.project, "Rule project");
    profile.project_rules.clear();
    profile.automatic_workspaces = false;
    store.save_personalization(profile).unwrap();
    let before = store.workspace_state().unwrap();
    let s = dictate(&store, CODE, "/Users/tester/work/alpha/a.rs", 6);
    assert_eq!(s.project_selection.source, ProjectSource::Shared);
    assert!(s.workspace.workspace_key.is_empty());
    assert_eq!(
        store.workspace_state().unwrap(),
        before,
        "off records nothing"
    );
}

#[test]
fn rename_and_merge_move_scoped_entries_and_delete_leaves_them() {
    let (_dir, store) = store();
    let alpha = make_visible(&store, "alpha");
    let beta = make_visible(&store, "beta");
    let mut profile = store.personalization().unwrap();
    profile.vocabulary = vec![
        word("1", "Kubectl", "alpha"),
        word("2", "Serde", "BETA"),
        word("3", "Shared word", ""),
        word("4", "Other", "gamma"),
    ];
    let profile = store.save_personalization(profile).unwrap();
    let revision = profile.revision;
    let update = store.rename_workspace(&alpha, "Alpha app").unwrap();
    assert!(update.profile_changed);
    let profile = store.personalization().unwrap();
    assert!(profile.revision > revision);
    let project = |id: &str| {
        store
            .personalization()
            .unwrap()
            .vocabulary
            .into_iter()
            .find(|w| w.id == id)
            .unwrap()
            .project
    };
    assert_eq!(project("1"), "Alpha app");
    assert_eq!(project("3"), "");
    assert_eq!(project("4"), "gamma");
    // Dictations now resolve to the new name, so the moved entries apply.
    let s = dictate(&store, CODE, "/Users/tester/work/alpha/a.rs", 7);
    assert_eq!(s.project_selection.project, "Alpha app");
    // A collision is refused; merge is the way to combine.
    assert!(store.rename_workspace(&beta, "alpha APP").is_err());
    let update = store.merge_workspace(&beta, &alpha).unwrap();
    assert!(update.profile_changed);
    assert_eq!(update.workspaces.len(), 1);
    assert_eq!(project("2"), "Alpha app");
    let s = dictate(&store, CODE, "/Users/tester/work/beta/a.rs", 8);
    assert_eq!(s.project_selection.project, "Alpha app");
    // Mining root: explicit and cleared by delete.
    store
        .set_workspace_mining_root(&alpha, Some("/Users/tester/work/alpha".into()))
        .unwrap();
    assert_eq!(store.workspace_mining_roots().unwrap().len(), 1);
    let update = store.delete_workspace(&alpha).unwrap();
    assert!(!update.profile_changed);
    assert!(update.workspaces.is_empty());
    assert!(store.workspace_mining_roots().unwrap().is_empty());
    assert_eq!(project("1"), "Alpha app", "delete keeps scoped entries");
    let s = dictate(&store, CODE, "/Users/tester/work/alpha/a.rs", 9);
    assert_eq!(s.project_selection.source, ProjectSource::Category);
}

#[test]
fn rename_also_moves_learning_candidates() {
    let (_dir, store) = store();
    let alpha = make_visible(&store, "alpha");
    let mut state = store.automatic_learning_state().unwrap();
    state.candidates.push(crate::LearningCandidate {
        id: "c".into(),
        phrase: "Kubectl".into(),
        alias: "cube control".into(),
        language: vesper_core::Language::English,
        project: "alpha".into(),
        context: String::new(),
        state: vesper_core::CandidateState::Observed,
        support: vec![],
        observed_count: 1,
        last_seen_ms: 0,
        reason: String::new(),
    });
    put_state(&store.connection, &state).unwrap();
    let update = store.rename_workspace(&alpha, "Renamed").unwrap();
    assert!(!update.profile_changed, "no vocabulary was scoped");
    assert_eq!(
        store.automatic_learning_state().unwrap().candidates[0].project,
        "Renamed"
    );
}

#[test]
fn the_salt_is_stable_per_install_and_never_exported_in_debug() {
    let (_dir, store) = store();
    let a = store.dictation_personalization_snapshot().unwrap();
    let b = store.dictation_personalization_snapshot().unwrap();
    assert_eq!(a.workspaces.salt, b.workspaces.salt);
    assert!(a.workspaces.salt.len() >= 32);
    assert!(!format!("{a:?}").contains(&a.workspaces.salt));
    let (_other_dir, other) = store_pair();
    let c = other.dictation_personalization_snapshot().unwrap();
    assert_ne!(a.workspaces.salt, c.workspaces.salt);
}

fn store_pair() -> (tempfile::TempDir, DatasetStore) {
    store()
}

/// Opt-in folder → miner (the worker's calls) → inventory → per-recording working
/// set → correction gate, through the real snapshot path.
#[test]
fn an_opted_in_folder_teaches_its_identifiers_to_dictation_in_that_workspace() {
    use crate::term_mining;
    use vesper_core::{correction_gate, Language, TermSource};
    let (_dir, store) = store();
    let repo = tempfile::tempdir().unwrap();
    let git = term_mining::git_binary().expect("git is required for workspace mining");
    let run = |args: &[&str]| {
        assert!(std::process::Command::new(&git)
            .args(args)
            .current_dir(repo.path())
            .output()
            .unwrap()
            .status
            .success());
    };
    run(&["init", "-q"]);
    // Identifiers count only when the repository defines them (or lists them in a
    // manifest or heading); imports alone are not the user's spellings.
    std::fs::write(repo.path().join("a.rs"), "pub struct SherpaOnnx;\n").unwrap();
    std::fs::write(
        repo.path().join("b.rs"),
        "fn load() -> SherpaOnnx { SherpaOnnx }\n",
    )
    .unwrap();
    run(&["add", "a.rs", "b.rs"]);
    let key = make_visible(&store, "speechapp");
    store
        .set_workspace_mining_root(&key, Some(repo.path().to_string_lossy().into_owned()))
        .unwrap();
    for workspace in store.workspace_mining_roots().unwrap() {
        let root = std::path::PathBuf::from(workspace.mining_root.unwrap());
        let mut cursor = Default::default();
        let report = term_mining::scan_workspace(
            &git,
            &root,
            &workspace.key,
            &mut cursor,
            Default::default(),
            &|| false,
        )
        .unwrap();
        store
            .replace_mined_sources(
                &[
                    TermSource::Identifier,
                    TermSource::Manifest,
                    TermSource::Documentation,
                ],
                &workspace.key,
                &report.admitted.unwrap(),
                0,
            )
            .unwrap();
    }
    let correct = |path: &str, raw: &str| {
        let mut snapshot = dictate(&store, CODE, path, 9);
        snapshot.select_working_set(Language::English);
        let set = snapshot.post_asr_working_set(raw, Language::English);
        correction_gate(raw, &set, Language::English).text
    };
    let raw = "Link sherpa onnx before the release";
    assert_eq!(
        correct("/Users/tester/work/speechapp/src/b.rs", raw),
        "Link SherpaOnnx before the release"
    );
    // Another workspace never receives this repository's spellings.
    make_visible(&store, "billing");
    assert_eq!(correct("/Users/tester/work/billing/a.rs", raw), raw);
}
