// Synthetic paths only: no real user folders or documents.
use super::*;

const SALT: &str = "test-salt";
const HOME: &str = "/Users/tester";

fn file(app: &str, path: &str) -> WorkspaceObservation {
    WorkspaceObservation::new(
        SALT,
        app,
        DocumentSignal::File {
            path,
            directory: false,
        },
        Some(HOME),
    )
}
fn dir(app: &str, path: &str) -> WorkspaceObservation {
    WorkspaceObservation::new(
        SALT,
        app,
        DocumentSignal::File {
            path,
            directory: true,
        },
        Some(HOME),
    )
}
fn web(host: &str) -> WorkspaceObservation {
    WorkspaceObservation::new(
        SALT,
        "com.apple.Safari",
        DocumentSignal::WebHost(host),
        Some(HOME),
    )
}
const CODE: &str = "com.microsoft.VSCode";
const TERM: &str = "com.apple.Terminal";
const PAGES: &str = "com.apple.iWork.Pages";

fn visible_label(state: &WorkspaceState, o: &WorkspaceObservation) -> Option<String> {
    state.visible_for(o).map(|w| w.label.clone())
}

#[test]
fn categories_come_from_a_fixed_bundle_table() {
    assert_eq!(app_category("com.apple.dt.Xcode"), AppCategory::Code);
    assert_eq!(app_category("com.jetbrains.rustrover"), AppCategory::Code);
    assert_eq!(
        app_category("com.todesktop.230313mzl4w4u92"),
        AppCategory::Code
    );
    assert_eq!(app_category("com.mitchellh.ghostty"), AppCategory::Terminal);
    assert_eq!(app_category("com.apple.mail"), AppCategory::Email);
    assert_eq!(app_category("com.tinyspeck.slackmacgap"), AppCategory::Chat);
    assert_eq!(app_category("com.apple.iWork.Pages"), AppCategory::Document);
    assert_eq!(app_category("md.obsidian"), AppCategory::Notes);
    assert_eq!(
        app_category("company.thebrowser.Browser"),
        AppCategory::Browser
    );
    assert_eq!(app_category("com.brave.Browser"), AppCategory::Browser);
    assert_eq!(app_category("com.example.unknown"), AppCategory::Other);
    assert!(is_browser("com.apple.Safari") && !is_browser("com.apple.mail"));
}

#[test]
fn keys_are_stable_salted_and_case_insensitive() {
    let a = file(CODE, "/Users/tester/work/alpha/src/main.rs");
    let b = file(CODE, "/Users/tester/work/alpha/src/main.rs");
    let c = file(CODE, "/USERS/tester/Work/Alpha/src/main.rs");
    assert_eq!(a.ancestor_keys(), b.ancestor_keys());
    assert_eq!(a.ancestor_keys(), c.ancestor_keys());
    let other_salt = WorkspaceObservation::new(
        "different",
        CODE,
        DocumentSignal::File {
            path: "/Users/tester/work/alpha/src/main.rs",
            directory: false,
        },
        Some(HOME),
    );
    assert_ne!(a.ancestor_keys(), other_salt.ancestor_keys());
    // `src` is project structure: the chain stops at `alpha`.
    assert_eq!(a.ancestor_keys().len(), 2);
    // A Terminal working directory inside the repo shares the repo key.
    let t = dir(TERM, "/Users/tester/work/alpha");
    assert_eq!(t.ancestor_keys(), a.ancestor_keys());
    let t_src = dir(TERM, "/Users/tester/work/alpha/src");
    assert_eq!(t_src.ancestor_keys(), a.ancestor_keys());
    // Keys are hex hashes, never the folder names.
    for key in a.ancestor_keys() {
        assert_eq!(key.len(), 32);
        assert!(!key.contains("alpha") && !key.contains("work"));
    }
}

#[test]
fn excluded_and_unusable_locations_have_no_target() {
    for path in [
        "/Users/tester/Desktop/note.txt",
        "/Users/tester/Documents/letter.pages",
        "/Users/tester/Downloads/file.pdf",
        "/Users/tester/readme.txt",
        "/Users/tester/Library/Mobile Documents/com~apple~CloudDocs/a.txt",
        "/Users/tester/Library/Mobile Documents/com~apple~CloudDocs/Documents/a.txt",
        "/Users/tester/Library/Containers/x/y.txt",
        "/Applications/Thing.app/Contents/Info.plist",
        "/System/Library/x",
        "/private/var/folders/aa/T/tmp.txt",
        "relative/path.txt",
        "/Users/tester/work/../escape.txt",
    ] {
        assert!(!file(PAGES, path).has_document(), "{path}");
    }
    assert!(!dir(TERM, "/Users/tester").has_document());
    assert!(!dir(TERM, "/Users/tester/Documents").has_document());
    // Folders below the excluded ones are eligible.
    let inside = file(PAGES, "/Users/tester/Documents/Acme/letter.pages");
    assert_eq!(inside.ancestor_keys().len(), 1);
    let icloud = file(
        PAGES,
        "/Users/tester/Library/Mobile Documents/com~apple~CloudDocs/Acme/a.pages",
    );
    assert_eq!(icloud.ancestor_keys().len(), 1);
}

