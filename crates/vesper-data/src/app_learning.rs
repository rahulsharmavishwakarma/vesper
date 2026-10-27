//! Per-app counts for the opt-in app-learning features (field words and fix
//! read-back; docs/personalization/app-learning.md): the app identifier and
//! counts only, never the text read. Cleared when both features are off.
use crate::*;

pub(crate) const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS app_learning_coverage (
        application TEXT PRIMARY KEY NOT NULL,
        readbacks INTEGER NOT NULL DEFAULT 0,
        unchanged INTEGER NOT NULL DEFAULT 0,
        edited INTEGER NOT NULL DEFAULT 0,
        unaligned INTEGER NOT NULL DEFAULT 0,
        unreadable INTEGER NOT NULL DEFAULT 0,
        field_reads INTEGER NOT NULL DEFAULT 0,
        field_unreadable INTEGER NOT NULL DEFAULT 0,
        field_terms INTEGER NOT NULL DEFAULT 0
    );";

/// What reading back one paste found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Readback {
    /// The pasted text is still there as pasted.
    Unchanged,
    /// The user changed it; the change was offered to learning as a draft edit.
    Edited,
    /// The text no longer lines up with what was pasted (rewritten, moved, deleted).
    Unaligned,
    /// The field could not be read (closed, secure, or no text access).
    Unreadable,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppLearningCoverage {
    pub application: String,
    pub readbacks: u32,
    pub unchanged: u32,
    pub edited: u32,
    pub unaligned: u32,
    pub unreadable: u32,
    pub field_reads: u32,
    pub field_unreadable: u32,
    /// Total spellings taken from fields in this app.
    pub field_terms: u32,
}

impl DatasetStore {
    fn bump_app_learning(&self, application: &str, columns: &[(&str, u32)]) -> Result<()> {
        let known: Option<i64> = self
            .connection
            .query_row(
                "SELECT 1 FROM app_learning_coverage WHERE application = ?1",
                [application],
                |row| row.get(0),
            )
            .optional()?;
        let apps: i64 =
            self.connection
                .query_row("SELECT COUNT(*) FROM app_learning_coverage", [], |row| {
                    row.get(0)
                })?;
        if application.is_empty() || (known.is_none() && apps as usize >= MAX_DETECTION_APPS) {
            return Ok(());
        }
        self.connection.execute(
            "INSERT OR IGNORE INTO app_learning_coverage (application) VALUES (?1)",
            [application],
        )?;
        let set = columns
            .iter()
            .map(|(column, n)| format!("{column} = {column} + {n}"))
            .collect::<Vec<_>>()
            .join(", ");
        self.connection.execute(
            &format!("UPDATE app_learning_coverage SET {set} WHERE application = ?1"),
            [application],
        )?;
        Ok(())
    }

    pub fn record_app_readback(&self, application: &str, outcome: Readback) -> Result<()> {
        let column = match outcome {
            Readback::Unchanged => "unchanged",
            Readback::Edited => "edited",
            Readback::Unaligned => "unaligned",
            Readback::Unreadable => "unreadable",
        };
        self.bump_app_learning(application, &[("readbacks", 1), (column, 1)])
    }

    /// `terms` is `None` when the field could not be read.
    pub fn record_field_read(&self, application: &str, terms: Option<usize>) -> Result<()> {
        match terms {
            Some(n) => self.bump_app_learning(
                application,
                &[
                    ("field_reads", 1),
                    ("field_terms", n.min(u32::MAX as usize) as u32),
                ],
            ),
            None => {
                self.bump_app_learning(application, &[("field_reads", 1), ("field_unreadable", 1)])
            }
        }
    }

    pub fn app_learning_coverage(&self) -> Result<Vec<AppLearningCoverage>> {
        let mut statement = self.connection.prepare(
            "SELECT application, readbacks, unchanged, edited, unaligned, unreadable, field_reads, \
             field_unreadable, field_terms FROM app_learning_coverage \
             ORDER BY readbacks + field_reads DESC, application",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(AppLearningCoverage {
                application: row.get(0)?,
                readbacks: row.get(1)?,
                unchanged: row.get(2)?,
                edited: row.get(3)?,
                unaligned: row.get(4)?,
                unreadable: row.get(5)?,
                field_reads: row.get(6)?,
                field_unreadable: row.get(7)?,
                field_terms: row.get(8)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn clear_app_learning_coverage(&self) -> Result<()> {
        self.connection
            .execute("DELETE FROM app_learning_coverage", [])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_per_app_and_clear_when_both_features_turn_off() {
        let dir = tempfile::tempdir().unwrap();
        let store = DatasetStore::open(dir.path().into()).unwrap();
        let mut profile = store.personalization().unwrap();
        profile.learn_from_app_edits = true;
        profile.app_field_words = true;
        let profile = store.save_personalization(profile).unwrap();
        store
            .record_app_readback("com.apple.Notes", Readback::Edited)
            .unwrap();
        store
            .record_app_readback("com.apple.Notes", Readback::Unchanged)
            .unwrap();
        store.record_field_read("com.apple.Notes", Some(3)).unwrap();
        store.record_field_read("com.apple.Notes", None).unwrap();
        assert_eq!(
            store.app_learning_coverage().unwrap(),
            vec![AppLearningCoverage {
                application: "com.apple.Notes".into(),
                readbacks: 2,
                unchanged: 1,
                edited: 1,
                field_reads: 2,
                field_unreadable: 1,
                field_terms: 3,
                ..Default::default()
            }]
        );
        let mut partly = profile.clone();
        partly.app_field_words = false;
        let partly = store.save_personalization(partly).unwrap();
        assert_eq!(
            store.app_learning_coverage().unwrap().len(),
            1,
            "one feature still on"
        );
        let mut off = partly;
        off.learn_from_app_edits = false;
        store.save_personalization(off).unwrap();
        assert!(store.app_learning_coverage().unwrap().is_empty());
    }
}
