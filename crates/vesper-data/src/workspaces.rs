//! Automatic workspace storage: a per-install random salt plus the bounded
//! lattice of salted folder/host keys, day stamps and counts. Raw document
//! paths, URLs and window titles are never written. A workspace label (folder
//! name or web host) is written only once the workspace becomes visible.
//!
//! Scoping: the visible workspace's label is its project name. Vocabulary,
//! examples and learning candidates are scoped by that name, so rename and
//! merge move every entry scoped to the old name in the same transaction.
use crate::learning::{put_profile, put_state};
use crate::*;
use vesper_core::{
    resolve_project, resolve_workspace, AppCategory, DocumentSignal, PersonalizationProfile,
    WorkspaceKind, WorkspaceObservation, WorkspaceState,
};

/// The per-operation workspace inputs frozen with the personalization snapshot.
#[derive(Clone, Default)]
pub struct WorkspaceSnapshot {
    salt: String,
    state: WorkspaceState,
}

impl std::fmt::Debug for WorkspaceSnapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkspaceSnapshot")
            .field("salt", &"<redacted>")
            .field("workspaces", &self.state.workspaces.len())
            .finish()
    }
}

/// A visible workspace as shown in Settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceSummary {
    pub key: String,
    pub label: String,
    pub kind: WorkspaceKind,
    pub category: AppCategory,
    pub dictations: u32,
    pub first_seen_day: u32,
    pub last_seen_day: u32,
    pub renamed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceUpdate {
    pub workspaces: Vec<WorkspaceSummary>,
    /// Scoped vocabulary/examples moved; the profile revision changed.
    pub profile_changed: bool,
}

/// What one dictation's bounded Accessibility read found in the frontmost app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetectedDocument {
    None,
    File,
    Folder,
    Web,
}

/// Per-app detection coverage measured on real dictations while automatic
/// workspaces are on: the app identifier, counts by result and read-latency
/// buckets only. Never a path, host, title or text. Cleared when detection is
/// turned off.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectionCoverage {
    pub application: String,
    pub dictations: u32,
    pub file: u32,
    pub folder: u32,
    pub web: u32,
    pub none: u32,
    /// Read latency: under 1 ms, under 5 ms, under 30 ms, slower.
    pub latency: [u32; 4],
}

/// Distinct apps kept in the coverage table; further apps are not recorded.
pub const MAX_DETECTION_APPS: usize = 64;

/// Local calendar day used for the "seen on two days" rules.
pub fn workspace_day() -> u32 {
    use chrono::Datelike;
    chrono::Local::now().date_naive().num_days_from_ce().max(0) as u32
}

fn new_salt() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

fn summaries(state: &WorkspaceState) -> Vec<WorkspaceSummary> {
    let mut out: Vec<_> = state
        .visible()
        .map(|w| WorkspaceSummary {
            key: w.key.clone(),
            label: w.label.clone(),
            kind: w.kind,
            category: w.category,
            dictations: w.dictations,
            first_seen_day: w.first_seen_day,
            last_seen_day: w.last_seen_day,
            renamed: w.renamed,
        })
        .collect();
    out.sort_by(|a, b| {
        b.last_seen_day
            .cmp(&a.last_seen_day)
            .then(b.dictations.cmp(&a.dictations))
            .then(a.label.cmp(&b.label))
    });
    out
}

/// Parse stored state, dropping `miningRoot` (the folder opt-in removed on
/// 28 October 2026). The flag says whether a retired field was present.
fn parse_state(json: &str) -> Result<(WorkspaceState, bool)> {
    let mut value: serde_json::Value = serde_json::from_str(json)?;
    let mut retired = false;
    if let Some(list) = value.get_mut("workspaces").and_then(|v| v.as_array_mut()) {
        for w in list {
            retired |= w
                .as_object_mut()
                .and_then(|w| w.remove("miningRoot"))
                .is_some();
        }
    }
    Ok((stored::decode_value(&mut value)?, retired))
}

fn same_project(entry: &str, name: &str) -> bool {
    !name.trim().is_empty() && entry.trim().to_lowercase() == name.trim().to_lowercase()
}

impl DatasetStore {
    fn read_workspace_row(&self) -> Result<Option<(String, WorkspaceState)>> {
        let row: Option<(String, String)> = self
            .connection
            .query_row(
                "SELECT salt, state_json FROM workspace_store WHERE id = 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let Some((salt, json)) = row else {
            return Ok(None);
        };
        let state = parse_state(&json)?.0;
        state.validate().map_err(DatasetError::InvalidConfig)?;
        if salt.len() < 32 {
            return Err(DatasetError::InvalidConfig(
                "Invalid workspace salt.".into(),
            ));
        }
        Ok(Some((salt, state)))
    }

