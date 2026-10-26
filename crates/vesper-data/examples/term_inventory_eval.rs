//! Local evaluation harness for the term inventory. Reads the real local sources
//! named on the command line and writes results to `--out`; keep that directory
//! outside the repository because results contain local spellings.
//!
//! ```sh
//! cargo run --release -p vesper-data --example term_inventory_eval -- \
//!   scan --store DIR --out DIR --root NAME=PATH [--root NAME=PATH] [--tier-a]
//! cargo run --release -p vesper-data --example term_inventory_eval -- \
//!   select --store DIR --out DIR [--scope NAME]
//! cargo run --release -p vesper-data --example term_inventory_eval -- \
//!   coverage --store DIR --out DIR --history COPY_OF_DATASET_DIR [--scope NAME]
//! ```
//! Coverage writes counts only, never history text.
use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;
use std::time::Instant;

use vesper_core::{
    ordinary_word, select_working_set, AppCategory, Language, SelectionContext, TermSource,
    TermTier, WorkspaceContext,
};
use vesper_data::term_mining::{self, ScanBudget, WorkspaceCursor};
use vesper_data::DatasetStore;

fn arg_values(args: &[String], name: &str) -> Vec<String> {
    args.windows(2)
        .filter(|w| w[0] == name)
        .map(|w| w[1].clone())
        .collect()
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

fn web2() -> HashSet<String> {
    std::fs::read_to_string("/usr/share/dict/words")
        .unwrap_or_default()
        .lines()
        .map(str::to_lowercase)
        .collect()
}

/// xorshift: reproducible sampling without a dependency.
fn sample<T: Clone>(items: &[T], n: usize, mut seed: u64) -> Vec<T> {
    let mut indices = (0..items.len()).collect::<Vec<_>>();
    for i in (1..indices.len()).rev() {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        indices.swap(i, (seed % (i as u64 + 1)) as usize);
    }
    indices
        .into_iter()
        .take(n)
        .map(|i| items[i].clone())
        .collect()
}

fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    let mode = args.get(1).cloned().unwrap_or_default();
    let store_dir = PathBuf::from(arg_values(&args, "--store").pop().expect("--store"));
    let out = PathBuf::from(arg_values(&args, "--out").pop().expect("--out"));
    std::fs::create_dir_all(&out).unwrap();
    let store = DatasetStore::open(store_dir).unwrap();
    match mode.as_str() {
        "scan" => scan(&args, &store, &out),
        "select" => select(&args, &store, &out),
        "coverage" => coverage(&args, &store, &out),
        _ => panic!("mode: scan | select | coverage"),
    }
}

