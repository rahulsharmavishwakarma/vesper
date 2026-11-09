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
        cleared INTEGER NOT NULL DEFAULT 0,
        sent_edited INTEGER NOT NULL DEFAULT 0,
        watch_notified INTEGER NOT NULL DEFAULT 0,
        watch_polled INTEGER NOT NULL DEFAULT 0,
        field_reads INTEGER NOT NULL DEFAULT 0,
        field_unreadable INTEGER NOT NULL DEFAULT 0,
        field_terms INTEGER NOT NULL DEFAULT 0
    );
    -- Automatic paste outcomes per app; `last_failure` holds AX role names only.
    CREATE TABLE IF NOT EXISTS paste_coverage (
        application TEXT PRIMARY KEY NOT NULL,
        attempts INTEGER NOT NULL DEFAULT 0,
        sent INTEGER NOT NULL DEFAULT 0,
        no_field INTEGER NOT NULL DEFAULT 0,
        no_permission INTEGER NOT NULL DEFAULT 0,
        secure INTEGER NOT NULL DEFAULT 0,
        field_changed INTEGER NOT NULL DEFAULT 0,
        clipboard INTEGER NOT NULL DEFAULT 0,
        other INTEGER NOT NULL DEFAULT 0,
        last_failure TEXT NOT NULL DEFAULT ''
    );";

/// How one automatic paste ended, for the per-app paste counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasteOutcome {
    Sent,
    /// Capture found no editable field (`last_failure` says what had focus).
    NoField,
    NoPermission,
    Secure,
    /// The field or focus changed before delivery.
    FieldChanged,
    Clipboard,
    Other,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PasteCoverage {
    pub application: String,
    pub attempts: u32,
    pub sent: u32,
    pub no_field: u32,
    pub no_permission: u32,
    pub secure: u32,
    pub field_changed: u32,
    pub clipboard: u32,
    pub other: u32,
    pub last_failure: String,
}

/// What reading back one paste found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Readback {
    /// The pasted text is still there as pasted.
    Unchanged,
    /// The user changed it; the change was offered to learning as a draft edit.
    Edited,
    /// The text no longer lines up with what was pasted (rewritten, moved, deleted).
    Unaligned,
    /// The span is now empty: the text was sent or cleared (a chat box after
    /// Return) with no fix the watch could keep.
    Cleared,
    /// The text was sent or cleared, but the watch kept a fix made before
    /// that; it was offered to learning as a draft edit (issue #410).
    EditedBeforeSend,
    /// The field could not be read (closed, secure, or no text access).
    Unreadable,
}

/// How the field was watched before its read-back, for the per-app counts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ReadbackWatch {
    /// No change was seen.
    #[default]
    Quiet,
    /// The app sent value-change notifications.
    Notified,
    /// Only polling saw changes.
    Polled,
}