    fn write_workspace_state(&self, state: &WorkspaceState) -> Result<()> {
        state.validate().map_err(DatasetError::InvalidConfig)?;
        self.connection.execute(
            "UPDATE workspace_store SET state_json = ?1 WHERE id = 1",
            [serde_json::to_string(state)?],
        )?;
        Ok(())
    }

    /// Create the per-install salt once. Never exported or shown. Also erases
    /// a folder path stored by the retired folder opt-in.
    pub fn ensure_workspace_store(&self) -> Result<()> {
        self.connection.execute(
            "INSERT OR IGNORE INTO workspace_store (id, salt, state_json) VALUES (1, ?1, ?2)",
            params![
                new_salt(),
                serde_json::to_string(&WorkspaceState::default())?
            ],
        )?;
        let json: String = self.connection.query_row(
            "SELECT state_json FROM workspace_store WHERE id = 1",
            [],
            |r| r.get(0),
        )?;
        if let (state, true) = parse_state(&json)? {
            self.write_workspace_state(&state)?;
        }
        Ok(())
    }

    pub fn workspace_state(&self) -> Result<WorkspaceState> {
        Ok(self.read_workspace_row()?.map(|r| r.1).unwrap_or_default())
    }

    pub(crate) fn workspace_snapshot(&self) -> Result<WorkspaceSnapshot> {
        Ok(self
            .read_workspace_row()?
            .map(|(salt, state)| WorkspaceSnapshot { salt, state })
            .unwrap_or_default())
    }

    /// Persist one dictation's observation (off the main thread). Re-applies it
    /// to the latest stored state so a concurrent rename is never overwritten.
    pub fn record_workspace_observation(
        &self,
        observation: &WorkspaceObservation,
        day: u32,
    ) -> Result<Option<String>> {
        let tx = self.connection.unchecked_transaction()?;
        let Some((_, mut state)) = self.read_workspace_row()? else {
            return Ok(None);
        };
        let key = state.observe(observation, day);
        self.write_workspace_state(&state)?;
        tx.commit()?;
        Ok(key)
    }

    /// Count one dictation's detection result for its frontmost app.
    pub fn record_detection(
        &self,
        application: &str,
        found: DetectedDocument,
        read_ms: f64,
    ) -> Result<()> {
        let known: Option<i64> = self
            .connection
            .query_row(
                "SELECT 1 FROM workspace_detection WHERE application = ?1",
                [application],
                |row| row.get(0),
            )
            .optional()?;
        let apps: i64 =
            self.connection
                .query_row("SELECT COUNT(*) FROM workspace_detection", [], |row| {
                    row.get(0)
                })?;
        if application.is_empty() || (known.is_none() && apps as usize >= MAX_DETECTION_APPS) {
            return Ok(());
        }
        let kind = match found {
            DetectedDocument::None => "none",
            DetectedDocument::File => "file",
            DetectedDocument::Folder => "folder",
            DetectedDocument::Web => "web",
        };
        let bucket = match read_ms {
            ms if ms < 1.0 => "under_1ms",
            ms if ms < 5.0 => "under_5ms",
            ms if ms < 30.0 => "under_30ms",
            _ => "slower",
        };
        self.connection.execute(
            "INSERT OR IGNORE INTO workspace_detection VALUES (?1, 0, 0, 0, 0, 0, 0, 0, 0, 0)",
            [application],
        )?;
        self.connection.execute(
            &format!(
                "UPDATE workspace_detection SET dictations = dictations + 1, \
                 {kind} = {kind} + 1, {bucket} = {bucket} + 1 WHERE application = ?1"
            ),
            [application],
        )?;
        Ok(())
    }

