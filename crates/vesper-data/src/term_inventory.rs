//! Term inventory: spellings mined from user-asserted and user-authored sources,
//! separate from the 128-entry profile vocabulary. Rows hold spellings, spoken
//! forms, per-source/per-scope counts and status; never paths or document text.
//! A user removal is a tombstone that mining never overwrites.
use std::collections::{BTreeMap, BTreeSet, HashMap};

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use vesper_core::{term_kind, InventoryTerm, Language, TermKind, TermSource, TermTier};

use crate::term_mining::{MinedTerm, WorkspaceCursor};
use crate::{DatasetError, DatasetStore, Result};

/// Inventory bound; the least useful unconfirmed rows are evicted beyond it.
pub const MAX_INVENTORY_TERMS: usize = 20_000;
const MAX_RUNS: i64 = 200;
const MAX_SPOKEN: usize = 4;

pub(crate) const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS term_inventory (
    id INTEGER PRIMARY KEY,
    surface TEXT NOT NULL UNIQUE,
    key TEXT NOT NULL,
    kind TEXT NOT NULL,
    tier TEXT NOT NULL CHECK(tier IN ('asserted', 'authored')),
    status TEXT NOT NULL CHECK(status IN ('unconfirmed', 'active', 'suspended', 'removed')),
    spoken_json TEXT NOT NULL,
    evidence_json TEXT NOT NULL,
    doc_freq INTEGER NOT NULL,
    first_seen_ms INTEGER NOT NULL,
    last_seen_ms INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS term_inventory_key ON term_inventory(key);
CREATE INDEX IF NOT EXISTS term_inventory_status ON term_inventory(status);
CREATE TABLE IF NOT EXISTS term_mining_state (key TEXT PRIMARY KEY NOT NULL, value_json TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS term_mining_runs (id INTEGER PRIMARY KEY, started_ms INTEGER NOT NULL, run_json TEXT NOT NULL);
";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TermStatus {
    /// Tier B: usable for hints and references; not user-confirmed.
    Unconfirmed,
    /// Tier A or user-confirmed.
    Active,
    /// Turned off by negative evidence; mining updates counts but not status.
    Suspended,
    /// User removal; a tombstone mining never re-adds.
    Removed,
}

impl TermStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Unconfirmed => "unconfirmed",
            Self::Active => "active",
            Self::Suspended => "suspended",
            Self::Removed => "removed",
        }
    }
    fn parse(value: &str) -> Self {
        match value {
            "active" => Self::Active,
            "suspended" => Self::Suspended,
            "removed" => Self::Removed,
            _ => Self::Unconfirmed,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TermEvidence {
    pub source: TermSource,
    pub scope: String,
    pub count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InventoryRow {
    pub id: i64,
    pub surface: String,
    pub spoken: Vec<String>,
    pub kind: TermKind,
    pub tier: TermTier,
    pub status: TermStatus,
    pub evidence: Vec<TermEvidence>,
    pub doc_freq: u32,
    pub first_seen_ms: i64,
    pub last_seen_ms: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeReport {
    pub added: usize,
    pub updated: usize,
    pub dropped: usize,
    pub tombstoned: usize,
    pub evicted: usize,
}

/// Counts and timings only: no spellings, paths or text.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TermMiningRun {
    pub started_ms: i64,
    pub wall_ms: u64,
    pub cpu_ms: u64,
    pub outcome: String,
    pub files_read: usize,
    pub bytes_read: u64,
    pub files_skipped: usize,
    pub workspaces_scanned: usize,
    pub workspaces_unchanged: usize,
    pub terms_added: usize,
    pub terms_updated: usize,
    pub terms_dropped: usize,
    pub terms_total: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceSummary {
    pub source: TermSource,
    pub tier: TermTier,
    pub terms: usize,
    pub paused: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TermWorkspaceSummary {
    pub scope: String,
    pub label: String,
    pub terms: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InventoryRowView {
    pub id: i64,
    pub surface: String,
    pub kind: TermKind,
    pub tier: TermTier,
    pub status: TermStatus,
    pub sources: Vec<TermSource>,
    /// Workspace labels (folder names); empty when only global.
    pub workspaces: Vec<String>,
    pub count: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TermInventorySummary {
    pub total: usize,
    pub usable: usize,
    pub removed: usize,
    pub suspended: usize,
    pub sources: Vec<SourceSummary>,
    pub workspaces: Vec<TermWorkspaceSummary>,
    pub rows: Vec<InventoryRowView>,
    pub last_run: Option<TermMiningRun>,
}

fn json<T: Serialize>(value: &T) -> Result<String> {
    Ok(serde_json::to_string(value)?)
}

fn enum_str<T: Serialize>(value: T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

fn enum_parse<T: for<'de> Deserialize<'de>>(value: &str) -> Option<T> {
    serde_json::from_value(serde_json::Value::String(value.to_owned())).ok()
}

fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<InventoryRow> {
    let text = |i| r.get::<_, String>(i);
    Ok(InventoryRow {
        id: r.get(0)?,
        surface: text(1)?,
        kind: enum_parse(&text(2)?).unwrap_or(TermKind::Name),
        tier: enum_parse(&text(3)?).unwrap_or(TermTier::Authored),
        status: TermStatus::parse(&text(4)?),
        spoken: serde_json::from_str(&text(5)?).unwrap_or_default(),
        evidence: serde_json::from_str(&text(6)?).unwrap_or_default(),
        doc_freq: r.get(7)?,
        first_seen_ms: r.get(8)?,
        last_seen_ms: r.get(9)?,
    })
}

const COLUMNS: &str = "id, surface, kind, tier, status, spoken_json, evidence_json, doc_freq, first_seen_ms, last_seen_ms";

fn best_tier(evidence: &[TermEvidence]) -> TermTier {
    evidence
        .iter()
        .map(|e| e.source.tier())
        .min()
        .unwrap_or(TermTier::Authored)
}

fn invalid(message: &str) -> DatasetError {
    DatasetError::InvalidConfig(message.into())
}

impl DatasetStore {
    fn inventory_rows(&self, filter: &str) -> Result<Vec<InventoryRow>> {
        let mut statement = self
            .connection
            .prepare(&format!("SELECT {COLUMNS} FROM term_inventory {filter}"))?;
        let rows = statement.query_map([], row)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    fn write_inventory_row(&self, r: &InventoryRow) -> Result<()> {
        self.connection.execute(
            "UPDATE term_inventory SET tier = ?2, status = ?3, spoken_json = ?4, evidence_json = ?5,
             doc_freq = ?6, last_seen_ms = ?7 WHERE id = ?1",
            params![
                r.id,
                enum_str(r.tier),
                r.status.as_str(),
                json(&r.spoken)?,
                json(&r.evidence)?,
                r.doc_freq,
                r.last_seen_ms
            ],
        )?;
        Ok(())
    }

    /// Replace one source's evidence in one scope with a fresh mining result.
    /// Removed rows are never revived; suspended rows keep their status.
    pub fn replace_mined_terms(
        &self,
        source: TermSource,
        scope: &str,
        terms: &[MinedTerm],
        now_ms: i64,
    ) -> Result<MergeReport> {
        self.replace_mined_sources(&[source], scope, terms, now_ms)
    }

    /// As `replace_mined_terms` for every source in `sources` at once.
    pub fn replace_mined_sources(
        &self,
        sources: &[TermSource],
        scope: &str,
        terms: &[MinedTerm],
        now_ms: i64,
    ) -> Result<MergeReport> {
        if terms
            .iter()
            .any(|t| !sources.contains(&t.source) || t.scope != scope)
        {
            return Err(invalid("Mined terms do not match the replaced source."));
        }
        let paused = self.paused_term_sources()?;
        let terms = terms
            .iter()
            .filter(|t| !paused.contains(&t.source))
            .collect::<Vec<_>>();
        let tx = self.connection.unchecked_transaction()?;
        let mut report = MergeReport::default();
        let mut rows = self
            .inventory_rows("")?
            .into_iter()
            .map(|r| (r.surface.clone(), r))
            .collect::<HashMap<_, _>>();
        let removed_keys = rows
            .values()
            .filter(|r| r.status == TermStatus::Removed)
            .map(|r| r.surface.to_lowercase())
            .collect::<BTreeSet<_>>();
        let mut previous = HashMap::new();
        let mut touched = BTreeSet::new();
        for r in rows.values_mut() {
            let before = r.evidence.len();
            r.evidence.retain(|e| {
                let replaced = sources.contains(&e.source) && e.scope == scope;
                if replaced {
                    previous.insert((r.surface.clone(), e.source), e.count);
                }
                !replaced
            });
            if r.evidence.len() != before {
                touched.insert(r.surface.clone());
            }
        }
        for term in terms {
            if removed_keys.contains(&term.surface.to_lowercase()) {
                report.tombstoned += 1;
                continue;
            }
            let evidence = TermEvidence {
                source: term.source,
                scope: scope.to_owned(),
                count: term.count.max(1),
            };
            let grew = previous
                .get(&(term.surface.clone(), term.source))
                .is_none_or(|count| *count < evidence.count);
            if let Some(existing) = rows.get_mut(&term.surface) {
                existing.evidence.push(evidence);
                for spoken in &term.spoken {
                    if existing.spoken.len() < MAX_SPOKEN && !existing.spoken.contains(spoken) {
                        existing.spoken.push(spoken.clone());
                    }
                }
                if grew {
                    existing.last_seen_ms = now_ms;
                }
                touched.insert(term.surface.clone());
            } else {
                let tier = term.source.tier();
                let status = match tier {
                    TermTier::Asserted => TermStatus::Active,
                    TermTier::Authored => TermStatus::Unconfirmed,
                };
                self.connection.execute(
                    "INSERT INTO term_inventory (surface, key, kind, tier, status, spoken_json,
                     evidence_json, doc_freq, first_seen_ms, last_seen_ms)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)",
                    params![
                        term.surface,
                        term.surface.to_lowercase(),
                        enum_str(term_kind(&term.surface)),
                        enum_str(tier),
                        status.as_str(),
                        json(&term.spoken.iter().take(MAX_SPOKEN).collect::<Vec<_>>())?,
                        json(&[&evidence])?,
                        evidence.count,
                        now_ms
                    ],
                )?;
                report.added += 1;
            }
        }
        for surface in touched {
            let r = &rows[&surface];
            if r.status == TermStatus::Removed {
                continue;
            }
            if r.evidence.is_empty() && r.status != TermStatus::Suspended {
                self.connection
                    .execute("DELETE FROM term_inventory WHERE id = ?1", [r.id])?;
                report.dropped += 1;
                continue;
            }
            let mut r = r.clone();
            r.tier = best_tier(&r.evidence);
            r.doc_freq = r.evidence.iter().map(|e| e.count).sum();
            if r.status == TermStatus::Unconfirmed && r.tier == TermTier::Asserted {
                r.status = TermStatus::Active;
            } else if r.status == TermStatus::Active
                && r.tier == TermTier::Authored
                && !r.evidence.is_empty()
            {
                // Asserted evidence went away: fall back to unconfirmed.
                r.status = TermStatus::Unconfirmed;
            }
            self.write_inventory_row(&r)?;
            report.updated += 1;
        }
        report.evicted = self.evict_inventory()?;
        tx.commit()?;
        Ok(report)
    }

    /// Keep the inventory bounded: drop unconfirmed tier-B rows with the least
    /// evidence and the oldest last sighting first. Tombstones are kept.
    fn evict_inventory(&self) -> Result<usize> {
        let total: i64 =
            self.connection
                .query_row("SELECT COUNT(*) FROM term_inventory", [], |r| r.get(0))?;
        let excess = total - MAX_INVENTORY_TERMS as i64;
        if excess <= 0 {
            return Ok(0);
        }
        Ok(self.connection.execute(
            "DELETE FROM term_inventory WHERE id IN (
                SELECT id FROM term_inventory WHERE status = 'unconfirmed'
                ORDER BY tier = 'authored' DESC, doc_freq ASC, last_seen_ms ASC, id DESC LIMIT ?1)",
            [excess],
        )?)
    }

    /// Usable rows (unconfirmed and active) as selector input.
    pub fn inventory_terms(&self) -> Result<Vec<InventoryTerm>> {
        Ok(self
            .inventory_rows("WHERE status IN ('unconfirmed', 'active')")?
            .into_iter()
            .map(|r| {
                let mut scopes = r
                    .evidence
                    .iter()
                    .map(|e| e.scope.clone())
                    .collect::<Vec<_>>();
                scopes.sort();
                scopes.dedup();
                let mut sources = r.evidence.iter().map(|e| e.source).collect::<Vec<_>>();
                sources.sort();
                sources.dedup();
                InventoryTerm {
                    id: format!("inv:{}", r.id),
                    surface: r.surface,
                    spoken: r.spoken,
                    kind: r.kind,
                    tier: r.tier,
                    sources,
                    scopes,
                    language: Language::Auto,
                    evidence: r.doc_freq,
                    last_seen_ms: r.last_seen_ms,
                }
            })
            .collect())
    }

    fn set_inventory_status(&self, id: i64, status: TermStatus) -> Result<bool> {
        let changed = if status == TermStatus::Removed {
            self.connection.execute(
                "UPDATE term_inventory SET status = 'removed', evidence_json = '[]', spoken_json = '[]',
                 doc_freq = 0 WHERE id = ?1",
                [id],
            )?
        } else {
            self.connection.execute(
                "UPDATE term_inventory SET status = ?2 WHERE id = ?1 AND status <> 'removed'",
                params![id, status.as_str()],
            )?
        };
        Ok(changed == 1)
    }

    /// User removal: a tombstone mining never re-adds (any letter case).
    pub fn remove_inventory_term(&self, id: i64) -> Result<bool> {
        self.set_inventory_status(id, TermStatus::Removed)
    }

    /// Negative evidence (for example a revert) turns a term off.
    pub fn suspend_inventory_term(&self, id: i64) -> Result<bool> {
        self.set_inventory_status(id, TermStatus::Suspended)
    }

    /// Forget every term from one source and pause that source until resumed.
    pub fn forget_term_source(&self, source: TermSource) -> Result<usize> {
        let tx = self.connection.unchecked_transaction()?;
        let mut paused = self.paused_term_sources()?;
        paused.insert(source);
        self.put_mining_state("paused_sources", &paused)?;
        let mut forgotten = 0;
        for mut r in self.inventory_rows("WHERE status <> 'removed'")? {
            let before = r.evidence.len();
            r.evidence.retain(|e| e.source != source);
            if r.evidence.len() == before {
                continue;
            }
            forgotten += 1;
            if r.evidence.is_empty() {
                self.connection
                    .execute("DELETE FROM term_inventory WHERE id = ?1", [r.id])?;
            } else {
                r.tier = best_tier(&r.evidence);
                r.doc_freq = r.evidence.iter().map(|e| e.count).sum();
                if r.status == TermStatus::Active && r.tier == TermTier::Authored {
                    r.status = TermStatus::Unconfirmed;
                }
                self.write_inventory_row(&r)?;
            }
        }
        if source.tier() == TermTier::Authored {
            // A later resume rescans from scratch.
            self.connection.execute(
                "DELETE FROM term_mining_state WHERE key LIKE 'cursor:%'",
                [],
            )?;
        }
        tx.commit()?;
        Ok(forgotten)
    }

    pub fn resume_term_source(&self, source: TermSource) -> Result<()> {
        let mut paused = self.paused_term_sources()?;
        paused.remove(&source);
        self.put_mining_state("paused_sources", &paused)
    }

    pub fn paused_term_sources(&self) -> Result<BTreeSet<TermSource>> {
        Ok(self.mining_state("paused_sources")?.unwrap_or_default())
    }

    fn mining_state<T: for<'de> Deserialize<'de>>(&self, key: &str) -> Result<Option<T>> {
        let value: Option<String> = self
            .connection
            .query_row(
                "SELECT value_json FROM term_mining_state WHERE key = ?1",
                [key],
                |r| r.get(0),
            )
            .optional()?;
        Ok(value.map(|v| serde_json::from_str(&v)).transpose()?)
    }

    fn put_mining_state<T: Serialize>(&self, key: &str, value: &T) -> Result<()> {
        self.connection.execute(
            "INSERT INTO term_mining_state (key, value_json) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json",
            params![key, json(value)?],
        )?;
        Ok(())
    }

    /// Workspace scopes that have a scan cursor (mined at least once).
    pub fn mined_workspace_scopes(&self) -> Result<Vec<String>> {
        let mut statement = self
            .connection
            .prepare("SELECT key FROM term_mining_state WHERE key LIKE 'cursor:%' ORDER BY key")?;
        let rows = statement.query_map([], |r| r.get::<_, String>(0))?;
        rows.map(|r| Ok(r?.trim_start_matches("cursor:").to_owned()))
            .collect()
    }

    /// Drop every workspace-scoped source's evidence for `scope` and its cursor,
    /// for a workspace whose mining root was cleared or that was removed.
    pub fn forget_term_workspace(&self, scope: &str, now_ms: i64) -> Result<MergeReport> {
        let report = self.replace_mined_sources(
            &[
                TermSource::Identifier,
                TermSource::Manifest,
                TermSource::Documentation,
                TermSource::ProjectDictionary,
            ],
            scope,
            &[],
            now_ms,
        )?;
        self.connection.execute(
            "DELETE FROM term_mining_state WHERE key = ?1",
            [format!("cursor:{scope}")],
        )?;
        Ok(report)
    }

    pub fn workspace_mining_cursor(&self, scope: &str) -> Result<WorkspaceCursor> {
        Ok(self
            .mining_state(&format!("cursor:{scope}"))?
            .unwrap_or_default())
    }

    pub fn save_workspace_mining_cursor(
        &self,
        scope: &str,
        cursor: &WorkspaceCursor,
    ) -> Result<()> {
        self.put_mining_state(&format!("cursor:{scope}"), cursor)
    }

    /// Display label (a folder name) for a workspace scope.
    pub fn set_term_workspace_label(&self, scope: &str, label: &str) -> Result<()> {
        let mut labels: BTreeMap<String, String> =
            self.mining_state("workspace_labels")?.unwrap_or_default();
        if labels.get(scope).map(String::as_str) != Some(label) {
            labels.insert(scope.to_owned(), label.to_owned());
            self.put_mining_state("workspace_labels", &labels)?;
        }
        Ok(())
    }

    pub fn record_term_mining_run(&self, run: &TermMiningRun) -> Result<()> {
        self.connection.execute(
            "INSERT INTO term_mining_runs (started_ms, run_json) VALUES (?1, ?2)",
            params![run.started_ms, json(run)?],
        )?;
        self.connection.execute(
            "DELETE FROM term_mining_runs WHERE id NOT IN
             (SELECT id FROM term_mining_runs ORDER BY id DESC LIMIT ?1)",
            [MAX_RUNS],
        )?;
        Ok(())
    }

    pub fn term_mining_runs(&self, limit: usize) -> Result<Vec<TermMiningRun>> {
        let mut statement = self
            .connection
            .prepare("SELECT run_json FROM term_mining_runs ORDER BY id DESC LIMIT ?1")?;
        let rows = statement.query_map([limit as i64], |r| r.get::<_, String>(0))?;
        rows.map(|r| Ok(serde_json::from_str(&r?)?)).collect()
    }

    /// Counts per source and workspace plus the strongest rows for review.
    pub fn term_inventory_summary(&self, row_limit: usize) -> Result<TermInventorySummary> {
        let labels: BTreeMap<String, String> =
            self.mining_state("workspace_labels")?.unwrap_or_default();
        let paused = self.paused_term_sources()?;
        let rows = self.inventory_rows(
            "ORDER BY status = 'removed', tier = 'authored', doc_freq DESC, surface",
        )?;
        let mut summary = TermInventorySummary {
            total: rows.len(),
            last_run: self.term_mining_runs(1)?.pop(),
            ..TermInventorySummary::default()
        };
        let mut by_source: BTreeMap<TermSource, usize> = BTreeMap::new();
        let mut by_scope: BTreeMap<String, usize> = BTreeMap::new();
        for r in &rows {
            match r.status {
                TermStatus::Removed => summary.removed += 1,
                TermStatus::Suspended => summary.suspended += 1,
                _ => summary.usable += 1,
            }
            let sources = r.evidence.iter().map(|e| e.source).collect::<BTreeSet<_>>();
            for source in &sources {
                *by_source.entry(*source).or_default() += 1;
            }
            let scopes = r
                .evidence
                .iter()
                .filter(|e| !e.scope.is_empty())
                .map(|e| e.scope.clone())
                .collect::<BTreeSet<_>>();
            for scope in &scopes {
                *by_scope.entry(scope.clone()).or_default() += 1;
            }
            if summary.rows.len() < row_limit && r.status != TermStatus::Removed {
                summary.rows.push(InventoryRowView {
                    id: r.id,
                    surface: r.surface.clone(),
                    kind: r.kind,
                    tier: r.tier,
                    status: r.status,
                    sources: sources.into_iter().collect(),
                    workspaces: scopes
                        .iter()
                        .map(|s| labels.get(s).cloned().unwrap_or_else(|| "Workspace".into()))
                        .collect(),
                    count: r.doc_freq,
                });
            }
        }
        for source in paused.iter() {
            by_source.entry(*source).or_default();
        }
        summary.sources = by_source
            .into_iter()
            .map(|(source, terms)| SourceSummary {
                source,
                tier: source.tier(),
                terms,
                paused: paused.contains(&source),
            })
            .collect();
        summary.workspaces = by_scope
            .into_iter()
            .map(|(scope, terms)| TermWorkspaceSummary {
                label: labels
                    .get(&scope)
                    .cloned()
                    .unwrap_or_else(|| "Workspace".into()),
                scope,
                terms,
            })
            .collect();
        Ok(summary)
    }
}

impl crate::DictationPersonalizationSnapshot {
    fn selection_terms(&self) -> Vec<InventoryTerm> {
        let mut terms = vesper_core::profile_terms(&self.profile);
        terms.extend(self.inventory.iter().cloned());
        terms
    }

    fn selection_context(&self, language: Language) -> vesper_core::SelectionContext {
        vesper_core::SelectionContext {
            workspace: self.workspace.clone(),
            project: self.profile.active_project.clone(),
            language,
            now_ms: chrono::Utc::now().timestamp_millis(),
        }
    }

    /// Pre-ASR selection; call after the workspace and project are resolved.
    /// Fills `working_set` with hints and references (no normalizations).
    pub fn select_working_set(&mut self, language: Language) {
        self.working_set = vesper_core::select_working_set(
            &self.selection_terms(),
            &self.selection_context(language),
            None,
        );
        // Every decode path reads hints from here; only a qualified hook sends them.
        self.profile.decode_context = Some(vesper_core::DecodeContext {
            working_set: self.working_set.clone(),
            ..Default::default()
        });
    }

    /// Post-ASR selection over the raw text: references prefer heard terms and
    /// normalizations list spoken forms that occur in `raw`, for the correction
    /// gate. Hints and `hint_hash` are copied from the pre-ASR `working_set`, so
    /// they describe what the decode actually received (empty if never selected).
    pub fn post_asr_working_set(&self, raw: &str, language: Language) -> vesper_core::WorkingSet {
        let mut set = vesper_core::select_working_set(
            &self.selection_terms(),
            &self.selection_context(language),
            Some(raw),
        );
        set.hints = self.working_set.hints.clone();
        set.hint_hash = self.working_set.hint_hash.clone();
        // Opt-in field words: spellings the user already wrote in this field.
        for term in &self.field_terms {
            if !set.references.contains(term) {
                set.references.push(term.clone());
            }
        }
        set
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, DatasetStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = DatasetStore::open(dir.path().into()).unwrap();
        (dir, store)
    }

    fn mined(surface: &str, source: TermSource, scope: &str, count: u32) -> MinedTerm {
        MinedTerm::new(surface, source, scope, count)
    }

    #[test]
    fn field_words_become_references_that_repair_a_near_miss() {
        let (_dir, store) = store();
        let mut snapshot = store.dictation_personalization_snapshot().unwrap();
        snapshot.field_terms =
            vesper_core::field_terms("We moved Kubernetes to the new Vesper runner.");
        let raw = "the cubernetes job runs on vasper";
        let set = snapshot.post_asr_working_set(raw, Language::English);
        assert!(set.references.iter().any(|r| r == "Kubernetes"));
        let gate = vesper_core::correction_gate_with(
            raw,
            &set,
            Language::English,
            vesper_core::NearMiss::Apply,
        );
        assert_eq!(gate.text, "the Kubernetes job runs on Vesper");
        // Without field words nothing is known, so nothing changes.
        snapshot.field_terms.clear();
        let set = snapshot.post_asr_working_set(raw, Language::English);
        let gate = vesper_core::correction_gate_with(
            raw,
            &set,
            Language::English,
            vesper_core::NearMiss::Apply,
        );
        assert_eq!(gate.text, raw);
    }

    #[test]
    fn tier_a_is_active_tier_b_unconfirmed_and_replacement_is_per_source_and_scope() {
        let (_dir, store) = store();
        store
            .replace_mined_terms(
                TermSource::TextReplacement,
                "",
                &[mined("Zorblax", TermSource::TextReplacement, "", 1)],
                10,
            )
            .unwrap();
        let report = store
            .replace_mined_sources(
                &[TermSource::Identifier, TermSource::Manifest],
                "ws-1",
                &[
                    mined("fooBar", TermSource::Identifier, "ws-1", 3),
                    mined("Zorblax", TermSource::Identifier, "ws-1", 2),
                ],
                20,
            )
            .unwrap();
        assert_eq!((report.added, report.updated), (1, 1));
        let terms = store.inventory_terms().unwrap();
        let zorblax = terms.iter().find(|t| t.surface == "Zorblax").unwrap();
        assert_eq!(zorblax.tier, TermTier::Asserted);
        assert_eq!(zorblax.scopes, ["", "ws-1"]);
        assert_eq!(zorblax.evidence, 3);
        let foo = terms.iter().find(|t| t.surface == "fooBar").unwrap();
        assert_eq!(foo.tier, TermTier::Authored);
        assert_eq!(foo.spoken, ["foo bar"]);
        // A second workspace does not touch the first one's evidence.
        store
            .replace_mined_sources(
                &[TermSource::Identifier],
                "ws-2",
                &[mined("fooBar", TermSource::Identifier, "ws-2", 2)],
                30,
            )
            .unwrap();
        // Rescanning ws-1 without fooBar drops only ws-1 evidence.
        let report = store
            .replace_mined_sources(
                &[TermSource::Identifier, TermSource::Manifest],
                "ws-1",
                &[],
                40,
            )
            .unwrap();
        assert_eq!(report.dropped, 0);
        let terms = store.inventory_terms().unwrap();
        assert_eq!(
            terms.iter().find(|t| t.surface == "fooBar").unwrap().scopes,
            ["ws-2"]
        );
        assert_eq!(
            terms
                .iter()
                .find(|t| t.surface == "Zorblax")
                .unwrap()
                .scopes,
            [""]
        );
        let rows = store.inventory_rows("").unwrap();
        assert!(rows.iter().all(|r| r.status != TermStatus::Removed));
        assert!(rows.iter().find(|r| r.surface == "Zorblax").unwrap().status == TermStatus::Active);
    }

    #[test]
    fn removal_is_a_tombstone_that_mining_never_revives_in_any_case() {
        let (_dir, store) = store();
        store
            .replace_mined_sources(
                &[TermSource::Identifier],
                "ws",
                &[mined("fooBar", TermSource::Identifier, "ws", 3)],
                1,
            )
            .unwrap();
        let id = store.inventory_rows("").unwrap()[0].id;
        assert!(store.remove_inventory_term(id).unwrap());
        assert!(store.inventory_terms().unwrap().is_empty());
        let report = store
            .replace_mined_sources(
                &[TermSource::Identifier],
                "ws",
                &[
                    mined("fooBar", TermSource::Identifier, "ws", 9),
                    mined("FooBar", TermSource::Identifier, "ws", 9),
                ],
                2,
            )
            .unwrap();
        assert_eq!(report.tombstoned, 2);
        assert!(store.inventory_terms().unwrap().is_empty());
        assert!(!store.suspend_inventory_term(id).unwrap());
        let summary = store.term_inventory_summary(10).unwrap();
        assert_eq!((summary.total, summary.removed, summary.usable), (1, 1, 0));
        assert!(summary.rows.is_empty());
    }

    #[test]
    fn suspended_terms_are_not_selected_and_keep_status_through_rescans() {
        let (_dir, store) = store();
        store
            .replace_mined_sources(
                &[TermSource::Identifier],
                "ws",
                &[mined("fooBar", TermSource::Identifier, "ws", 3)],
                1,
            )
            .unwrap();
        let id = store.inventory_rows("").unwrap()[0].id;
        assert!(store.suspend_inventory_term(id).unwrap());
        store
            .replace_mined_sources(
                &[TermSource::Identifier],
                "ws",
                &[mined("fooBar", TermSource::Identifier, "ws", 5)],
                2,
            )
            .unwrap();
        assert!(store.inventory_terms().unwrap().is_empty());
        store
            .replace_mined_sources(&[TermSource::Identifier], "ws", &[], 3)
            .unwrap();
        assert_eq!(
            store.inventory_rows("").unwrap()[0].status,
            TermStatus::Suspended
        );
    }

    #[test]
    fn forgetting_a_source_pauses_it_until_resumed() {
        let (_dir, store) = store();
        let term = [mined("Zorblax", TermSource::TextReplacement, "", 1)];
        store
            .replace_mined_terms(TermSource::TextReplacement, "", &term, 1)
            .unwrap();
        store.set_term_workspace_label("ws", "demo").unwrap();
        store
            .replace_mined_sources(
                &[TermSource::Identifier],
                "ws",
                &[mined("Zorblax", TermSource::Identifier, "ws", 2)],
                1,
            )
            .unwrap();
        assert_eq!(
            store
                .forget_term_source(TermSource::TextReplacement)
                .unwrap(),
            1
        );
        let terms = store.inventory_terms().unwrap();
        assert_eq!(terms[0].tier, TermTier::Authored);
        store
            .replace_mined_terms(TermSource::TextReplacement, "", &term, 2)
            .unwrap();
        assert_eq!(store.inventory_terms().unwrap()[0].tier, TermTier::Authored);
        let summary = store.term_inventory_summary(10).unwrap();
        let source = summary
            .sources
            .iter()
            .find(|s| s.source == TermSource::TextReplacement)
            .unwrap();
        assert!(source.paused && source.terms == 0);
        assert_eq!(summary.workspaces[0].label, "demo");
        assert_eq!(summary.rows[0].workspaces, ["demo"]);
        store
            .resume_term_source(TermSource::TextReplacement)
            .unwrap();
        store
            .replace_mined_terms(TermSource::TextReplacement, "", &term, 3)
            .unwrap();
        assert_eq!(store.inventory_terms().unwrap()[0].tier, TermTier::Asserted);
    }

    #[test]
    fn inventory_is_bounded_by_evicting_weak_unconfirmed_rows() {
        let (_dir, store) = store();
        let terms = (0..MAX_INVENTORY_TERMS + 5)
            .map(|i| {
                mined(
                    &format!("termNo{i}"),
                    TermSource::Identifier,
                    "ws",
                    if i < 5 { 2 } else { 3 },
                )
            })
            .collect::<Vec<_>>();
        let report = store
            .replace_mined_sources(&[TermSource::Identifier], "ws", &terms, 1)
            .unwrap();
        assert_eq!(report.evicted, 5);
        let left = store.inventory_terms().unwrap();
        assert_eq!(left.len(), MAX_INVENTORY_TERMS);
        assert!(left.iter().all(|t| t.evidence == 3));
    }

    #[test]
    fn snapshot_selects_after_workspace_resolution_and_post_asr_keeps_the_hints() {
        let (_dir, store) = store();
        store
            .replace_mined_sources(
                &[TermSource::Identifier],
                "ws-key",
                &[mined("sherpa_onnx", TermSource::Identifier, "ws-key", 4)],
                1,
            )
            .unwrap();
        let mut profile = store.dictation_personalization().unwrap();
        profile.vocabulary.push(vesper_core::VocabularyEntry {
            id: "w".into(),
            phrase: "Zorblax".into(),
            enabled: true,
            ..Default::default()
        });
        store.save_personalization(profile).unwrap();
        let mut snapshot = store.dictation_personalization_snapshot().unwrap();
        assert_eq!(snapshot.inventory.len(), 1);
        assert!(snapshot.working_set.hints.is_empty());
        let raw = "load sherpa onnx for Zorblax";
        // Never selected: post-ASR reports no hints, still offers normalizations.
        let unselected = snapshot.post_asr_working_set(raw, Language::English);
        assert!(unselected.hints.is_empty() && unselected.hint_hash.is_empty());
        snapshot.select_working_set(Language::English);
        assert_eq!(snapshot.working_set.hints, ["Zorblax"]);
        snapshot.workspace.workspace_key = "ws-key".into();
        snapshot.select_working_set(Language::English);
        assert_eq!(snapshot.working_set.hints, ["sherpa_onnx", "Zorblax"]);
        let post = snapshot.post_asr_working_set(raw, Language::English);
        assert_eq!(post.hint_hash, snapshot.working_set.hint_hash);
        assert_eq!(post.normalizations.len(), 1);
        assert_eq!(post.normalizations[0].spelling, "sherpa_onnx");
    }

    #[test]
    fn forgetting_a_workspace_drops_only_its_scoped_evidence_and_cursor() {
        let (_dir, store) = store();
        store
            .replace_mined_terms(
                TermSource::TextReplacement,
                "",
                &[mined("Zorblax", TermSource::TextReplacement, "", 1)],
                1,
            )
            .unwrap();
        for scope in ["ws-1", "ws-2"] {
            store
                .replace_mined_sources(
                    &[TermSource::Identifier],
                    scope,
                    &[
                        mined("Zorblax", TermSource::Identifier, scope, 2),
                        mined(
                            &format!("only_{}", scope.replace('-', "")),
                            TermSource::Identifier,
                            scope,
                            2,
                        ),
                    ],
                    1,
                )
                .unwrap();
            store
                .save_workspace_mining_cursor(scope, &WorkspaceCursor::default())
                .unwrap();
        }
        assert_eq!(store.mined_workspace_scopes().unwrap(), ["ws-1", "ws-2"]);
        let report = store.forget_term_workspace("ws-1", 2).unwrap();
        assert_eq!(report.dropped, 1);
        assert_eq!(store.mined_workspace_scopes().unwrap(), ["ws-2"]);
        let terms = store.inventory_terms().unwrap();
        assert_eq!(
            terms
                .iter()
                .find(|t| t.surface == "Zorblax")
                .unwrap()
                .scopes,
            ["", "ws-2"]
        );
        assert!(terms.iter().all(|t| t.surface != "only_ws1"));
    }

    #[test]
    fn cursors_and_run_logs_round_trip_without_text() {
        let (_dir, store) = store();
        let cursor = WorkspaceCursor {
            next_index: 7,
            ..Default::default()
        };
        store.save_workspace_mining_cursor("ws", &cursor).unwrap();
        assert_eq!(store.workspace_mining_cursor("ws").unwrap().next_index, 7);
        for i in 0..(MAX_RUNS + 3) {
            store
                .record_term_mining_run(&TermMiningRun {
                    started_ms: i,
                    outcome: "completed".into(),
                    ..Default::default()
                })
                .unwrap();
        }
        let runs = store.term_mining_runs(500).unwrap();
        assert_eq!(runs.len(), MAX_RUNS as usize);
        assert_eq!(runs[0].started_ms, MAX_RUNS + 2);
    }
}