/// Decide what one read-back found. `text` is what was read from where the
/// paste began; `before_send` says it is the last text seen before the field
/// emptied. Returns the outcome and, for a fix, the corrected pasted text.
pub fn classify_readback(
    pasted: &str,
    text: Option<&str>,
    before_send: bool,
) -> (Readback, Option<String>) {
    let Some(text) = text else {
        return (Readback::Unreadable, None);
    };
    if text.trim().is_empty() {
        return (Readback::Cleared, None);
    }
    let aligned = vesper_core::aligned_edit(pasted, text);
    match (aligned, before_send) {
        (Some(fixed), true) if fixed != pasted => (Readback::EditedBeforeSend, Some(fixed)),
        // Sent as pasted, or the kept text no longer lines up: nothing to learn.
        (_, true) => (Readback::Cleared, None),
        (None, false) => (Readback::Unaligned, None),
        (Some(same), false) if same == pasted => (Readback::Unchanged, None),
        (Some(fixed), false) => (Readback::Edited, Some(fixed)),
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AppLearningCoverage {
    pub application: String,
    pub readbacks: u32,
    pub unchanged: u32,
    pub edited: u32,
    pub unaligned: u32,
    pub unreadable: u32,
    pub cleared: u32,
    /// Fixes kept by the watch from a field that was then sent or cleared.
    pub sent_edited: u32,
    /// Read-backs whose field sent value-change notifications.
    pub watch_notified: u32,
    /// Read-backs where only polling saw the field change.
    pub watch_polled: u32,
    pub field_reads: u32,
    pub field_unreadable: u32,
    /// Total spellings taken from fields in this app.
    pub field_terms: u32,
}

/// Columns added after the table first shipped locally; added in place when missing.
pub(crate) fn migrate(connection: &Connection) -> Result<()> {
    for column in ["cleared", "sent_edited", "watch_notified", "watch_polled"] {
        let present: bool = connection
            .prepare("SELECT 1 FROM pragma_table_info('app_learning_coverage') WHERE name = ?1")?
            .exists([column])?;
        if !present {
            connection.execute_batch(&format!(
                "ALTER TABLE app_learning_coverage ADD COLUMN {column} INTEGER NOT NULL DEFAULT 0;"
            ))?;
        }
    }
    Ok(())
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

    pub fn record_app_readback(
        &self,
        application: &str,
        outcome: Readback,
        watch: ReadbackWatch,
    ) -> Result<()> {
        let column = match outcome {
            Readback::Unchanged => "unchanged",
            Readback::Edited => "edited",
            Readback::Unaligned => "unaligned",
            Readback::Unreadable => "unreadable",
            Readback::Cleared => "cleared",
            Readback::EditedBeforeSend => "sent_edited",
        };
        let mut columns = vec![("readbacks", 1), (column, 1)];
        match watch {
            ReadbackWatch::Quiet => {}
            ReadbackWatch::Notified => columns.push(("watch_notified", 1)),
            ReadbackWatch::Polled => columns.push(("watch_polled", 1)),
        }
        self.bump_app_learning(application, &columns)
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
             field_unreadable, field_terms, cleared, sent_edited, watch_notified, watch_polled \
             FROM app_learning_coverage \
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
                cleared: row.get(9)?,
                sent_edited: row.get(10)?,
                watch_notified: row.get(11)?,
                watch_polled: row.get(12)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Count one automatic paste. `failure` is the capture diagnostic (AX role
    /// names and flags), kept only as the app's latest failure.
    pub fn record_paste(
        &self,
        application: &str,
        outcome: PasteOutcome,
        failure: Option<&str>,
    ) -> Result<()> {
        let known: Option<i64> = self
            .connection
            .query_row(
                "SELECT 1 FROM paste_coverage WHERE application = ?1",
                [application],
                |row| row.get(0),
            )
            .optional()?;
        let apps: i64 =
            self.connection
                .query_row("SELECT COUNT(*) FROM paste_coverage", [], |row| row.get(0))?;
        if application.is_empty() || (known.is_none() && apps as usize >= MAX_DETECTION_APPS) {
            return Ok(());
        }
        let column = match outcome {
            PasteOutcome::Sent => "sent",
            PasteOutcome::NoField => "no_field",
            PasteOutcome::NoPermission => "no_permission",
            PasteOutcome::Secure => "secure",
            PasteOutcome::FieldChanged => "field_changed",
            PasteOutcome::Clipboard => "clipboard",
            PasteOutcome::Other => "other",
        };
        self.connection.execute(
            "INSERT OR IGNORE INTO paste_coverage (application) VALUES (?1)",
            [application],
        )?;
        let failure: String = failure.unwrap_or("").chars().take(192).collect();
        self.connection.execute(
            &format!(
                "UPDATE paste_coverage SET attempts = attempts + 1, {column} = {column} + 1, \
                 last_failure = CASE WHEN ?2 = '' THEN last_failure ELSE ?2 END \
                 WHERE application = ?1"
            ),
            rusqlite::params![application, failure],
        )?;
        Ok(())
    }

    pub fn paste_coverage(&self) -> Result<Vec<PasteCoverage>> {
        let mut statement = self.connection.prepare(
            "SELECT application, attempts, sent, no_field, no_permission, secure, field_changed, \
             clipboard, other, last_failure FROM paste_coverage ORDER BY attempts DESC, application",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(PasteCoverage {
                application: row.get(0)?,
                attempts: row.get(1)?,
                sent: row.get(2)?,
                no_field: row.get(3)?,
                no_permission: row.get(4)?,
                secure: row.get(5)?,
                field_changed: row.get(6)?,
                clipboard: row.get(7)?,
                other: row.get(8)?,
                last_failure: row.get(9)?,
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
            .record_app_readback("com.apple.Notes", Readback::Edited, ReadbackWatch::Notified)
            .unwrap();
        store
            .record_app_readback("com.apple.Notes", Readback::Unchanged, ReadbackWatch::Quiet)
            .unwrap();
        store
            .record_app_readback(
                "com.apple.Notes",
                Readback::EditedBeforeSend,
                ReadbackWatch::Polled,
            )
            .unwrap();
        store.record_field_read("com.apple.Notes", Some(3)).unwrap();
        store.record_field_read("com.apple.Notes", None).unwrap();
        assert_eq!(
            store.app_learning_coverage().unwrap(),
            vec![AppLearningCoverage {
                application: "com.apple.Notes".into(),
                readbacks: 3,
                unchanged: 1,
                edited: 1,
                sent_edited: 1,
                watch_notified: 1,
                watch_polled: 1,
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

    #[test]
    fn a_fix_kept_before_send_teaches_and_a_plain_send_does_not() {
        let pasted = "restart the cubernetes cluster today";
        let fixed = "restart the Kubernetes cluster today";
        // Read in place, as before the watch.
        assert_eq!(
            classify_readback(pasted, None, false),
            (Readback::Unreadable, None)
        );
        assert_eq!(
            classify_readback(pasted, Some("  "), false),
            (Readback::Cleared, None)
        );
        assert_eq!(
            classify_readback(pasted, Some(pasted), false),
            (Readback::Unchanged, None)
        );
        assert_eq!(
            classify_readback(
                pasted,
                Some("restart the Kubernetes cluster today and then more"),
                false
            )
            .0,
            Readback::Edited
        );
        assert_eq!(
            classify_readback(pasted, Some("something else entirely now written"), false),
            (Readback::Unaligned, None)
        );
        // The field was sent: the watch's last text decides.
        assert_eq!(
            classify_readback(pasted, Some(fixed), true),
            (Readback::EditedBeforeSend, Some(fixed.into()))
        );
        assert_eq!(
            classify_readback(pasted, Some(pasted), true),
            (Readback::Cleared, None)
        );
        // Backspaced away word by word before sending: nothing lines up.
        assert_eq!(
            classify_readback(pasted, Some("d"), true),
            (Readback::Cleared, None)
        );
    }

    #[test]
    fn older_tables_gain_the_watch_counts_in_place() {
        let dir = tempfile::tempdir().unwrap();
        {
            let connection = Connection::open(dir.path().join("sessions.sqlite3")).unwrap();
            connection
                .execute_batch(
                    "CREATE TABLE app_learning_coverage (
                        application TEXT PRIMARY KEY NOT NULL,
                        readbacks INTEGER NOT NULL DEFAULT 0,
                        unchanged INTEGER NOT NULL DEFAULT 0,
                        edited INTEGER NOT NULL DEFAULT 0,
                        unaligned INTEGER NOT NULL DEFAULT 0,
                        unreadable INTEGER NOT NULL DEFAULT 0,
                        field_reads INTEGER NOT NULL DEFAULT 0,
                        field_unreadable INTEGER NOT NULL DEFAULT 0,
                        field_terms INTEGER NOT NULL DEFAULT 0
                    );
                    INSERT INTO app_learning_coverage (application, readbacks, unreadable)
                    VALUES ('com.microsoft.VSCode', 4, 4);",
                )
                .unwrap();
        }
        let store = DatasetStore::open(dir.path().into()).unwrap();
        store
            .record_app_readback(
                "com.microsoft.VSCode",
                Readback::EditedBeforeSend,
                ReadbackWatch::Notified,
            )
            .unwrap();
        let row = &store.app_learning_coverage().unwrap()[0];
        assert_eq!((row.readbacks, row.unreadable), (5, 4));
        assert_eq!(
            (row.sent_edited, row.watch_notified, row.watch_polled),
            (1, 1, 0)
        );
    }

    #[test]
    fn paste_counts_keep_the_latest_failure_reason() {
        let dir = tempfile::tempdir().unwrap();
        let store = DatasetStore::open(dir.path().into()).unwrap();
        let brave = "com.brave.Browser";
        store
            .record_paste(
                brave,
                PasteOutcome::NoField,
                Some("no-editable-field role=AXGroup"),
            )
            .unwrap();
        store.record_paste(brave, PasteOutcome::Sent, None).unwrap();
        let rows = store.paste_coverage().unwrap();
        assert_eq!(
            (
                rows[0].attempts,
                rows[0].sent,
                rows[0].no_field,
                rows[0].last_failure.as_str()
            ),
            (2, 1, 1, "no-editable-field role=AXGroup")
        );
    }
}