    pub fn detection_coverage(&self) -> Result<Vec<DetectionCoverage>> {
        let mut statement = self.connection.prepare(
            "SELECT application, dictations, file, folder, web, none, under_1ms, under_5ms, \
             under_30ms, slower FROM workspace_detection ORDER BY dictations DESC, application",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(DetectionCoverage {
                application: row.get(0)?,
                dictations: row.get(1)?,
                file: row.get(2)?,
                folder: row.get(3)?,
                web: row.get(4)?,
                none: row.get(5)?,
                latency: [row.get(6)?, row.get(7)?, row.get(8)?, row.get(9)?],
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn clear_detection_coverage(&self) -> Result<()> {
        self.connection
            .execute("DELETE FROM workspace_detection", [])?;
        Ok(())
    }

    pub fn list_workspaces(&self) -> Result<Vec<WorkspaceSummary>> {
        Ok(summaries(&self.workspace_state()?))
    }

    fn mutate_workspaces(
        &self,
        change: impl FnOnce(
            &mut WorkspaceState,
        ) -> std::result::Result<Option<(String, String)>, String>,
    ) -> Result<WorkspaceUpdate> {
        let tx = self.connection.unchecked_transaction()?;
        let (_, mut state) = self
            .read_workspace_row()?
            .ok_or_else(|| DatasetError::InvalidConfig("No workspaces yet.".into()))?;
        let migration = change(&mut state).map_err(DatasetError::InvalidConfig)?;
        let profile_changed = match migration {
            Some((old, new)) => self.migrate_project_scope(&tx, &old, &new)?,
            None => false,
        };
        self.write_workspace_state(&state)?;
        tx.commit()?;
        Ok(WorkspaceUpdate {
            workspaces: summaries(&state),
            profile_changed,
        })
    }

    /// Move every vocabulary entry, example and learning candidate scoped to
    /// `old` to `new`. Learning events and History keep their original scope.
    fn migrate_project_scope(&self, tx: &Connection, old: &str, new: &str) -> Result<bool> {
        if same_project(old, new) && old.trim() == new.trim() {
            return Ok(false);
        }
        let mut profile: PersonalizationProfile = self.personalization()?;
        let mut changed = false;
        for word in &mut profile.vocabulary {
            if same_project(&word.project, old) {
                word.project = new.into();
                changed = true;
            }
        }
        for example in &mut profile.examples {
            if same_project(&example.project, old) {
                example.project = new.into();
                changed = true;
            }
        }
        let mut learning = self.automatic_learning_state()?;
        let mut learning_changed = false;
        for candidate in &mut learning.candidates {
            if same_project(&candidate.project, old) {
                candidate.project = new.into();
                learning_changed = true;
            }
        }
        if changed {
            put_profile(tx, &mut profile)?;
        }
        if learning_changed {
            put_state(tx, &learning)?;
        }
        Ok(changed)
    }

    pub fn rename_workspace(&self, key: &str, label: &str) -> Result<WorkspaceUpdate> {
        let label = label.trim().to_string();
        self.mutate_workspaces(|state| {
            let old = state.rename(key, &label)?;
            Ok(Some((old, label)))
        })
    }

    pub fn merge_workspace(&self, from: &str, into: &str) -> Result<WorkspaceUpdate> {
        self.mutate_workspaces(|state| state.merge(from, into).map(Some))
    }

    /// Delete (suppress). Entries scoped to its name stay, unused, until removed.
    pub fn delete_workspace(&self, key: &str) -> Result<WorkspaceUpdate> {
        self.mutate_workspaces(|state| state.delete(key).map(|_| None))
    }
}

impl DictationPersonalizationSnapshot {
    /// Resolve this operation's project and workspace context, in order: manual
    /// project → existing rule → visible workspace → app category → shared.
    /// Pure and in-memory (safe on the main thread). Returns the transient
    /// observation the caller persists off the main thread.
    pub fn resolve_context(
        &mut self,
        application: Option<&str>,
        document: DocumentSignal<'_>,
        home: Option<&str>,
        day: u32,
    ) -> Option<WorkspaceObservation> {
        let file = match document {
            DocumentSignal::File { path, .. } => Some(path),
            _ => None,
        };
        let rules = resolve_project(
            self.profile.project_mode,
            &self.profile.active_project,
            &self.profile.project_rules,
            application,
            file,
        );
        let automatic = self.profile.automatic_workspaces && !self.workspaces.salt.is_empty();
        let signal = if automatic {
            document
        } else {
            DocumentSignal::None
        };
        let observation = application
            .map(|app| WorkspaceObservation::new(&self.workspaces.salt, app, signal, home));
        let recorded = observation
            .as_ref()
            .filter(|o| automatic && o.has_document())
            .cloned();
        if let Some(o) = &recorded {
            self.workspaces.state.observe(o, day);
        }
        let resolution = resolve_workspace(
            rules,
            automatic,
            &self.workspaces.state,
            observation.as_ref(),
        );
        self.project_selection = resolution.selection;
        self.profile.active_project = self.project_selection.project.clone();
        self.workspace = resolution.context;
        recorded
    }
}

#[cfg(test)]
mod tests;