#[test]
fn a_repo_becomes_a_latent_root_then_a_visible_project() {
    let mut s = WorkspaceState::default();
    let readme = file(CODE, "/Users/tester/work/alpha/README.md");
    let main = file(CODE, "/Users/tester/work/alpha/src/main.rs");
    // Day 1: two documents, one day: not yet a root.
    assert_eq!(s.observe(&readme, 100), None);
    assert_eq!(s.observe(&main, 100), None);
    assert!(s.workspaces.is_empty());
    // Day 2: second day registers the anchor as a latent workspace.
    let key = s.observe(&main, 101).expect("root registered");
    let w = s.workspace(&key).unwrap();
    assert!(w.label.is_empty() && !w.visible());
    assert_eq!(w.dictations, 3);
    assert_eq!(visible_label(&s, &main), None);
    s.observe(&readme, 101);
    assert_eq!(visible_label(&s, &main), None);
    s.observe(&main, 101);
    // Fifth dictation over two days: visible with the folder's name.
    assert_eq!(visible_label(&s, &main).as_deref(), Some("alpha"));
    assert_eq!(s.workspace(&key).unwrap().category, AppCategory::Code);
    // A Terminal in the same repo resolves to the same project.
    assert_eq!(
        visible_label(&s, &dir(TERM, "/Users/tester/work/alpha/crates")).as_deref(),
        Some("alpha")
    );
    // The container folder is not a workspace.
    assert_eq!(
        visible_label(&s, &file(CODE, "/Users/tester/work/beta/README.md")),
        None
    );
    s.validate().unwrap();
}

#[test]
fn many_dictations_on_one_day_stay_latent() {
    let mut s = WorkspaceState::default();
    let a = file(PAGES, "/Users/tester/Documents/Acme/a.pages");
    let b = file(PAGES, "/Users/tester/Documents/Acme/b.pages");
    s.observe(&a, 10);
    s.observe(&b, 11);
    assert_eq!(s.workspaces.len(), 1);
    for _ in 0..10 {
        s.observe(&a, 11);
    }
    // 12 dictations over two days → visible; a single-day flood would not.
    assert_eq!(visible_label(&s, &a).as_deref(), Some("Acme"));
    let mut single = WorkspaceState::default();
    for i in 0..20 {
        single.observe(if i % 2 == 0 { &a } else { &b }, 50);
    }
    assert!(
        single.workspaces.is_empty(),
        "one day never qualifies a root"
    );
}

#[test]
fn a_single_document_forms_a_workspace_after_five_dictations_over_two_days() {
    let mut s = WorkspaceState::default();
    let only = file(CODE, "/Users/tester/work/alpha/notes.md");
    for _ in 0..20 {
        assert_eq!(s.observe(&only, 5), None, "one day never qualifies a root");
    }
    assert!(s.workspaces.is_empty());
    assert!(s.observe(&only, 6).is_some());
    assert_eq!(visible_label(&s, &only).as_deref(), Some("alpha"));

    let mut few = WorkspaceState::default();
    for day in 0..4 {
        assert_eq!(few.observe(&only, day), None);
    }
    assert!(few.workspaces.is_empty(), "four dictations stay a node");
    few.observe(&only, 4);
    assert_eq!(visible_label(&few, &only).as_deref(), Some("alpha"));
}

#[test]
fn container_documents_do_not_merge_separate_repos() {
    let mut s = WorkspaceState::default();
    let a1 = file(CODE, "/Users/tester/work/alpha/a.rs");
    let a2 = file(CODE, "/Users/tester/work/alpha/b.rs");
    let b1 = file(CODE, "/Users/tester/work/beta/a.rs");
    let b2 = file(CODE, "/Users/tester/work/beta/b.rs");
    for day in 0..3 {
        for o in [&a1, &a2, &b1, &b2] {
            s.observe(o, day);
        }
    }
    assert_eq!(visible_label(&s, &a1).as_deref(), Some("alpha"));
    assert_eq!(visible_label(&s, &b2).as_deref(), Some("beta"));
}

