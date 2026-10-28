//! Background term-inventory worker: one plain thread at background QoS with
//! throttled I/O. It runs once shortly after launch and then hourly, defers while
//! a dictation or model operation is active, the Mac is warm, in Low Power Mode,
//! or (for workspace scans) on battery, and stops within one file when a
//! dictation starts. It records only counts and timings.
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

use tauri::Manager;
use vesper_core::TermSource;
use vesper_data::term_mining::{self, ScanBudget};
use vesper_data::{DatasetStore, TermMiningRun};
use vesper_desktop_platform::{system_conditions, thread_cpu_time, ThermalState};

use crate::AppStorage;

const FIRST_RUN_DELAY: Duration = Duration::from_secs(120);
const INTERVAL: Duration = Duration::from_secs(60 * 60);
const RETRY: Duration = Duration::from_secs(5 * 60);
/// Deferred runs retry until this much of the interval has passed.
const TOLERANCE: Duration = Duration::from_secs(30 * 60);
const CPU_BUDGET: Duration = Duration::from_secs(20);
const CONDITION_CHECK_FILES: usize = 64;

/// Wakes or stops the worker; held in app state so commands can request a run.
#[derive(Default)]
pub struct TermWorker {
    wake: Mutex<bool>,
    signal: Condvar,
    pub(crate) cancel: AtomicBool,
}

impl TermWorker {
    /// Request a run soon (for example after Resume); gates still apply.
    pub fn wake(&self) {
        if let Ok(mut wake) = self.wake.lock() {
            *wake = true;
            self.signal.notify_all();
        }
    }

    fn sleep(&self, duration: Duration) {
        let Ok(guard) = self.wake.lock() else {
            return;
        };
        if let Ok((mut guard, _)) = self
            .signal
            .wait_timeout_while(guard, duration, |woken| !*woken)
        {
            *guard = false;
        }
    }
}

/// Why a run did not start, recorded in the run log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Gate {
    Busy,
    Thermal,
    LowPower,
    Closing,
}

impl Gate {
    fn label(self) -> &'static str {
        match self {
            Self::Busy => "deferred:busy",
            Self::Thermal => "deferred:thermal",
            Self::LowPower => "deferred:low_power",
            Self::Closing => "deferred:closing",
        }
    }
}

fn busy(storage: &AppStorage) -> bool {
    storage.runtime.require_idle().is_err()
        || storage.runtime.background_download_running()
        || crate::dictation::active(storage)
}

fn gate(storage: &AppStorage) -> Option<Gate> {
    if storage.runtime.closing.load(Ordering::Relaxed) {
        return Some(Gate::Closing);
    }
    if busy(storage) {
        return Some(Gate::Busy);
    }
    let conditions = system_conditions();
    if conditions.thermal > ThermalState::Fair {
        return Some(Gate::Thermal);
    }
    conditions.low_power_mode.then_some(Gate::LowPower)
}

/// Developer-only extra mining roots as `(scope, label, path)` from
/// `VESPER_TERM_MINING_ROOTS`: entries separated by `;`, each `scope=path` or a
/// bare path (scope = folder name, which then matches a project of that name).
/// This is the only folder source; ordinary use reads no folders.
pub fn mining_roots() -> Vec<(String, String, PathBuf)> {
    parse_mining_roots(&std::env::var("VESPER_TERM_MINING_ROOTS").unwrap_or_default())
}

fn parse_mining_roots(value: &str) -> Vec<(String, String, PathBuf)> {
    value
        .split(';')
        .filter(|entry| !entry.trim().is_empty())
        .filter_map(|entry| {
            let (scope, path) = match entry.split_once('=') {
                Some((scope, path)) => (Some(scope.trim().to_owned()), path.trim()),
                None => (None, entry.trim()),
            };
            let path = PathBuf::from(path).canonicalize().ok()?;
            let label = path.file_name()?.to_str()?.to_owned();
            Some((scope.unwrap_or_else(|| label.clone()), label, path))
        })
        .take(32)
        .collect()
}