fn scan(args: &[String], store: &DatasetStore, out: &std::path::Path) {
    let mut report = serde_json::Map::new();
    let started = Instant::now();
    if args.iter().any(|a| a == "--tier-a") {
        let home = PathBuf::from(std::env::var("HOME").unwrap());
        let replacements = term_mining::read_text_replacements(
            &home.join("Library/KeyboardServices/TextReplacements.db"),
        )
        .unwrap_or_default();
        let tr = term_mining::text_replacement_terms(&replacements);
        let local = std::fs::read_to_string(home.join("Library/Spelling/LocalDictionary"))
            .map(|t| term_mining::local_dictionary_terms(&t))
            .unwrap_or_default();
        let vscode = std::fs::read_to_string(
            home.join("Library/Application Support/Code/User/settings.json"),
        )
        .map(|t| term_mining::parse_vscode_cspell_words(&t))
        .unwrap_or_default();
        report.insert(
            "tierA".into(),
            serde_json::json!({
                "textReplacementRows": replacements.len(),
                "textReplacementTerms": tr.len(),
                "localDictionaryTerms": local.len(),
                "vscodeUserCspellWords": vscode.len(),
            }),
        );
        store
            .replace_mined_terms(TermSource::TextReplacement, "", &tr, now_ms())
            .unwrap();
        store
            .replace_mined_terms(TermSource::SpellingDictionary, "", &local, now_ms())
            .unwrap();
    }
    let git = term_mining::git_binary().expect("git");
    let dict = web2();
    let mut all_b = Vec::new();
    for root in arg_values(args, "--root") {
        let (scope, path) = root.split_once('=').expect("NAME=PATH");
        let dictionaries = term_mining::mine_workspace_dictionaries(path.as_ref(), scope);
        store
            .replace_mined_terms(
                TermSource::ProjectDictionary,
                scope,
                &dictionaries,
                now_ms(),
            )
            .unwrap();
        let mut cursor: WorkspaceCursor = store.workspace_mining_cursor(scope).unwrap();
        let t0 = Instant::now();
        let never = || false;
        let budget = if args.iter().any(|a| a == "--unbounded") {
            ScanBudget {
                max_files: usize::MAX,
                max_bytes: u64::MAX,
            }
        } else {
            ScanBudget::default()
        };
        let scan =
            term_mining::scan_workspace(&git, path.as_ref(), scope, &mut cursor, budget, &never)
                .unwrap();
        let wall_ms = t0.elapsed().as_secs_f64() * 1000.0;
        store.save_workspace_mining_cursor(scope, &cursor).unwrap();
        let mut entry = serde_json::json!({
            "filesListed": scan.files_listed, "filesRead": scan.files_read,
            "bytesRead": scan.bytes_read, "filesSkipped": scan.files_skipped,
            "unchanged": scan.unchanged, "completed": scan.completed, "stopped": scan.stopped,
            "wallMs": wall_ms, "projectDictionaryTerms": dictionaries.len(),
        });
        if let Some(admitted) = scan.admitted {
            let mut by_source = BTreeMap::new();
            for t in &admitted {
                *by_source.entry(format!("{:?}", t.source)).or_insert(0usize) += 1;
            }
            // The surface itself is an ordinary word (lexicon, any case).
            let ordinary = admitted
                .iter()
                .filter(|t| ordinary_word(&t.surface))
                .count();
            // Every separated or camelCase part is ordinary (`relative_path`).
            let ordinary_parts = admitted
                .iter()
                .filter(|t| {
                    let spoken = vesper_core::identifier_spoken_form(&t.surface)
                        .unwrap_or_else(|| t.surface.to_lowercase());
                    spoken.split(' ').all(ordinary_word)
                })
                .count();
            let in_web2 = admitted
                .iter()
                .filter(|t| dict.contains(&t.surface.to_lowercase()))
                .count();
            let merge = store
                .replace_mined_sources(
                    &[
                        TermSource::Identifier,
                        TermSource::Manifest,
                        TermSource::Documentation,
                    ],
                    scope,
                    &admitted,
                    now_ms(),
                )
                .unwrap();
            entry["admitted"] = admitted.len().into();
            entry["admittedBySource"] = serde_json::to_value(&by_source).unwrap();
            entry["admittedOrdinaryCmudict"] = ordinary.into();
            entry["admittedAllPartsOrdinary"] = ordinary_parts.into();
            entry["admittedInWeb2"] = in_web2.into();
            entry["merge"] = serde_json::to_value(&merge).unwrap();
            let lines = admitted
                .iter()
                .map(|t| {
                    format!(
                        "{}\t{:?}\t{}\t{}",
                        t.surface,
                        t.source,
                        t.count,
                        t.spoken.join("|")
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            std::fs::write(out.join(format!("admitted-{scope}.tsv")), lines).unwrap();
            all_b.extend(admitted.into_iter().map(|t| (scope.to_owned(), t)));
        }
        report.insert(scope.to_owned(), entry);
    }
    if !all_b.is_empty() {
        let picked = sample(&all_b, 50, 0x5eed_1234_abcd);
        let lines = picked
            .iter()
            .map(|(scope, t)| format!("{scope}\t{}\t{:?}\t{}", t.surface, t.source, t.count))
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(out.join("sample-50.tsv"), lines).unwrap();
    }
    report.insert(
        "totalWallMs".into(),
        (started.elapsed().as_secs_f64() * 1000.0).into(),
    );
    report.insert(
        "inventory".into(),
        serde_json::to_value(store.term_inventory_summary(0).unwrap()).unwrap(),
    );
    let name = arg_values(args, "--name")
        .pop()
        .unwrap_or_else(|| "scan".into());
    std::fs::write(
        out.join(format!("{name}.json")),
        serde_json::to_string_pretty(&report).unwrap(),
    )
    .unwrap();
    println!("{}", serde_json::to_string_pretty(&report).unwrap());
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    sorted[((sorted.len() as f64 - 1.0) * p).round() as usize]
}

fn select(args: &[String], store: &DatasetStore, out: &std::path::Path) {
    let pad = arg_values(args, "--pad-to")
        .pop()
        .map_or(0, |v| v.parse::<usize>().unwrap());
    let mut timings = serde_json::Map::new();
    if let Some(target) = arg_values(args, "--pad-store").pop() {
        // Fill the (scratch) store with synthetic rows to measure a full load.
        let target = target.parse::<usize>().unwrap();
        let have = store.inventory_terms().unwrap().len();
        let synthetic = (have..target)
            .map(|i| {
                term_mining::MinedTerm::new(
                    &format!("synthTerm{i}"),
                    TermSource::Identifier,
                    "synthetic",
                    2 + (i % 17) as u32,
                )
            })
            .collect::<Vec<_>>();
        store
            .replace_mined_sources(&[TermSource::Identifier], "synthetic", &synthetic, now_ms())
            .unwrap();
    }
    let t0 = Instant::now();
    let mut terms = store.inventory_terms().unwrap();
    timings.insert(
        "loadMs".into(),
        (t0.elapsed().as_secs_f64() * 1000.0).into(),
    );
    timings.insert("realTerms".into(), terms.len().into());
    // Pad with synthetic identifiers in synthetic scopes to reach the target size.
    let real = terms.len();
    let pad_scope = arg_values(args, "--pad-scope").pop();
    for i in real..pad {
        terms.push(vesper_core::InventoryTerm {
            id: format!("syn:{i}"),
            surface: format!("synthTerm{i}"),
            spoken: vec![format!("synth term{i}")],
            kind: vesper_core::TermKind::Identifier,
            tier: TermTier::Authored,
            sources: vec![TermSource::Identifier],
            scopes: vec![pad_scope
                .clone()
                .unwrap_or_else(|| format!("synthetic-{}", i % 40))],
            language: Language::Auto,
            evidence: (i % 17) as u32 + 2,
            last_seen_ms: now_ms() - (i as i64 * 60_000),
        });
    }
    timings.insert("selectorInputTerms".into(), terms.len().into());
    let scope = arg_values(args, "--scope").pop().unwrap_or_default();
    let contexts = [
        ("code-repo", AppCategory::Code, scope.clone()),
        ("email", AppCategory::Email, String::new()),
        ("shared", AppCategory::Other, String::new()),
    ];
    let raw = arg_values(args, "--raw").pop().unwrap_or_else(|| {
        "please update the working set selector so the sherpa onnx hook reads the term inventory".into()
    });
    let mut sets = serde_json::Map::new();
    for (name, category, key) in &contexts {
        let context = SelectionContext {
            workspace: WorkspaceContext {
                application_id: "eval".into(),
                category: *category,
                workspace_key: key.clone(),
                workspace_label: String::new(),
            },
            project: String::new(),
            language: Language::English,
            now_ms: now_ms(),
        };
        let mut pre = Vec::new();
        let mut post = Vec::new();
        for _ in 0..200 {
            let t = Instant::now();
            std::hint::black_box(select_working_set(&terms, &context, None));
            pre.push(t.elapsed().as_secs_f64() * 1000.0);
            let t = Instant::now();
            std::hint::black_box(select_working_set(&terms, &context, Some(&raw)));
            post.push(t.elapsed().as_secs_f64() * 1000.0);
        }
        pre.sort_by(f64::total_cmp);
        post.sort_by(f64::total_cmp);
        let set = select_working_set(&terms, &context, Some(&raw));
        sets.insert(
            (*name).into(),
            serde_json::json!({
                "preP50Ms": percentile(&pre, 0.5), "preP95Ms": percentile(&pre, 0.95),
                "postP50Ms": percentile(&post, 0.5), "postP95Ms": percentile(&post, 0.95),
                "workingSet": set,
            }),
        );
    }
    timings.insert("contexts".into(), sets.into());
    if let Some(path) = arg_values(args, "--dump-terms").pop() {
        // Eligible real rows for the code-repo context (selector input) so a
        // replay can compute per-utterance post-ASR normalizations.
        let context = SelectionContext {
            workspace: WorkspaceContext {
                application_id: "eval".into(),
                category: AppCategory::Code,
                workspace_key: scope.clone(),
                workspace_label: String::new(),
            },
            project: String::new(),
            language: Language::English,
            now_ms: now_ms(),
        };
        let real = &terms[..real];
        let ranked = vesper_core::rank_terms(real, &context);
        std::fs::write(path, serde_json::to_string_pretty(&ranked).unwrap()).unwrap();
    }
    let name = arg_values(args, "--name")
        .pop()
        .unwrap_or_else(|| "select".into());
    std::fs::write(
        out.join(format!("{name}.json")),
        serde_json::to_string_pretty(&timings).unwrap(),
    )
    .unwrap();
    println!("{}", serde_json::to_string_pretty(&timings).unwrap());
}

fn words_of(text: &str) -> Vec<String> {
    text.split(|c: char| c.is_whitespace() || ",.;:!?()[]{}\"“”‘’".contains(c))
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_owned())
        .filter(|w| w.chars().filter(|c| c.is_alphabetic()).count() >= 3)
        .collect()
}

/// Would the inventory have contained the owner's terms? Counts only.
fn coverage(args: &[String], store: &DatasetStore, out: &std::path::Path) {
    let history = DatasetStore::open(PathBuf::from(
        arg_values(args, "--history").pop().expect("--history"),
    ))
    .unwrap();
    let scope = arg_values(args, "--scope").pop().unwrap_or_default();
    let terms = store.inventory_terms().unwrap();
    let mut surfaces = HashSet::new();
    let mut scoped = HashSet::new();
    let mut spoken = HashSet::new();
    for t in &terms {
        surfaces.insert(t.surface.to_lowercase());
        if t.scopes.contains(&scope) {
            scoped.insert(t.surface.to_lowercase());
        }
        spoken.extend(t.spoken.iter().cloned());
    }
    let sessions = history.list_sessions().unwrap();
    let mut kept_terms = HashSet::new();
    let mut edited_terms = HashSet::new();
    let mut kept_text = String::new();
    let mut with_draft = 0;
    for s in &sessions {
        let machine = s
            .personalized_transcript
            .clone()
            .or_else(|| s.cleaned_transcript.clone())
            .or_else(|| s.raw_transcript.clone())
            .unwrap_or_default();
        let user = s
            .draft_transcript
            .clone()
            .or_else(|| s.corrected_transcript.clone());
        let kept = user.clone().unwrap_or_else(|| machine.clone());
        kept_text.push_str(&kept.to_lowercase());
        kept_text.push('\n');
        for w in words_of(&kept) {
            if !ordinary_word(&w) {
                kept_terms.insert(w.to_lowercase());
            }
        }
        if let Some(user) = user {
            with_draft += 1;
            let before = words_of(&machine)
                .into_iter()
                .map(|w| w.to_lowercase())
                .collect::<HashSet<_>>();
            for w in words_of(&user) {
                if !before.contains(&w.to_lowercase()) && !ordinary_word(&w) {
                    edited_terms.insert(w.to_lowercase());
                }
            }
        }
    }
    let covered = |set: &HashSet<String>, inv: &HashSet<String>| {
        set.iter().filter(|w| inv.contains(*w)).count()
    };
    // Inventory spellings or spoken forms that occur in any kept text.
    let padded = format!(
        " {} ",
        kept_text.replace(|c: char| !c.is_alphanumeric() && c != '_' && c != '-', " ")
    );
    let said = terms
        .iter()
        .filter(|t| {
            padded.contains(&format!(" {} ", t.surface.to_lowercase()))
                || t.spoken.iter().any(|s| padded.contains(&format!(" {s} ")))
        })
        .count();
    let report = serde_json::json!({
        "sessions": sessions.len(),
        "sessionsWithUserText": with_draft,
        "inventoryTerms": terms.len(),
        "inventoryTermsInScope": scoped.len(),
        "keptNonOrdinaryDistinct": kept_terms.len(),
        "keptNonOrdinaryInInventory": covered(&kept_terms, &surfaces),
        "keptNonOrdinaryInScope": covered(&kept_terms, &scoped),
        "userIntroducedNonOrdinaryDistinct": edited_terms.len(),
        "userIntroducedInInventory": covered(&edited_terms, &surfaces),
        "inventoryTermsOccurringInKeptText": said,
        "spokenFormsTotal": spoken.len(),
    });
    std::fs::write(
        out.join("coverage.json"),
        serde_json::to_string_pretty(&report).unwrap(),
    )
    .unwrap();
    println!("{}", serde_json::to_string_pretty(&report).unwrap());
}
