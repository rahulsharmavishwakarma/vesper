//! Explicit confirmation of bounded external spelling candidates. Never labels.
use crate::{DatasetError, DatasetStore, ExternalShadowRecord, Result};
use rusqlite::OptionalExtension;
use serde::Serialize;
use vesper_core::{
    correction_suggestions, CorrectionPair, Language, PersonalizationProfile, VocabularyEntry,
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalCorrectionReview {
    pub token: String,
    pub before: String,
    pub after: String,
    pub project: String,
    pub language: Language,
    pub pairs: Vec<CorrectionPair>,
}

impl DatasetStore {
    fn external_review_current(&self, record: &ExternalShadowRecord) -> Result<bool> {
        if record.schema_version != 1
            || record.origin != "external_textedit_shadow"
            || record.after.is_none()
            || !self.learning_operation_current(&record.context)?
        {
            return Ok(false);
        }
        if let Some(id) = &record.session_id {
            let saved: Option<String> = self
                .connection
                .query_row("SELECT record_json FROM sessions WHERE id=?1", [id], |r| {
                    r.get(0)
                })
                .optional()?;
            let Some(saved) = saved else {
                return Ok(false);
            };
            if serde_json::from_str::<crate::SessionRecord>(&saved)?.personalization_rejected {
                return Ok(false);
            }
        }
        Ok(true)
    }

    pub fn external_correction_reviews(&self) -> Result<Vec<ExternalCorrectionReview>> {
        let mut reviews = Vec::new();
        for record in self.external_shadow_records()?.into_iter().rev() {
            if !self.external_review_current(&record)? {
                continue;
            }
            let after = record.after.as_deref().unwrap_or_default();
            let pairs = correction_suggestions(&record.before, after);
            if pairs.is_empty() {
                continue;
            }
            reviews.push(ExternalCorrectionReview {
                pairs,
                token: record.token,
                before: record.before,
                after: after.into(),
                project: record.context.project,
                language: record.context.language,
            });
            if reviews.len() == 16 {
                break;
            }
        }
        Ok(reviews)
    }

    /// Consume one candidate and remember one user-confirmed spelling atomically.
    /// Remaining changes in the span are not treated as corrections or labels.
    pub fn remember_external_correction(
        &self,
        token: &str,
        pair: &CorrectionPair,
        revision: u64,
    ) -> Result<PersonalizationProfile> {
        let tx = self.connection.unchecked_transaction()?;
        self.prune_external_shadow()?;
        let serialized: Option<String> = tx
            .query_row(
                "SELECT record_json FROM external_shadow WHERE token=?1",
                [token],
                |r| r.get(0),
            )
            .optional()?;
        let record: ExternalShadowRecord = serialized
            .as_deref()
            .map(serde_json::from_str)
            .transpose()?
            .ok_or_else(|| {
                DatasetError::InvalidConfig("This correction is no longer available.".into())
            })?;
        if !self.external_review_current(&record)?
            || !correction_suggestions(&record.before, record.after.as_deref().unwrap_or_default())
                .contains(pair)
        {
            return Err(DatasetError::InvalidConfig(
                "This spelling is not supported by the captured correction.".into(),
            ));
        }
        let mut profile = self.personalization()?;
        if profile.revision != revision {
            return Err(DatasetError::InvalidConfig(
                "Preferences changed; refresh before remembering this correction.".into(),
            ));
        }
        // Reuse an exact manual memory only. Do not relabel another automatic or
        // reviewed source, expand its aliases, or silently enable a conflicting word.
        let existing = profile.vocabulary.iter_mut().find(|word| {
            word.phrase == pair.intended
                && word.aliases.contains(&pair.heard)
                && word.project == record.context.project
                && word.language == record.context.language
                && word.context.is_empty()
                && !word.pronunciation
                && word.automatic_source.is_none()
        });
        if let Some(word) = existing {
            word.enabled = true;
        } else {
            profile.vocabulary.push(VocabularyEntry {
                id: uuid::Uuid::new_v4().to_string(),
                phrase: pair.intended.clone(),
                aliases: vec![pair.heard.clone()],
                language: record.context.language,
                project: record.context.project,
                enabled: true,
                ..Default::default()
            });
        }
        tx.execute("DELETE FROM external_shadow WHERE token=?1", [token])?;
        let saved = self.save_personalization_in_transaction(profile)?;
        tx.commit()?;
        Ok(saved)
    }

    pub fn dismiss_external_correction(&self, token: &str) -> Result<()> {
        self.connection
            .execute("DELETE FROM external_shadow WHERE token=?1", [token])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn candidate(store: &DatasetStore) -> ExternalShadowRecord {
        let p = store.dictation_personalization().unwrap();
        let context = vesper_core::LearningContext {
            result_id: uuid::Uuid::new_v4().to_string(),
            source_key: "fixture-audio".into(),
            episode_id: "episode".into(),
            language: Language::English,
            project: "Original".into(),
            learning_enabled: true,
            learning_epoch: p.learning_epoch,
            ..Default::default()
        };
        store
            .observe_learning(&context, "open read me in the repository", None)
            .unwrap();
        let r = store
            .reserve_external_shadow(&context, None, "open read me in the repository")
            .unwrap()
            .unwrap();
        assert!(store
            .finish_external_shadow(&r, &r.before, "open README in the repository")
            .unwrap());
        r
    }
    #[test]
    fn reviewed_external_spelling_is_scoped_exactly_once_and_never_a_training_label() {
        let dir = tempfile::tempdir().unwrap();
        let store = DatasetStore::open(dir.path().into()).unwrap();
        let r = candidate(&store);
        let review = store.external_correction_reviews().unwrap().remove(0);
        let mut profile = store.personalization().unwrap();
        profile.active_project = "Other".into();
        let profile = store.save_personalization(profile).unwrap();
        assert!(store
            .remember_external_correction(&r.token, &review.pairs[0], profile.revision - 1)
            .is_err());
        assert_eq!(store.external_correction_reviews().unwrap().len(), 1);
        let saved = store
            .remember_external_correction(&r.token, &review.pairs[0], profile.revision)
            .unwrap();
        assert_eq!(saved.vocabulary.len(), 1);
        assert_eq!(saved.vocabulary[0].project, "Original");
        assert!(store
            .remember_external_correction(&r.token, &review.pairs[0], saved.revision)
            .is_err());
        assert!(store.list_sessions().unwrap().is_empty());
        assert!(saved.examples.is_empty());
        assert!(saved.vocabulary[0].sources.is_empty());
        assert!(store
            .automatic_learning_state()
            .unwrap()
            .events
            .iter()
            .all(|e| matches!(e.kind, crate::LearningEventKind::RawObservation)));
        assert!(store.external_correction_reviews().unwrap().is_empty());
    }
    #[test]
    fn forged_pair_dismiss_undo_and_forget_cannot_admit_a_candidate() {
        let dir = tempfile::tempdir().unwrap();
        let store = DatasetStore::open(dir.path().into()).unwrap();
        let r = candidate(&store);
        let review = store.external_correction_reviews().unwrap().remove(0);
        let p = store.personalization().unwrap();
        assert!(store
            .remember_external_correction(
                &r.token,
                &CorrectionPair {
                    heard: "me".into(),
                    intended: "fabricated".into()
                },
                p.revision
            )
            .is_err());
        store.dismiss_external_correction(&r.token).unwrap();
        assert!(store
            .remember_external_correction(&r.token, &review.pairs[0], p.revision)
            .is_err());
        let r = candidate(&store);
        store
            .undo_learning_for_result(&r.context.result_id)
            .unwrap();
        assert!(store
            .remember_external_correction(&r.token, &review.pairs[0], p.revision)
            .is_err());
        let r = candidate(&store);
        store.forget_automatic_learning(p.revision).unwrap();
        assert!(store
            .remember_external_correction(&r.token, &review.pairs[0], p.revision)
            .is_err());
        assert!(store.personalization().unwrap().vocabulary.is_empty());
    }
}