#[test]
fn duplicate_folder_names_get_distinct_labels() {
    let mut s = WorkspaceState::default();
    let x = [
        file(CODE, "/Users/tester/one/notes/a.md"),
        file(CODE, "/Users/tester/one/notes/b.md"),
    ];
    let y = [
        file(CODE, "/Users/tester/two/notes/a.md"),
        file(CODE, "/Users/tester/two/notes/b.md"),
    ];
    for day in 0..3 {
        for o in x.iter().chain(&y) {
            s.observe(o, day);
        }
    }
    let lx = visible_label(&s, &x[0]).unwrap();
    let ly = visible_label(&s, &y[0]).unwrap();
    assert_ne!(lx.to_lowercase(), ly.to_lowercase());
    assert!(lx.starts_with("notes") && ly.starts_with("notes"));
}

#[test]
fn web_hosts_are_their_own_workspaces_and_keep_only_the_host() {
    assert_eq!(
        normalize_host("WWW.Example.com.").as_deref(),
        Some("example.com")
    );
    assert_eq!(normalize_host("bad host/path"), None);
    let mut s = WorkspaceState::default();
    let docs = web("docs.example.org");
    for day in [1, 1, 2, 2] {
        s.observe(&docs, day);
    }
    assert_eq!(visible_label(&s, &docs), None);
    s.observe(&docs, 2);
    let w = s.visible_for(&docs).unwrap();
    assert_eq!(w.label, "docs.example.org");
    assert_eq!(w.kind, WorkspaceKind::Web);
    assert_eq!(w.category, AppCategory::Browser);
}

#[test]
fn rename_merge_and_delete_follow_the_lifecycle() {
    let mut s = WorkspaceState::default();
    let alpha = [
        file(CODE, "/Users/tester/work/alpha/a.rs"),
        file(CODE, "/Users/tester/work/alpha/b.rs"),
    ];
    let beta = [
        file(CODE, "/Users/tester/work/beta/a.rs"),
        file(CODE, "/Users/tester/work/beta/b.rs"),
    ];
    for day in 0..3 {
        for o in alpha.iter().chain(&beta) {
            s.observe(o, day);
        }
    }
    let alpha_key = s.visible_for(&alpha[0]).unwrap().key.clone();
    let beta_key = s.visible_for(&beta[0]).unwrap().key.clone();
    // Rename: label changes, collisions are refused.
    assert_eq!(s.rename(&alpha_key, " Alpha app ").unwrap(), "alpha");
    assert!(s.rename(&beta_key, "alpha APP").is_err());
    assert!(s.rename(&beta_key, "").is_err());
    assert!(s.rename(&beta_key, &"x".repeat(65)).is_err());
    assert!(s.workspace(&alpha_key).unwrap().renamed);
    // Further dictations never overwrite a user's name.
    s.observe(&alpha[0], 5);
    assert_eq!(visible_label(&s, &alpha[1]).as_deref(), Some("Alpha app"));
    // Merge: beta's documents now resolve to alpha's project.
    let before = s.workspace(&alpha_key).unwrap().dictations;
    let beta_count = s.workspace(&beta_key).unwrap().dictations;
    assert_eq!(
        s.merge(&beta_key, &alpha_key).unwrap(),
        ("beta".to_string(), "Alpha app".to_string())
    );
    assert_eq!(
        s.workspace(&alpha_key).unwrap().dictations,
        before + beta_count
    );
    assert_eq!(visible_label(&s, &beta[0]).as_deref(), Some("Alpha app"));
    assert_eq!(s.observe(&beta[0], 6).as_deref(), Some(alpha_key.as_str()));
    assert!(s.merge(&alpha_key, &alpha_key).is_err());
    assert!(
        s.merge(&beta_key, &alpha_key).is_err(),
        "merged rows are hidden"
    );
    assert_eq!(s.visible().count(), 1);
    // Mining root: explicit, folder workspaces only, cleared by delete.
    s.set_mining_root(&alpha_key, Some("/Users/tester/work/alpha".into()))
        .unwrap();
    assert!(s
        .set_mining_root(&alpha_key, Some("relative".into()))
        .is_err());
    // Delete suppresses: nothing resolves, and it never resurfaces.
    assert_eq!(s.delete(&alpha_key).unwrap(), "Alpha app");
    let w = s.workspace(&alpha_key).unwrap();
    assert!(w.suppressed && w.mining_root.is_none() && w.label.is_empty());
    for day in 7..20 {
        assert_eq!(s.observe(&alpha[0], day), None);
    }
    assert_eq!(visible_label(&s, &alpha[0]), None);
    assert_eq!(visible_label(&s, &beta[0]), None);
    s.validate().unwrap();
}

