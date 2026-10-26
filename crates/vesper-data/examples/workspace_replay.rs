//! Synthetic week of shortcut dictations through the real snapshot →
//! `resolve_context` → `record_workspace_observation` path. Paths, hosts and
//! the schedule are invented; no accessibility read, app or user data is used.
//! This measures the workspace algorithm and its storage cost, not live AX
//! coverage. Usage: `workspace_replay [seeds] [scratch-dir]` → JSON on stdout.
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;
use vesper_core::{DocumentSignal, ProjectSource};
use vesper_data::DatasetStore;

const HOME: &str = "/Users/synthetic";

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    fn chance(&mut self, percent: u64) -> bool {
        self.next() % 100 < percent
    }
}

#[derive(Clone, Copy)]
enum Doc {
    File(&'static str),
    Dir(&'static str),
    Host(&'static str),
    Nothing,
}

struct Activity {
    truth: &'static str, // "" = no workspace expected
    weight: u32,
    first_day: u32,
    apps: &'static [&'static str],
    docs: &'static [Doc],
}

const VESPER: &[Doc] = &[
    Doc::File("/Users/synthetic/work/vesper/README.md"),
    Doc::File("/Users/synthetic/work/vesper/src/main.rs"),
    Doc::File("/Users/synthetic/work/vesper/crates/core/src/lib.rs"),
    Doc::File("/Users/synthetic/work/vesper/crates/data/src/store.rs"),
    Doc::File("/Users/synthetic/work/vesper/docs/guide.md"),
    Doc::File("/Users/synthetic/work/vesper/apps/desktop/src/App.svelte"),
    Doc::File("/Users/synthetic/work/vesper/benchmarks/asr/README.md"),
    Doc::File("/Users/synthetic/work/vesper/benchmarks/asr/manifest.json"),
    Doc::Dir("/Users/synthetic/work/vesper/"),
    Doc::Dir("/Users/synthetic/work/vesper/crates/core/"),
];
const BILLING: &[Doc] = &[
    Doc::File("/Users/synthetic/work/billing-api/go.mod"),
    Doc::File("/Users/synthetic/work/billing-api/README.md"),
    Doc::File("/Users/synthetic/work/billing-api/cmd/server/main.go"),
    Doc::File("/Users/synthetic/work/billing-api/internal/invoice/invoice.go"),
    Doc::File("/Users/synthetic/work/billing-api/internal/invoice/tax.go"),
    Doc::Dir("/Users/synthetic/work/billing-api/"),
];
const SITE: &[Doc] = &[
    Doc::File("/Users/synthetic/code/notes-site/config.toml"),
    Doc::File("/Users/synthetic/code/notes-site/content/posts/hello.md"),
    Doc::File("/Users/synthetic/code/notes-site/content/posts/streaming.md"),
    Doc::File("/Users/synthetic/code/notes-site/layouts/index.html"),
    Doc::Dir("/Users/synthetic/code/notes-site/"),
];
const ACME: &[Doc] = &[
    Doc::File("/Users/synthetic/Documents/Clients/Acme/proposal.pages"),
    Doc::File("/Users/synthetic/Documents/Clients/Acme/notes.pages"),
    Doc::File("/Users/synthetic/Documents/Clients/Acme/budget.numbers"),
    Doc::File("/Users/synthetic/Documents/Clients/Acme/Q3/report.pages"),
];
const GITHUB: &[Doc] = &[Doc::Host("github.com")];
const DOCSRS: &[Doc] = &[Doc::Host("docs.rs"), Doc::Host("www.docs.rs")];
const NONE: &[Doc] = &[Doc::Nothing];
const ONE_OFF: &[Doc] = &[
    Doc::File("/Users/synthetic/Downloads/invoice-copy.pdf"),
    Doc::File("/Users/synthetic/Desktop/todo.txt"),
    Doc::File("/Users/synthetic/tmp-letter.txt"),
];

const ACTIVITIES: &[Activity] = &[
    Activity {
        truth: "vesper",
        weight: 30,
        first_day: 0,
        apps: &["com.microsoft.VSCode", "com.apple.Terminal"],
        docs: VESPER,
    },
    Activity {
        truth: "billing-api",
        weight: 18,
        first_day: 0,
        apps: &["com.todesktop.230313mzl4w4u92", "com.apple.Terminal"],
        docs: BILLING,
    },
    Activity {
        truth: "notes-site",
        weight: 10,
        first_day: 2,
        apps: &["com.microsoft.VSCode"],
        docs: SITE,
    },
    Activity {
        truth: "Acme",
        weight: 10,
        first_day: 0,
        apps: &["com.apple.Pages"],
        docs: ACME,
    },
    Activity {
        truth: "github.com",
        weight: 8,
        first_day: 0,
        apps: &["com.apple.Safari"],
        docs: GITHUB,
    },
    Activity {
        truth: "docs.rs",
        weight: 7,
        first_day: 0,
        apps: &["com.brave.Browser"],
        docs: DOCSRS,
    },
    Activity {
        truth: "",
        weight: 10,
        first_day: 0,
        apps: &["com.apple.mail"],
        docs: NONE,
    },
    Activity {
        truth: "",
        weight: 4,
        first_day: 0,
        apps: &["com.tinyspeck.slackmacgap"],
        docs: NONE,
    },
    Activity {
        truth: "",
        weight: 3,
        first_day: 0,
        apps: &["com.apple.TextEdit"],
        docs: ONE_OFF,
    },
];

struct Row {
    day: u32,
    truth: &'static str,
    key: String,
    label: String,
}

fn percentile(values: &mut [f64], p: f64) -> f64 {
    values.sort_by(|a, b| a.partial_cmp(b).unwrap());
    values[((values.len() as f64 * p).ceil() as usize).clamp(1, values.len()) - 1]
}

fn run(seed: u64, dir: &std::path::Path, timings: &mut BTreeMap<&'static str, Vec<f64>>) -> Value {
    let root = dir.join(format!("seed-{seed}"));
    let _ = std::fs::remove_dir_all(&root);
    let store = DatasetStore::open(root.clone()).unwrap();
    let mut rng = Rng(seed.wrapping_mul(0x9E3779B97F4A7C15) | 1);
    let total: u32 = ACTIVITIES.iter().map(|a| a.weight).sum();
    let mut rows = Vec::new();
    let base_day = 740_000;
    for day in 0..7u32 {
        // Weekdays are busier; 6–12 sessions of 1–4 dictations each.
        let sessions = if day >= 5 {
            3 + rng.below(3)
        } else {
            6 + rng.below(7)
        };
        for _ in 0..sessions {
            let mut pick = rng.below(total as usize) as u32;
            let activity = ACTIVITIES
                .iter()
                .find(|a| {
                    if pick < a.weight {
                        true
                    } else {
                        pick -= a.weight;
                        false
                    }
                })
                .unwrap();
            if day < activity.first_day {
                continue;
            }
            for _ in 0..1 + rng.below(4) {
                let app = activity.apps[rng.below(activity.apps.len())];
                let terminal = app == "com.apple.Terminal";
                let candidates: Vec<Doc> = activity
                    .docs
                    .iter()
                    .copied()
                    .filter(|d| {
                        matches!(d, Doc::Dir(_)) == terminal
                            || matches!(d, Doc::Host(_) | Doc::Nothing)
                    })
                    .collect();
                let candidates = if candidates.is_empty() {
                    activity.docs.to_vec()
                } else {
                    candidates
                };
                // 10% of reads find nothing (untitled editor, focus in a panel).
                let doc = if rng.chance(10) {
                    Doc::Nothing
                } else {
                    candidates[rng.below(candidates.len())]
                };
                let signal = match doc {
                    Doc::File(path) => DocumentSignal::File {
                        path,
                        directory: false,
                    },
                    Doc::Dir(path) => DocumentSignal::File {
                        path,
                        directory: true,
                    },
                    Doc::Host(host) => DocumentSignal::WebHost(host),
                    Doc::Nothing => DocumentSignal::None,
                };
                let started = Instant::now();
                let mut snapshot = store.dictation_personalization_snapshot().unwrap();
                timings
                    .entry("snapshotLoadMs")
                    .or_default()
                    .push(started.elapsed().as_secs_f64() * 1e3);
                let started = Instant::now();
                let observation =
                    snapshot.resolve_context(Some(app), signal, Some(HOME), base_day + day);
                timings
                    .entry("resolveContextMs")
                    .or_default()
                    .push(started.elapsed().as_secs_f64() * 1e3);
                if let Some(o) = observation {
                    let started = Instant::now();
                    store
                        .record_workspace_observation(&o, base_day + day)
                        .unwrap();
                    timings
                        .entry("persistObservationMs")
                        .or_default()
                        .push(started.elapsed().as_secs_f64() * 1e3);
                }
                let workspace = snapshot.project_selection.source == ProjectSource::Workspace;
                rows.push(Row {
                    day,
                    truth: activity.truth,
                    key: if workspace {
                        snapshot.workspace.workspace_key.clone()
                    } else {
                        String::new()
                    },
                    label: if workspace {
                        snapshot.workspace.workspace_label.clone()
                    } else {
                        String::new()
                    },
                });
            }
        }
    }
    // Majority truth per produced workspace key (cluster purity).
    let mut by_key: BTreeMap<&str, BTreeMap<&str, usize>> = BTreeMap::new();
    for r in rows.iter().filter(|r| !r.key.is_empty()) {
        *by_key
            .entry(&r.key)
            .or_default()
            .entry(r.truth)
            .or_default() += 1;
    }
    let majority: BTreeMap<&str, &str> = by_key
        .iter()
        .map(|(k, counts)| (*k, *counts.iter().max_by_key(|(_, n)| **n).unwrap().0))
        .collect();
    let assigned: Vec<&Row> = rows.iter().filter(|r| !r.key.is_empty()).collect();
    let correct = |r: &&Row| !r.truth.is_empty() && majority.get(r.key.as_str()) == Some(&r.truth);
    let truthful: Vec<&Row> = rows.iter().filter(|r| !r.truth.is_empty()).collect();
    let mut per_truth = serde_json::Map::new();
    let truths: BTreeSet<&str> = truthful.iter().map(|r| r.truth).collect();
    let mut after_visible = (0usize, 0usize);
    for truth in &truths {
        let mine: Vec<&&Row> = truthful.iter().filter(|r| r.truth == *truth).collect();
        let first = mine.iter().position(|r| correct(r));
        let keys: BTreeSet<&str> = mine
            .iter()
            .filter(|r| !r.key.is_empty())
            .map(|r| r.key.as_str())
            .collect();
        let labels: BTreeSet<&str> = mine
            .iter()
            .filter(|r| !r.label.is_empty())
            .map(|r| r.label.as_str())
            .collect();
        if let Some(i) = first {
            after_visible.0 += mine[i..].iter().filter(|r| correct(r)).count();
            after_visible.1 += mine.len() - i;
        }
        per_truth.insert(
            truth.to_string(),
            json!({
                "dictations": mine.len(),
                "assignedCorrect": mine.iter().filter(|r| correct(r)).count(),
                "visibleAfterDictations": first.map(|i| i + 1),
                "visibleOnDay": first.map(|i| mine[i].day + 1),
                "distinctWorkspaces": keys.len(),
                "labelMatchesTruth": labels.contains(truth),
                "labels": labels.len(),
            }),
        );
    }
    let none_assigned = rows
        .iter()
        .filter(|r| r.truth.is_empty() && !r.key.is_empty())
        .count();
    let state = store.workspace_state().unwrap();
    let state_bytes = serde_json::to_string(&state).unwrap().len();
    drop(store);
    let _ = std::fs::remove_dir_all(&root);
    json!({
        "seed": seed,
        "dictations": rows.len(),
        "workspaceDictations": truthful.len(),
        "assigned": assigned.len(),
        "assignedCorrect": assigned.iter().filter(|r| correct(r)).count(),
        "precision": assigned.iter().filter(|r| correct(r)).count() as f64 / assigned.len().max(1) as f64,
        "recall": truthful.iter().filter(|r| correct(r)).count() as f64 / truthful.len().max(1) as f64,
        "recallAfterVisible": after_visible.0 as f64 / after_visible.1.max(1) as f64,
        "noWorkspaceDictationsAssigned": none_assigned,
        "visibleWorkspaces": state.visible().count(),
        "storedStateBytes": state_bytes,
        "perTruth": per_truth,
    })
}

fn main() {
    let mut args = std::env::args().skip(1);
    let seeds: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(20);
    let dir = args
        .next()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("vesper-workspace-replay"));
    let mut timings = BTreeMap::new();
    let runs: Vec<Value> = (1..=seeds)
        .map(|seed| run(seed, &dir, &mut timings))
        .collect();
    let mean = |field: &str| {
        runs.iter().map(|r| r[field].as_f64().unwrap()).sum::<f64>() / runs.len() as f64
    };
    let min = |field: &str| {
        runs.iter()
            .map(|r| r[field].as_f64().unwrap())
            .fold(f64::INFINITY, f64::min)
    };
    let timing: serde_json::Map<String, Value> = timings
        .iter_mut()
        .map(|(k, v)| {
            (k.to_string(), json!({"n": v.len(), "p50": percentile(v, 0.5), "p95": percentile(v, 0.95), "max": percentile(v, 1.0)}))
        })
        .collect();
    let summary = json!({
        "evidence": "synthetic schedule and paths; real store and resolver code; no live accessibility reads",
        "build": if cfg!(debug_assertions) { "debug" } else { "release" },
        "seeds": seeds,
        "meanPrecision": mean("precision"),
        "minPrecision": min("precision"),
        "meanRecall": mean("recall"),
        "minRecall": min("recall"),
        "meanRecallAfterVisible": mean("recallAfterVisible"),
        "totalNoWorkspaceDictationsAssigned": runs.iter().map(|r| r["noWorkspaceDictationsAssigned"].as_u64().unwrap()).sum::<u64>(),
        "timingMs": timing,
        "runs": runs,
    });
    println!("{}", serde_json::to_string_pretty(&summary).unwrap());
}
