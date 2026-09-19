//! Explicitly separate external candidates, never training targets or vocabulary evidence.
use crate::{DatasetStore, Result};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use transcribe_core::LearningContext;
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExternalShadowRecord {
    pub schema_version: u32,
    pub origin: String,
    pub token: String,
    pub context: LearningContext,
    pub session_id: Option<String>,
    pub before: String,
    pub after: Option<String>,
    pub created_at: i64,
}
fn bounded(text: &str) -> bool {
    !text.is_empty() && !text.contains('\0') && text.encode_utf16().count() <= 8192
}
impl DatasetStore {
    pub(crate) fn prune_external_shadow(&self) -> Result<()> {
        self.connection.execute(
            "DELETE FROM external_shadow WHERE created_at<=?1",
            [chrono::Utc::now().timestamp() - 30 * 24 * 60 * 60],
        )?;
        self.connection.execute("DELETE FROM external_shadow WHERE token IN (SELECT token FROM external_shadow ORDER BY created_at DESC,rowid DESC LIMIT -1 OFFSET 256)", [])?;
        Ok(())
    }
    pub fn reserve_external_shadow(
        &self,
        context: &LearningContext,
        session_id: Option<String>,
        before: &str,
    ) -> Result<Option<ExternalShadowRecord>> {
        if !bounded(before)
            || !context.learning_enabled
            || context.result_id.is_empty()
            || context.result_id.len() > 64
            || context.source_key.is_empty()
            || context.source_key.len() > 128
            || context.episode_id.is_empty()
            || context.episode_id.len() > 64
        {
            return Ok(None);
        }
        let tx = self.connection.unchecked_transaction()?;
        let profile = self.personalization()?;
        if !profile.automatic_learning
            || profile.learning_epoch != context.learning_epoch
            || profile.active_project != context.project
        {
            return Ok(None);
        }
        if let Some(id) = &session_id {
            if crate::read_record(&tx, id)?.personalization_rejected {
                return Ok(None);
            }
        }
        let record = ExternalShadowRecord {
            schema_version: 1,
            origin: "external_textedit_shadow".into(),
            token: uuid::Uuid::new_v4().to_string(),
            context: context.clone(),
            session_id,
            before: before.into(),
            after: None,
            created_at: chrono::Utc::now().timestamp(),
        };
        // At most one insertion can still be observed. Never replay a receipt on restart.
        tx.execute(
            "DELETE FROM external_shadow WHERE json_extract(record_json,'$.after') IS NULL",
            [],
        )?;
        tx.execute(
            "INSERT INTO external_shadow (token,created_at,record_json) VALUES (?1,?2,?3)",
            params![
                record.token,
                record.created_at,
                serde_json::to_string(&record)?
            ],
        )?;
        self.prune_external_shadow()?;
        tx.commit()?;
        Ok(Some(record))
    }
    pub fn finish_external_shadow(
        &self,
        reservation: &ExternalShadowRecord,
        before: &str,
        after: &str,
    ) -> Result<bool> {
        if reservation.schema_version != 1
            || reservation.origin != "external_textedit_shadow"
            || reservation.after.is_some()
            || before != reservation.before
            || !bounded(after)
            || before == after
        {
            return Ok(false);
        }
        let tx = self.connection.unchecked_transaction()?;
        self.prune_external_shadow()?;
        let profile = self.personalization()?;
        if !profile.automatic_learning
            || profile.learning_epoch != reservation.context.learning_epoch
            || profile.active_project != reservation.context.project
        {
            return Ok(false);
        }
        let mut complete = reservation.clone();
        complete.after = Some(after.into());
        let updated = tx.execute(
            "UPDATE external_shadow SET record_json=?1 WHERE token=?2 AND record_json=?3",
            params![
                serde_json::to_string(&complete)?,
                reservation.token,
                serde_json::to_string(reservation)?
            ],
        )?;
        tx.commit()?;
        Ok(updated == 1)
    }
    pub fn external_shadow_records(&self) -> Result<Vec<ExternalShadowRecord>> {
        self.prune_external_shadow()?;
        let mut statement = self
            .connection
            .prepare("SELECT record_json FROM external_shadow ORDER BY created_at,rowid")?;
        let rows = statement.query_map([], |r| r.get::<_, String>(0))?;
        rows.map(|r| Ok(serde_json::from_str(&r?)?)).collect()
    }
    pub fn clear_external_shadow(&self) -> Result<()> {
        self.connection.execute("DELETE FROM external_shadow", [])?;
        Ok(())
    }
    pub(crate) fn invalidate_external_result(&self, result_id: &str) -> Result<()> {
        self.connection.execute(
            "DELETE FROM external_shadow WHERE json_extract(record_json,'$.context.resultId')=?1",
            [result_id],
        )?;
        Ok(())
    }
    pub fn discard_external_reservation(&self, token: &str) -> Result<()> {
        let old: Option<String> = self
            .connection
            .query_row(
                "SELECT record_json FROM external_shadow WHERE token=?1",
                [token],
                |r| r.get(0),
            )
            .optional()?;
        if old
            .as_deref()
            .and_then(|s| serde_json::from_str::<ExternalShadowRecord>(s).ok())
            .is_some_and(|r| r.after.is_none())
        {
            self.connection
                .execute("DELETE FROM external_shadow WHERE token=?1", [token])?;
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn external_candidates_are_separate_bounded_exactly_once_and_forget_cannot_resurrect() {
        let dir = tempfile::tempdir().unwrap();
        let store = DatasetStore::open(dir.path().into()).unwrap();
        let profile = store.dictation_personalization().unwrap();
        let context = LearningContext {
            result_id: "r".into(),
            source_key: "audio".into(),
            episode_id: "e".into(),
            learning_enabled: true,
            learning_epoch: profile.learning_epoch,
            ..Default::default()
        };
        let r = store
            .reserve_external_shadow(&context, None, "read me 😀")
            .unwrap()
            .unwrap();
        assert!(!store
            .finish_external_shadow(&r, "different", "README 😀")
            .unwrap());
        assert!(store
            .finish_external_shadow(&r, &r.before, "README 😀")
            .unwrap());
        assert!(!store
            .finish_external_shadow(&r, &r.before, "README 😀")
            .unwrap());
        assert_eq!(store.personalization().unwrap(), profile);
        assert!(store.list_sessions().unwrap().is_empty());
        assert!(store.automatic_learning_state().unwrap().events.is_empty());
        let stale = store
            .reserve_external_shadow(&context, None, "old")
            .unwrap()
            .unwrap();
        store.forget_automatic_learning(profile.revision).unwrap();
        assert!(!store.finish_external_shadow(&stale, "old", "new").unwrap());
        assert!(store.external_shadow_records().unwrap().is_empty());
        assert!(!bounded(&"😀".repeat(4097)));
        assert!(!bounded("one\0two"));
    }
    #[test]
    fn new_receipt_and_undo_invalidate_prior_completions() {
        let dir = tempfile::tempdir().unwrap();
        let store = DatasetStore::open(dir.path().into()).unwrap();
        let p = store.dictation_personalization().unwrap();
        let c = LearningContext {
            result_id: "r".into(),
            source_key: "s".into(),
            episode_id: "e".into(),
            learning_enabled: true,
            learning_epoch: p.learning_epoch,
            ..Default::default()
        };
        let a = store
            .reserve_external_shadow(&c, None, "a")
            .unwrap()
            .unwrap();
        let b = store
            .reserve_external_shadow(&c, None, "b")
            .unwrap()
            .unwrap();
        assert!(!store.finish_external_shadow(&a, "a", "edit").unwrap());
        store.undo_learning_for_result("r").unwrap();
        assert!(!store.finish_external_shadow(&b, "b", "edit").unwrap());
    }
}