#[test]
fn resolution_precedence_is_manual_rule_workspace_category_shared() {
    let mut s = WorkspaceState::default();
    let a = file(CODE, "/Users/tester/work/alpha/a.rs");
    let b = file(CODE, "/Users/tester/work/alpha/b.rs");
    for day in 0..3 {
        s.observe(&a, day);
        s.observe(&b, day);
    }
    let shared = ProjectSelection::default();
    let rule = ProjectSelection {
        project: "Rule".into(),
        source: ProjectSource::Application,
        rule_id: Some("r".into()),
    };
    let manual = ProjectSelection {
        project: "Manual".into(),
        source: ProjectSource::Manual,
        rule_id: None,
    };
    let r = resolve_workspace(manual.clone(), true, &s, Some(&a));
    assert_eq!(r.selection, manual);
    assert_eq!(r.context.category, AppCategory::Code);
    assert_eq!(
        resolve_workspace(rule.clone(), true, &s, Some(&a)).selection,
        rule
    );
    let r = resolve_workspace(shared.clone(), true, &s, Some(&a));
    assert_eq!(r.selection.source, ProjectSource::Workspace);
    assert_eq!(r.selection.project, "alpha");
    assert_eq!(r.context.workspace_label, "alpha");
    assert_eq!(r.context.workspace_key.len(), 32);
    // Automatic workspaces off: rules' answer stands, context still carries the app.
    let r = resolve_workspace(shared.clone(), false, &s, Some(&a));
    assert_eq!(r.selection, shared);
    assert!(r.context.workspace_key.is_empty());
    // No workspace: category scope, shared vocabulary.
    let mail = WorkspaceObservation::new(SALT, "com.apple.mail", DocumentSignal::None, Some(HOME));
    let r = resolve_workspace(shared.clone(), true, &s, Some(&mail));
    assert_eq!(r.selection.source, ProjectSource::Category);
    assert!(r.selection.project.is_empty());
    assert_eq!(r.context.category, AppCategory::Email);
    // Unknown app, no document: shared.
    let other = WorkspaceObservation::new(SALT, "com.example.x", DocumentSignal::None, Some(HOME));
    assert_eq!(
        resolve_workspace(shared.clone(), true, &s, Some(&other)).selection,
        shared
    );
    // Abstaining rule outcomes are preserved when no workspace applies.
    let ambiguous = ProjectSelection {
        source: ProjectSource::Ambiguous,
        ..Default::default()
    };
    assert_eq!(
        resolve_workspace(ambiguous.clone(), true, &s, Some(&mail)).selection,
        ambiguous
    );
}

#[test]
fn a_latent_subfolder_root_defers_to_its_visible_parent() {
    let mut s = WorkspaceState::default();
    let top = [
        file(CODE, "/Users/tester/work/alpha/a.md"),
        file(CODE, "/Users/tester/work/alpha/b.md"),
    ];
    for day in 0..3 {
        for o in &top {
            s.observe(o, day);
        }
    }
    let nested = [
        file(CODE, "/Users/tester/work/alpha/bench/x.md"),
        file(CODE, "/Users/tester/work/alpha/bench/y.md"),
    ];
    s.observe(&nested[0], 3);
    s.observe(&nested[1], 4);
    // `bench` is now a latent root, but the visible repo still owns its documents.
    assert_eq!(s.workspaces.len(), 2);
    assert_eq!(visible_label(&s, &nested[0]).as_deref(), Some("alpha"));
}

#[test]
fn state_stays_bounded() {
    let mut s = WorkspaceState::default();
    for i in 0..2000u32 {
        let a = file(PAGES, &format!("/Users/tester/w/f{i}/a.pages"));
        let b = file(PAGES, &format!("/Users/tester/w/f{i}/b.pages"));
        s.observe(&a, i);
        s.observe(&b, i + 1);
        let h = web(&format!("h{i}.example.com"));
        s.observe(&h, i);
    }
    assert!(s.nodes.len() <= MAX_WORKSPACE_NODES);
    assert!(s.workspaces.len() <= MAX_WORKSPACES);
    s.validate().unwrap();
    let json = serde_json::to_string(&s).unwrap();
    assert!(json.len() < 200_000, "{}", json.len());
    assert!(
        !json.contains("example.com"),
        "latent hosts are never labelled"
    );
}

#[test]
fn replaying_an_observation_is_deterministic() {
    let mut a = WorkspaceState::default();
    let o = [
        file(CODE, "/Users/tester/work/alpha/a.rs"),
        file(CODE, "/Users/tester/work/alpha/b.rs"),
    ];
    for day in 0..3 {
        a.observe(&o[0], day);
        a.observe(&o[1], day);
    }
    let mut b = a.clone();
    a.observe(&o[0], 9);
    b.observe(&o[0], 9);
    assert_eq!(a, b);
}