pub fn start(app: tauri::AppHandle) {
    let spawned = std::thread::Builder::new()
        .name("vesper-term-miner".into())
        .spawn(move || {
            vesper_desktop_platform::enter_background_thread();
            let storage = app.state::<AppStorage>();
            let worker = &storage.term_worker;
            worker.sleep(FIRST_RUN_DELAY);
            loop {
                let window = Instant::now();
                let mut gated = gate(&storage);
                while let Some(reason) = gated {
                    if reason == Gate::Closing || worker.cancel.load(Ordering::Relaxed) {
                        return;
                    }
                    if window.elapsed() >= TOLERANCE {
                        log_deferred(&storage, reason);
                        break;
                    }
                    worker.sleep(RETRY);
                    gated = gate(&storage);
                }
                if gated.is_none() {
                    let run = run_once(&storage);
                    if let Ok(store) =
                        DatasetStore::open_shadow_completion(storage.dataset_dir.clone())
                    {
                        let _ = store.record_term_mining_run(&run);
                    }
                }
                worker.sleep(INTERVAL.saturating_sub(window.elapsed()));
                if worker.cancel.load(Ordering::Relaxed) {
                    return;
                }
            }
        });
    if let Err(error) = spawned {
        eprintln!("term worker unavailable: {error}");
    }
}

fn log_deferred(storage: &AppStorage, gate: Gate) {
    if let Ok(store) = DatasetStore::open_shadow_completion(storage.dataset_dir.clone()) {
        let _ = store.record_term_mining_run(&TermMiningRun {
            started_ms: chrono_ms(),
            outcome: gate.label().into(),
            ..TermMiningRun::default()
        });
    }
}

fn chrono_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

/// One bounded run. Tier-A sources always; tier-B scans only on AC power.
pub(crate) fn run_once(storage: &AppStorage) -> TermMiningRun {
    let started = Instant::now();
    let cpu_start = thread_cpu_time();
    let mut run = TermMiningRun {
        started_ms: chrono_ms(),
        outcome: "completed".into(),
        ..TermMiningRun::default()
    };
    let result = mine(storage, &mut run, cpu_start);
    if let Err(error) = result {
        // Store errors only: a short fixed category, never paths or text.
        run.outcome = format!("failed:{}", error_category(&error));
    }
    run.wall_ms = started.elapsed().as_millis() as u64;
    run.cpu_ms = thread_cpu_time().saturating_sub(cpu_start).as_millis() as u64;
    run
}

fn error_category(error: &vesper_data::DatasetError) -> &'static str {
    match error {
        vesper_data::DatasetError::Database(_) => "database",
        vesper_data::DatasetError::Io(_) => "io",
        vesper_data::DatasetError::InvalidConfig(_) => "workspace",
        _ => "other",
    }
}

fn add(run: &mut TermMiningRun, report: &vesper_data::MergeReport) {
    run.terms_added += report.added;
    run.terms_updated += report.updated;
    run.terms_dropped += report.dropped + report.evicted;
}

