//! Evaluation-only local text evidence. Never read by memory admission or export.
use crate::{stored, DatasetError, DatasetStore, Result};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use vesper_core::{SemanticAnalysisReport, SemanticAnalysisRequest, SemanticStatus};

const RETENTION_SECONDS: i64 = 30 * 24 * 60 * 60;
const MAX_PROPOSALS: usize = 256;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticShadowRecord {
    pub created_at: i64,
    pub request: SemanticAnalysisRequest,
    pub report: SemanticAnalysisReport,
}

impl DatasetStore {
    pub(crate) fn prune_semantic_shadow(&self) -> Result<()> {
        self.connection.execute(
            "DELETE FROM semantic_shadow WHERE created_at <= ?1",
            [chrono::Utc::now().timestamp() - RETENTION_SECONDS],
        )?;
        self.connection.execute("DELETE FROM semantic_shadow WHERE result_id IN (SELECT result_id FROM semantic_shadow ORDER BY created_at DESC, rowid DESC LIMIT -1 OFFSET ?1)", [MAX_PROPOSALS as i64])?;
        Ok(())
    }

    /// Reserve a specific, already committed edit. Completion only updates this
    /// exact reservation; a deleted/undone/forgotten request cannot be resurrected.
    pub fn begin_semantic_shadow(&self, request: &SemanticAnalysisRequest) -> Result<bool> {
        request.validate().map_err(invalid)?;
        let tx = self.connection.unchecked_transaction()?;
        let profile = self.personalization()?;
        if request.origin != vesper_core::SemanticOrigin::InAppDraftEdit
            || !profile.automatic_learning
            || profile.learning_epoch != request.context.learning_epoch
            || !self.learning_operation_current(&request.context)?
            || request.memories.iter().any(|m| {
                !profile.vocabulary.iter().any(|v| {
                    v.enabled
                        && v.id == m.id
                        && v.phrase == m.phrase
                        && vesper_core::project_matches(&v.project, &request.context.project)
                        && vesper_core::language_matches(v.language, request.context.language)
                })
            })
        {
            return Ok(false);
        }
        if let Some(id) = &request.session_id {
            let record = crate::read_record(&tx, id)?;
            if record.personalization_rejected
                || record.draft_revision != request.result_revision
                || record.draft_transcript.as_deref() != Some(request.after.as_str())
            {
                return Ok(false);
            }
        }
        let serialized = serde_json::to_string(request)?;
        let old: Option<String> = tx
            .query_row(
                "SELECT request_json FROM semantic_shadow WHERE result_id = ?1",
                [&request.context.result_id],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(old) = old {
            let prior: SemanticAnalysisRequest = stored::decode(&old)?;
            if prior.result_revision >= request.result_revision {
                return Ok(false);
            }
        }
        tx.execute("INSERT INTO semantic_shadow (result_id, created_at, request_json, report_json) VALUES (?1, ?2, ?3, ?4) ON CONFLICT(result_id) DO UPDATE SET created_at=excluded.created_at, request_json=excluded.request_json, report_json=excluded.report_json", params![request.context.result_id, chrono::Utc::now().timestamp(), serialized, serde_json::to_string(&SemanticAnalysisReport::empty(SemanticStatus::Pending))?])?;
        self.prune_semantic_shadow()?;
        tx.commit()?;
        Ok(true)
    }

    pub fn finish_semantic_shadow(
        &self,
        request: &SemanticAnalysisRequest,
        report: &SemanticAnalysisReport,
    ) -> Result<bool> {
        request.validate().map_err(invalid)?;
        if report.status == SemanticStatus::Pending
            || (report.status == SemanticStatus::Completed) != report.proposal.is_some()
            || report.provenance.to_string().len() > 4096
        {
            return Err(invalid("Invalid shadow report"));
        }
        if let Some(proposal) = &report.proposal {
            proposal.validate(request).map_err(invalid)?;
        }
        let tx = self.connection.unchecked_transaction()?;
        let profile = self.personalization()?;
        if !profile.automatic_learning
            || profile.learning_epoch != request.context.learning_epoch
            || !self.learning_operation_current(&request.context)?
        {
            return Ok(false);
        }
        self.prune_semantic_shadow()?;
        let changed = tx.execute("UPDATE semantic_shadow SET report_json = ?1 WHERE result_id = ?2 AND request_json = ?3 AND json_extract(report_json, '$.status') = 'pending'", params![serde_json::to_string(report)?, request.context.result_id, serde_json::to_string(request)?])?;
        tx.commit()?;
        Ok(changed == 1)
    }

    pub fn semantic_shadow_records(&self) -> Result<Vec<SemanticShadowRecord>> {
        self.prune_semantic_shadow()?;
        let mut statement = self.connection.prepare("SELECT created_at, request_json, report_json FROM semantic_shadow ORDER BY created_at, rowid")?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        let mut records = Vec::new();
        for row in rows {
            let (created_at, request, report) = row?;
            // A record this build cannot read is skipped, not fatal.
            if let (Ok(request), Ok(report)) = (stored::decode(&request), stored::decode(&report)) {
                records.push(SemanticShadowRecord {
                    created_at,
                    request,
                    report,
                });
            }
        }
        Ok(records)
    }

    pub fn invalidate_semantic_result(&self, result_id: &str) -> Result<()> {
        self.connection.execute(
            "DELETE FROM semantic_shadow WHERE result_id = ?1",
            [result_id],
        )?;
        Ok(())
    }
}

fn invalid(message: &str) -> DatasetError {
    DatasetError::InvalidConfig(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request(store: &DatasetStore, id: &str) -> SemanticAnalysisRequest {
        let p = store.dictation_personalization().unwrap();
        let request = SemanticAnalysisRequest {
            schema_version: 1,
            origin: vesper_core::SemanticOrigin::InAppDraftEdit,
            session_id: None,
            context: vesper_core::LearningContext {
                language: vesper_core::Language::English,
                result_id: id.into(),
                episode_id: id.into(),
                source_key: "source".into(),
                learning_epoch: p.learning_epoch,
                learning_enabled: true,
                ..Default::default()
            },
            result_revision: 1,
            before: "read me".into(),
            after: "README".into(),
            earlier: None,
            memories: vec![],
        };
        store
            .observe_learning(&request.context, &request.before, None)
            .unwrap();
        request
    }
    #[test]
    fn completion_cannot_resurrect_undo_forget_or_superseded_edits() {
        let dir = tempfile::tempdir().unwrap();
        let s = DatasetStore::open(dir.path().into()).unwrap();
        let r = request(&s, "r");
        let report = SemanticAnalysisReport::empty(SemanticStatus::InvalidOutput);
        assert!(s.begin_semantic_shadow(&r).unwrap());
        let mut newer = r.clone();
        newer.result_revision = 2;
        assert!(s.begin_semantic_shadow(&newer).unwrap());
        assert!(!s.finish_semantic_shadow(&r, &report).unwrap());
        s.undo_learning_for_result("r").unwrap();
        assert!(!s.finish_semantic_shadow(&newer, &report).unwrap());
        assert!(s.begin_semantic_shadow(&r).unwrap());
        let revision = s.personalization().unwrap().revision;
        s.forget_automatic_learning(revision).unwrap();
        assert!(!s.finish_semantic_shadow(&r, &report).unwrap());
        assert!(!s.begin_semantic_shadow(&r).unwrap());
        assert!(s.semantic_shadow_records().unwrap().is_empty());
        assert!(s.list_sessions().unwrap().is_empty());
    }
    #[test]
    fn bounded_retained_records_survive_restart_without_replaying_and_expire() {
        let dir = tempfile::tempdir().unwrap();
        let s = DatasetStore::open(dir.path().into()).unwrap();
        for i in 0..260 {
            assert!(s
                .begin_semantic_shadow(&request(&s, &format!("r{i}")))
                .unwrap());
        }
        assert_eq!(s.semantic_shadow_records().unwrap().len(), MAX_PROPOSALS);
        drop(s);
        let s = DatasetStore::open(dir.path().into()).unwrap();
        assert_eq!(s.semantic_shadow_records().unwrap().len(), MAX_PROPOSALS);
        s.connection
            .execute("UPDATE semantic_shadow SET created_at = 1", [])
            .unwrap();
        assert!(s.semantic_shadow_records().unwrap().is_empty());
    }
    #[test]
    fn wrong_scope_and_forged_memories_are_not_reserved() {
        let dir = tempfile::tempdir().unwrap();
        let s = DatasetStore::open(dir.path().into()).unwrap();
        let mut r = request(&s, "r");
        r.context.project = "elsewhere".into();
        assert!(!s.begin_semantic_shadow(&r).unwrap());
        r.context.project.clear();
        r.memories.push(vesper_core::SemanticMemory {
            id: "fake".into(),
            phrase: "fake".into(),
        });
        assert!(!s.begin_semantic_shadow(&r).unwrap());
    }

    #[test]
    fn valid_proposals_never_activate_memory_and_stale_connections_cannot_restore_forget() {
        let dir = tempfile::tempdir().unwrap();
        let s = DatasetStore::open(dir.path().into()).unwrap();
        let other = DatasetStore::open(dir.path().into()).unwrap();
        let r = request(&s, "r");
        let profile = s.personalization().unwrap();
        assert!(s.begin_semantic_shadow(&r).unwrap());
        let mut report = SemanticAnalysisReport::empty(SemanticStatus::Completed);
        report.proposal = Some(r.parse_proposal(r#"{"schemaVersion":1,"relation":"repair","relationEvidence":[{"source":"before","start":0,"end":7},{"source":"after","start":0,"end":6}],"intent":null,"intentEvidence":[],"memories":[]}"#).unwrap());
        assert!(other.finish_semantic_shadow(&r, &report).unwrap());
        assert_eq!(s.personalization().unwrap(), profile);
        assert!(s
            .automatic_learning_state()
            .unwrap()
            .events
            .iter()
            .all(|e| matches!(e.kind, crate::LearningEventKind::RawObservation)));
        assert!(!s.finish_semantic_shadow(&r, &report).unwrap());
        s.forget_automatic_learning(profile.revision).unwrap();
        assert!(!other.finish_semantic_shadow(&r, &report).unwrap());
        assert!(other.semantic_shadow_records().unwrap().is_empty());
    }

    #[test]
    fn saved_output_undo_deletes_its_shadow_and_blocks_late_completion() {
        let dir = tempfile::tempdir().unwrap();
        let s = DatasetStore::open(dir.path().join("data")).unwrap();
        let audio = dir.path().join("authored.wav");
        std::fs::write(&audio, b"storage fixture, never decoded").unwrap();
        let session = s
            .import_audio(&audio, &vesper_core::AppConfig::default())
            .unwrap();
        s.set_transcripts(&session.id, "read me".into(), None, serde_json::json!({}))
            .unwrap();
        let mut r = request(&s, "r");
        r.context.language = vesper_core::Language::English;
        r.session_id = Some(session.id.clone());
        assert!(!s.begin_semantic_shadow(&r).unwrap()); // not yet a committed edit
        s.edit_session_draft(&session.id, 0, &r.before, &r.after, &r.context)
            .unwrap();
        assert!(s.begin_semantic_shadow(&r).unwrap());
        s.undo_personalization(&session.id).unwrap();
        assert!(!s.begin_semantic_shadow(&r).unwrap());
        assert!(!s
            .finish_semantic_shadow(
                &r,
                &SemanticAnalysisReport::empty(SemanticStatus::Cancelled)
            )
            .unwrap());
        assert!(s.semantic_shadow_records().unwrap().is_empty());
    }
}