fn mine(
    storage: &AppStorage,
    run: &mut TermMiningRun,
    cpu_start: Duration,
) -> vesper_data::Result<()> {
    let store = DatasetStore::open_shadow_completion(storage.dataset_dir.clone())?;
    let now = chrono_ms();
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        // A read failure skips the source; only a successful read (including a
        // missing file, which means "no entries") replaces stored evidence.
        if let Ok(replacements) = term_mining::read_text_replacements(
            &home.join("Library/KeyboardServices/TextReplacements.db"),
        ) {
            let terms = term_mining::text_replacement_terms(&replacements);
            add(
                run,
                &store.replace_mined_terms(TermSource::TextReplacement, "", &terms, now)?,
            );
        }
        let local = match std::fs::read_to_string(home.join("Library/Spelling/LocalDictionary")) {
            Ok(text) => Some(term_mining::local_dictionary_terms(&text)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Some(vec![]),
            Err(_) => None,
        };
        if let Some(local) = local {
            add(
                run,
                &store.replace_mined_terms(TermSource::SpellingDictionary, "", &local, now)?,
            );
        }
    }
    // Developer variable only. Scopes no longer listed lose their mined evidence,
    // which also clears words read from folders chosen before the folder
    // opt-in was removed.
    let mut roots = mining_roots();
    roots.truncate(32);
    for scope in store.mined_workspace_scopes()? {
        if !roots.iter().any(|(key, _, _)| *key == scope) {
            add(run, &store.forget_term_workspace(&scope, now)?);
        }
    }
    if roots.is_empty() {
        run.terms_total = store.inventory_terms()?.len();
        return Ok(());
    }
    if system_conditions().on_ac_power != Some(true) {
        run.outcome = "partial:battery".into();
        run.terms_total = store.inventory_terms()?.len();
        return Ok(());
    }
    let Some(git) = term_mining::git_binary() else {
        run.outcome = "partial:no_git".into();
        return Ok(());
    };
    let mut budget = ScanBudget::default();
    let checks = std::cell::Cell::new(0usize);
    let stop = || {
        let n = checks.get() + 1;
        checks.set(n);
        storage.term_worker.cancel.load(Ordering::Relaxed)
            || busy(storage)
            || thread_cpu_time().saturating_sub(cpu_start) >= CPU_BUDGET
            || (n % CONDITION_CHECK_FILES == 0 && {
                let c = system_conditions();
                c.thermal > ThermalState::Fair || c.low_power_mode || c.on_ac_power != Some(true)
            })
    };
    for (scope, label, path) in roots {
        if stop() || budget.max_files == 0 || budget.max_bytes == 0 {
            run.outcome = "partial:budget_or_gate".into();
            break;
        }
        store.set_term_workspace_label(&scope, &label)?;
        let dictionaries = term_mining::mine_workspace_dictionaries(&path, &scope);
        add(
            run,
            &store.replace_mined_terms(
                TermSource::ProjectDictionary,
                &scope,
                &dictionaries,
                now,
            )?,
        );
        let mut cursor = store.workspace_mining_cursor(&scope)?;
        let report = term_mining::scan_workspace(&git, &path, &scope, &mut cursor, budget, &stop)?;
        store.save_workspace_mining_cursor(&scope, &cursor)?;
        run.workspaces_scanned += 1;
        run.workspaces_unchanged += usize::from(report.unchanged);
        run.files_read += report.files_read;
        run.bytes_read += report.bytes_read;
        run.files_skipped += report.files_skipped;
        budget.max_files = budget.max_files.saturating_sub(report.files_read);
        budget.max_bytes = budget.max_bytes.saturating_sub(report.bytes_read);
        if let Some(admitted) = report.admitted {
            add(
                run,
                &store.replace_mined_sources(
                    &[
                        TermSource::Identifier,
                        TermSource::Manifest,
                        TermSource::Documentation,
                    ],
                    &scope,
                    &admitted,
                    chrono_ms(),
                )?,
            );
        }
        if report.stopped {
            run.outcome = "partial:budget_or_gate".into();
            break;
        }
    }
    run.terms_total = store.inventory_terms()?.len();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mining_roots_need_existing_folders_and_default_to_the_folder_name() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("demo-repo");
        std::fs::create_dir(&repo).unwrap();
        let value = format!(
            "{};key-1={};/missing/folder;",
            repo.display(),
            repo.display()
        );
        let roots = parse_mining_roots(&value);
        assert_eq!(roots.len(), 2);
        assert_eq!(
            (roots[0].0.as_str(), roots[0].1.as_str()),
            ("demo-repo", "demo-repo")
        );
        assert_eq!(roots[1].0, "key-1");
        assert!(parse_mining_roots("").is_empty());
    }

    #[test]
    fn wake_interrupts_a_long_sleep() {
        let worker = std::sync::Arc::new(TermWorker::default());
        let sleeper = worker.clone();
        let started = Instant::now();
        let handle = std::thread::spawn(move || sleeper.sleep(Duration::from_secs(30)));
        std::thread::sleep(Duration::from_millis(50));
        worker.wake();
        handle.join().unwrap();
        assert!(started.elapsed() < Duration::from_secs(5));
    }
}
