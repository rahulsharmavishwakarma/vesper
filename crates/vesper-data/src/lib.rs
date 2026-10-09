//! Local audio and correction storage. This crate does not run inference or training.

mod learning;
mod personalization;
pub mod policy;
mod policy_shadow;
mod semantic;
pub use learning::{
    learning_audio_key, AutomaticLearningState, DictationPersonalizationSnapshot, EditSupport,
    LearningCandidate, LearningEvent, LearningEventKind,
};
pub use personalization::{
    CorrectionSuggestion, LearningDashboard, RememberCorrection, ReplayGroup, ReviewCandidate,
};
pub use policy_shadow::{
    PolicyReservation, PolicyShadowDecision, PolicyShadowJob, PolicyShadowState,
};
pub use semantic::SemanticShadowRecord;

use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use chrono::{SecondsFormat, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;
use vesper_core::{AppConfig, LearningContext, LearningOutcome};

pub type Result<T> = std::result::Result<T, DatasetError>;
pub const MAX_TRANSCRIPT_CHARS: usize = 100_000;
pub const MAX_AUDIO_BYTES: u64 = 512 * 1024 * 1024;
pub const SUPPORTED_AUDIO_EXTENSIONS: &[&str] = &[
    "wav", "mp3", "m4a", "flac", "ogg", "opus", "aac", "aif", "aiff", "webm", "mp4",
];

#[derive(Debug, thiserror::Error)]
pub enum DatasetError {
    #[error("dataset I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("dataset database failed: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("dataset serialization failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("data collection is disabled in settings")]
    CollectionDisabled,
    #[error("invalid configuration: {0}")]
    InvalidConfig(String),
    #[error("audio source must be a nonempty regular file")]
    InvalidAudio,
    #[error("audio import exceeds the 512 MiB limit")]
    AudioTooLarge,
    #[error("unsupported audio filename extension")]
    InvalidAudioExtension,
    #[error("session not found: {0}")]
    SessionNotFound(String),
    #[error("an approved transcript or correction must contain text")]
    EmptyCorrection,
    #[error("transcript exceeds the 100000 character limit")]
    TranscriptTooLong,
    #[error("original machine predictions are already stored for session {0}")]
    PredictionsAlreadyStored(String),
    #[error("model version metadata must be a JSON object")]
    InvalidModelVersions,
    #[error("raw transcription must be stored before text polishing")]
    MissingRawTranscript,
    #[error("unsafe stored audio path: {0}")]
    UnsafeAudioPath(String),
    #[error("stored audio integrity check failed for session {0}")]
    AudioIntegrity(String),
    #[error("export destination must be a new or empty directory outside the dataset store")]
    InvalidExportDestination,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewStatus {
    Pending,
    Approved,
}

/// A personal usefulness rating, independent of reviewed labels or training eligibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TranscriptFeedback {
    Like,
    Dislike,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewTargetKind {
    VerbatimTranscription,
    CleanupCorrection,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewRevision {
    pub id: String,
    pub target_kind: ReviewTargetKind,
    pub text: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRecord {
    pub id: String,
    pub created_at: String,
    pub original_filename: String,
    pub audio_relative_path: String,
    pub audio_sha256: String,
    pub audio_bytes: u64,
    pub raw_transcript: Option<String>,
    #[serde(default)]
    pub speaker_timeline: Option<vesper_core::SpeakerTimeline>,
    pub cleaned_transcript: Option<String>,
    #[serde(default)]
    pub personalized_transcript: Option<String>,
    /// Ordinary edited output, not a human-reviewed audio/cleanup training target.
    #[serde(default)]
    pub draft_transcript: Option<String>,
    #[serde(default)]
    pub draft_revision: u64,
    #[serde(default)]
    pub personalization_evidence: Option<vesper_core::MemoryEvidence>,
    #[serde(default)]
    pub personalization_project: String,
    #[serde(default)]
    pub project_selection: Option<vesper_core::ProjectSelection>,
    #[serde(default)]
    pub personalization_rejected: bool,
    pub corrected_transcript: Option<String>,
    /// Human-verified verbatim speech; distinct from edited prose.
    #[serde(default)]
    pub verified_transcript: Option<String>,
    #[serde(default)]
    pub review_history: Vec<ReviewRevision>,
    pub review_status: ReviewStatus,
    #[serde(default)]
    pub transcript_feedback: Option<TranscriptFeedback>,
    pub config_snapshot: AppConfig,
    /// Actual model artifact/runtime revisions, when inference is integrated.
    #[serde(default = "empty_object")]
    pub model_versions: Value,
    #[serde(default)]
    pub pipeline_events: Vec<PipelineEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PipelineEvent {
    pub stage: String,
    pub message: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportSummary {
    pub destination: String,
    pub session_count: usize,
    pub approved_only: bool,
    pub manifest_path: String,
    pub metadata_path: String,
}

/// A connection to a dataset directory. Synchronize access when sharing across threads.
pub struct DatasetStore {
    root: PathBuf,
    connection: Connection,
}

impl DatasetStore {
    pub fn open(root: PathBuf) -> Result<Self> {
        Self::open_with_lock_timeout(root, Duration::from_secs(5))
    }

    /// Background completion must not wait seconds behind a foreground writer.
    /// This bounds SQLite lock waiting, not filesystem or native call duration.
    pub fn open_shadow_completion(root: PathBuf) -> Result<Self> {
        Self::open_with_lock_timeout(root, Duration::from_millis(25))
    }

    fn open_with_lock_timeout(root: PathBuf, lock_timeout: Duration) -> Result<Self> {
        fs::create_dir_all(&root)?;
        let root = root.canonicalize()?;
        fs::create_dir_all(root.join("audio"))?;
        if !root.join("audio").canonicalize()?.starts_with(&root) {
            return Err(DatasetError::UnsafeAudioPath("audio".into()));
        }
        let mut connection = Connection::open(root.join("sessions.sqlite3"))?;
        connection.busy_timeout(lock_timeout)?;
        // Reserve the writer before reading mutation inputs. In WAL mode a
        // deferred read cannot upgrade after a background writer commits, even
        // with a busy timeout. Explicit read-only snapshots stay deferred.
        connection.set_transaction_behavior(rusqlite::TransactionBehavior::Immediate);
        connection.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = FULL;
             CREATE TABLE IF NOT EXISTS sessions (
                 id TEXT PRIMARY KEY NOT NULL,
                 created_at TEXT NOT NULL,
                 review_status TEXT NOT NULL CHECK(review_status IN ('pending', 'approved')),
                 record_json TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS personalization (
                 id INTEGER PRIMARY KEY CHECK(id = 1),
                 profile_json TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS automatic_learning (
                 id INTEGER PRIMARY KEY CHECK(id = 1),
                 state_json TEXT NOT NULL
             );
             -- TextEdit external observation was removed; drop retained candidate rows.
             DROP TABLE IF EXISTS external_shadow;
             CREATE TABLE IF NOT EXISTS policy_shadow (id INTEGER PRIMARY KEY CHECK(id=1), state_json TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS policy_shadow_job (id INTEGER PRIMARY KEY CHECK(id=1), request_json TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS semantic_shadow (
                 result_id TEXT PRIMARY KEY NOT NULL,
                 created_at INTEGER NOT NULL,
                 request_json TEXT NOT NULL,
                 report_json TEXT NOT NULL
             );",
        )?;
        let store = Self { root, connection };
        store.prune_semantic_shadow()?;
        store.policy_shadow_state()?;
        Ok(store)
    }

    /// Copy original bytes into managed storage; an import never invents a transcript.
    pub fn import_audio(&self, source: &Path, config: &AppConfig) -> Result<SessionRecord> {
        config
            .validate()
            .map_err(|error| DatasetError::InvalidConfig(error.to_string()))?;
        if !config.collect_training_data {
            return Err(DatasetError::CollectionDisabled);
        }
        let mut input = File::open(source)?;
        let metadata = input.metadata()?;
        if !metadata.is_file() || metadata.len() == 0 {
            return Err(DatasetError::InvalidAudio);
        }
        if metadata.len() > MAX_AUDIO_BYTES {
            return Err(DatasetError::AudioTooLarge);
        }
        let id = Uuid::new_v4().to_string();
        let extension = safe_extension(source);
        if !SUPPORTED_AUDIO_EXTENSIONS.contains(&extension.as_str()) {
            return Err(DatasetError::InvalidAudioExtension);
        }
        let relative = format!("audio/{id}.{extension}");
        let final_path = self.root.join(&relative);
        let staging_path = self.root.join("audio").join(format!(".{id}.tmp"));
        let operation = (|| {
            let mut staging = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&staging_path)?;
            let (audio_bytes, audio_sha256) = copy_with_hash(&mut input, &mut staging)?;
            if audio_bytes == 0 {
                return Err(DatasetError::InvalidAudio);
            }
            staging.sync_all()?;
            drop(staging);
            let record = SessionRecord {
                id,
                created_at: now(),
                original_filename: source
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
                audio_relative_path: relative,
                audio_sha256,
                audio_bytes,
                raw_transcript: None,
                speaker_timeline: None,
                cleaned_transcript: None,
                corrected_transcript: None,
                verified_transcript: None,
                review_history: Vec::new(),
                personalized_transcript: None,
                draft_transcript: None,
                draft_revision: 0,
                personalization_evidence: None,
                personalization_project: String::new(),
                project_selection: None,
                personalization_rejected: false,
                review_status: ReviewStatus::Pending,
                transcript_feedback: None,
                config_snapshot: config.clone(),
                model_versions: empty_object(),
                pipeline_events: Vec::new(),
            };
            let serialized = serde_json::to_string(&record)?;
            fs::rename(&staging_path, &final_path)?;
            sync_directory(&self.root.join("audio"))?;
            self.connection.execute(
                "INSERT INTO sessions (id, created_at, review_status, record_json)
                 VALUES (?1, ?2, 'pending', ?3)",
                params![record.id, record.created_at, serialized],
            )?;
            Ok(record)
        })();
        if operation.is_err() {
            let _ = fs::remove_file(&staging_path);
            let _ = fs::remove_file(&final_path);
        }
        operation
    }

    pub fn list_sessions(&self) -> Result<Vec<SessionRecord>> {
        let mut statement = self
            .connection
            .prepare("SELECT record_json FROM sessions ORDER BY created_at DESC, id")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        rows.map(|row| Ok(serde_json::from_str(&row?)?)).collect()
    }

    /// Store original machine predictions once, separately from human labels.
    /// Repeated calls are rejected so the original predictions remain available.
    pub fn set_transcripts(
        &self,
        id: &str,
        raw: String,
        cleaned: Option<String>,
        model_versions: Value,
    ) -> Result<SessionRecord> {
        if !model_versions.is_object() {
            return Err(DatasetError::InvalidModelVersions);
        }
        validate_text(&raw, false)?;
        if let Some(text) = &cleaned {
            validate_text(text, false)?;
        }
        let transaction = self.connection.unchecked_transaction()?;
        let mut record = read_record(&transaction, id)?;
        if record.raw_transcript.is_some() || record.cleaned_transcript.is_some() {
            return Err(DatasetError::PredictionsAlreadyStored(id.into()));
        }
        record.raw_transcript = Some(raw);
        record.cleaned_transcript = cleaned;
        record.model_versions = model_versions;
        write_record(&transaction, &record)?;
        transaction.commit()?;
        Ok(record)
    }

    /// Append the first cleanup result after the raw ASR result is durable.
    pub fn complete_cleanup(&self, id: &str, cleaned: String) -> Result<SessionRecord> {
        validate_text(&cleaned, false)?;
        let transaction = self.connection.unchecked_transaction()?;
        let mut record = read_record(&transaction, id)?;
        if record.raw_transcript.is_none() {
            return Err(DatasetError::MissingRawTranscript);
        }
        if record.cleaned_transcript.is_some() {
            return Err(DatasetError::PredictionsAlreadyStored(id.into()));
        }
        record.cleaned_transcript = Some(cleaned);
        write_record(&transaction, &record)?;
        transaction.commit()?;
        Ok(record)
    }

    /// Append anonymous machine activity without creating a reviewed label.
    pub fn complete_speaker_timeline(
        &self,
        id: &str,
        timeline: vesper_core::SpeakerTimeline,
    ) -> Result<SessionRecord> {
        timeline
            .validate()
            .map_err(|_| DatasetError::InvalidModelVersions)?;
        let transaction = self.connection.unchecked_transaction()?;
        let mut record = read_record(&transaction, id)?;
        if record.raw_transcript.is_none() {
            return Err(DatasetError::MissingRawTranscript);
        }
        if record.speaker_timeline.is_some() {
            return Err(DatasetError::PredictionsAlreadyStored(id.into()));
        }
        record.speaker_timeline = Some(timeline);
        write_record(&transaction, &record)?;
        transaction.commit()?;
        Ok(record)
    }

    pub fn record_pipeline_event(
        &self,
        id: &str,
        stage: &str,
        message: &str,
    ) -> Result<SessionRecord> {
        validate_text(message, false)?;
        let transaction = self.connection.unchecked_transaction()?;
        let mut record = read_record(&transaction, id)?;
        record.pipeline_events.push(PipelineEvent {
            stage: stage.into(),
            message: message.into(),
            timestamp: now(),
        });
        write_record(&transaction, &record)?;
        transaction.commit()?;
        Ok(record)
    }

    /// Update only the rating in the existing record, preserving all review metadata.
    pub fn set_transcript_feedback(
        &self,
        id: &str,
        feedback: Option<TranscriptFeedback>,
    ) -> Result<SessionRecord> {
        let transaction = self.connection.unchecked_transaction()?;
        let mut record = read_record(&transaction, id)?;
        record.transcript_feedback = feedback;
        write_record(&transaction, &record)?;
        transaction.commit()?;
        Ok(record)
    }

    /// Approve edited prose for cleanup training, never as a verbatim ASR label.
    pub fn approve_correction(&self, id: &str, corrected_text: String) -> Result<SessionRecord> {
        self.approve_text(id, corrected_text, ReviewTargetKind::CleanupCorrection)
    }

    /// Approve what was actually spoken for ASR training, including disfluencies.
    pub fn approve_transcription(&self, id: &str, verbatim_text: String) -> Result<SessionRecord> {
        self.approve_text(id, verbatim_text, ReviewTargetKind::VerbatimTranscription)
    }

    fn approve_text(
        &self,
        id: &str,
        text: String,
        kind: ReviewTargetKind,
    ) -> Result<SessionRecord> {
        self.approve_text_with_feedback(id, text, kind, None)
    }

    /// Record an approval and optionally stamp the usefulness vote in the same
    /// transaction. `None` feedback preserves whatever vote is already stored.
    fn approve_text_with_feedback(
        &self,
        id: &str,
        text: String,
        kind: ReviewTargetKind,
        feedback: Option<TranscriptFeedback>,
    ) -> Result<SessionRecord> {
        validate_text(&text, true)?;
        let transaction = self.connection.unchecked_transaction()?;
        let mut record = read_record(&transaction, id)?;
        match kind {
            ReviewTargetKind::VerbatimTranscription => {
                record.verified_transcript = Some(text.clone())
            }
            ReviewTargetKind::CleanupCorrection => record.corrected_transcript = Some(text.clone()),
        }
        if let Some(vote) = feedback {
            record.transcript_feedback = Some(vote);
        }
        record.review_history.push(ReviewRevision {
            id: Uuid::new_v4().to_string(),
            target_kind: kind,
            text,
            timestamp: now(),
        });
        record.review_status = ReviewStatus::Approved;
        write_record(&transaction, &record)?;
        transaction.commit()?;
        Ok(record)
    }

    /// A History/Dictate like on the polished text. Approves what the user
    /// actually verified: a verbatim ASR label only when the polished text is
    /// byte-identical to what was heard, otherwise a cleanup correction. The
    /// like itself teaches nothing; only an edited correction learns.
    pub fn approve_like(&self, id: &str, polished_text: String) -> Result<SessionRecord> {
        let transaction = self.connection.unchecked_transaction()?;
        let record = read_record(&transaction, id)?;
        let kind = if record.raw_transcript.as_deref() == Some(polished_text.as_str()) {
            ReviewTargetKind::VerbatimTranscription
        } else {
            ReviewTargetKind::CleanupCorrection
        };
        drop(record);
        drop(transaction);
        self.approve_text_with_feedback(id, polished_text, kind, Some(TranscriptFeedback::Like))
    }

    /// Read the stored audio bytes for in-app playback, with the same
    /// path-integrity checks as export. Playback is a convenience read; it
    /// never modifies the record, so missing audio is reported as `None`.
    pub fn session_audio_bytes(&self, id: &str) -> Result<Option<(String, Vec<u8>)>> {
        let transaction = self.connection.unchecked_transaction()?;
        let record = read_record(&transaction, id)?;
        let path = self.checked_audio_path(&record)?;
        drop(transaction);
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(DatasetError::Io(error)),
        };
        if bytes.len() as u64 > MAX_AUDIO_BYTES {
            return Err(DatasetError::InvalidAudio);
        }
        let extension = Path::new(&record.audio_relative_path)
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        Ok(Some((extension, bytes)))
    }

    /// Undo a review: drop the approved labels and the vote, back to Pending.
    /// No history entry is appended because there is no retract revision kind;
    /// the taught vocabulary entries this approval may have produced stay and
    /// are removed explicitly by the user.
    pub fn retract_approval(&self, id: &str) -> Result<SessionRecord> {
        let transaction = self.connection.unchecked_transaction()?;
        let mut record = read_record(&transaction, id)?;
        record.verified_transcript = None;
        record.corrected_transcript = None;
        record.transcript_feedback = None;
        record.review_status = ReviewStatus::Pending;
        write_record(&transaction, &record)?;
        transaction.commit()?;
        Ok(record)
    }

    /// Save a user's correction from a History/Dictate review and teach the
    /// automatic-learning loop from that explicit edit. Learning runs before
    /// the approval commit so a retry after a crash reuses the same revision
    /// and the edit is processed exactly once. Approval itself is always
    /// recorded, even when learning is off.
    pub fn approve_correction_and_learn(
        &self,
        id: &str,
        corrected_text: String,
    ) -> Result<(SessionRecord, LearningOutcome)> {
        validate_text(&corrected_text, true)?;
        let profile = self.personalization()?;
        let transaction = self.connection.unchecked_transaction()?;
        let record = read_record(&transaction, id)?;
        drop(transaction);
        let before = record
            .corrected_transcript
            .clone()
            .or(record.personalized_transcript.clone())
            .or(record.cleaned_transcript.clone())
            .or(record.raw_transcript.clone())
            .unwrap_or_default();
        let revision = record.review_history.len() as u64;
        let context = LearningContext {
            result_id: record.id.clone(),
            source_key: "history-review".into(),
            episode_id: record.id.clone(),
            language: record.config_snapshot.language,
            project: record.personalization_project.clone(),
            model_id: record.config_snapshot.asr_name().into(),
            learning_epoch: profile.learning_epoch,
            learning_enabled: profile.automatic_learning,
        };
        let outcome = if profile.automatic_learning {
            self.learn_from_edit(&context, revision, &before, &corrected_text)?
        } else {
            LearningOutcome {
                status: "Correction saved for this session. Learning is off.".into(),
                ..Default::default()
            }
        };
        let saved = self.approve_text_with_feedback(
            id,
            corrected_text,
            ReviewTargetKind::CleanupCorrection,
            None,
        )?;
        Ok((saved, outcome))
    }

    /// Export JSONL records, metadata, and original audio. Call with `true` for
    /// training-eligible reviewed data; `false` archives all records. ASR and
    /// cleanup targets are explicit and never inferred from machine predictions.
    pub fn export_dataset(&self, destination: &Path, approved_only: bool) -> Result<ExportSummary> {
        let destination = prepare_destination(destination, &self.root)?;
        let parent = destination
            .parent()
            .ok_or(DatasetError::InvalidExportDestination)?;
        let staging = parent.join(format!(".vesper-export-{}", Uuid::new_v4()));
        fs::create_dir(&staging)?;
        let result = (|| {
            fs::create_dir(staging.join("audio"))?;
            let records: Vec<_> = self
                .list_sessions()?
                .into_iter()
                .filter(|record| !approved_only || !task_eligibility(record).is_empty())
                .collect();
            let manifest_file = File::create(staging.join("manifest.jsonl"))?;
            let mut manifest = BufWriter::new(manifest_file);
            for record in &records {
                let source = self.checked_audio_path(record)?;
                let mut input = File::open(source)?;
                let mut output = File::create(staging.join(&record.audio_relative_path))?;
                let (bytes, hash) = copy_with_hash(&mut input, &mut output)?;
                if bytes != record.audio_bytes || hash != record.audio_sha256 {
                    return Err(DatasetError::AudioIntegrity(record.id.clone()));
                }
                output.sync_all()?;
                let tasks = task_eligibility(record);
                let asr_target = tasks
                    .contains(&"asr")
                    .then_some(record.verified_transcript.as_deref())
                    .flatten();
                let cleanup_input = tasks
                    .contains(&"cleanup")
                    .then_some(record.raw_transcript.as_deref())
                    .flatten();
                let cleanup_target = tasks
                    .contains(&"cleanup")
                    .then_some(record.corrected_transcript.as_deref())
                    .flatten();
                let mut entry = serde_json::to_value(record)?;
                entry["taskEligibility"] = json!(tasks);
                entry["asrTarget"] = json!(asr_target);
                entry["cleanupInput"] = json!(cleanup_input);
                entry["cleanupTarget"] = json!(cleanup_target);
                serde_json::to_writer(&mut manifest, &entry)?;
                manifest.write_all(b"\n")?;
            }
            manifest.flush()?;
            manifest.get_ref().sync_all()?;
            let metadata = json!({
                "schemaVersion": 1,
                "createdAt": now(),
                "approvedOnly": approved_only,
                "sessionCount": records.len(),
                "manifest": "manifest.jsonl",
                "audioDirectory": "audio",
                "labelPolicy": {
                    "asrTarget": "human-verified verbatim speech only",
                    "cleanupTarget": "human-approved edited text paired with original rawTranscript as cleanupInput",
                    "pending": "not eligible for training; machine output is never a ground-truth label"
                }
            });
            let mut metadata_file = File::create(staging.join("metadata.json"))?;
            serde_json::to_writer_pretty(&mut metadata_file, &metadata)?;
            metadata_file.write_all(b"\n")?;
            metadata_file.sync_all()?;
            sync_directory(&staging.join("audio"))?;
            sync_directory(&staging)?;
            // Recheck immediately before publishing; never merge into an existing export.
            if destination.exists() {
                if !destination.is_dir() || fs::read_dir(&destination)?.next().is_some() {
                    return Err(DatasetError::InvalidExportDestination);
                }
                fs::remove_dir(&destination)?;
            }
            fs::rename(&staging, &destination)?;
            sync_directory(parent)?;
            Ok(ExportSummary {
                destination: destination.to_string_lossy().into_owned(),
                session_count: records.len(),
                approved_only,
                manifest_path: destination
                    .join("manifest.jsonl")
                    .to_string_lossy()
                    .into_owned(),
                metadata_path: destination
                    .join("metadata.json")
                    .to_string_lossy()
                    .into_owned(),
            })
        })();
        if result.is_err() {
            let _ = fs::remove_dir_all(&staging);
        }
        result
    }

    fn checked_audio_path(&self, record: &SessionRecord) -> Result<PathBuf> {
        let relative = Path::new(&record.audio_relative_path);
        let mut components = relative.components();
        let safe = matches!(components.next(), Some(Component::Normal(part)) if part == "audio")
            && matches!(components.next(), Some(Component::Normal(_)))
            && components.next().is_none()
            && Uuid::parse_str(&record.id).is_ok()
            && relative.file_stem().and_then(|value| value.to_str()) == Some(record.id.as_str())
            && relative
                .extension()
                .and_then(|value| value.to_str())
                .is_some_and(|extension| {
                    !extension.is_empty()
                        && extension
                            .chars()
                            .all(|character| character.is_ascii_alphanumeric())
                });
        if !safe {
            return Err(DatasetError::UnsafeAudioPath(
                record.audio_relative_path.clone(),
            ));
        }
        let path = self.root.join(relative).canonicalize()?;
        if !path.starts_with(self.root.join("audio")) || !path.is_file() {
            return Err(DatasetError::UnsafeAudioPath(
                record.audio_relative_path.clone(),
            ));
        }
        Ok(path)
    }
}

fn read_record(connection: &Connection, id: &str) -> Result<SessionRecord> {
    let value: Option<String> = connection
        .query_row(
            "SELECT record_json FROM sessions WHERE id = ?1",
            [id],
            |row| row.get(0),
        )
        .optional()?;
    Ok(serde_json::from_str(&value.ok_or_else(|| {
        DatasetError::SessionNotFound(id.into())
    })?)?)
}

fn write_record(connection: &Connection, record: &SessionRecord) -> Result<()> {
    let review = match record.review_status {
        ReviewStatus::Pending => "pending",
        ReviewStatus::Approved => "approved",
    };
    connection.execute(
        "UPDATE sessions SET review_status = ?1, record_json = ?2 WHERE id = ?3",
        params![review, serde_json::to_string(record)?, record.id],
    )?;
    Ok(())
}

fn safe_extension(path: &Path) -> String {
    let extension: String = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .take(12)
        .map(|character| character.to_ascii_lowercase())
        .collect();
    if extension.is_empty() {
        "bin".into()
    } else {
        extension
    }
}

fn validate_text(text: &str, require_nonblank: bool) -> Result<()> {
    if text.chars().count() > MAX_TRANSCRIPT_CHARS {
        return Err(DatasetError::TranscriptTooLong);
    }
    if require_nonblank && text.trim().is_empty() {
        return Err(DatasetError::EmptyCorrection);
    }
    Ok(())
}

fn task_eligibility(record: &SessionRecord) -> Vec<&'static str> {
    let mut tasks = Vec::new();
    if record.review_status == ReviewStatus::Approved {
        if record
            .verified_transcript
            .as_ref()
            .is_some_and(|text| !text.trim().is_empty())
        {
            tasks.push("asr");
        }
        if record.raw_transcript.is_some()
            && record
                .corrected_transcript
                .as_ref()
                .is_some_and(|text| !text.trim().is_empty())
        {
            tasks.push("cleanup");
        }
    }
    tasks
}

fn copy_with_hash(input: &mut File, output: &mut File) -> Result<(u64, String)> {
    let mut hasher = Sha256::new();
    let mut bytes = 0_u64;
    let mut buffer = [0_u8; 65_536];
    loop {
        let count = input.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        if bytes + count as u64 > MAX_AUDIO_BYTES {
            return Err(DatasetError::AudioTooLarge);
        }
        output.write_all(&buffer[..count])?;
        hasher.update(&buffer[..count]);
        bytes += count as u64;
    }
    Ok((bytes, format!("{:x}", hasher.finalize())))
}

fn prepare_destination(destination: &Path, root: &Path) -> Result<PathBuf> {
    if destination.exists()
        && (!destination.is_dir() || fs::read_dir(destination)?.next().is_some())
    {
        return Err(DatasetError::InvalidExportDestination);
    }
    let name = destination
        .file_name()
        .ok_or(DatasetError::InvalidExportDestination)?;
    let parent = destination
        .parent()
        .filter(|value| !value.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let destination = parent.canonicalize()?.join(name);
    if destination.starts_with(root)
        || destination
            .symlink_metadata()
            .is_ok_and(|metadata| metadata.file_type().is_symlink())
    {
        return Err(DatasetError::InvalidExportDestination);
    }
    Ok(destination)
}

fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}
fn empty_object() -> Value {
    json!({})
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<()> {
    File::open(path)?.sync_all()?;
    Ok(())
}
#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speaker_predictions_survive_reopen_and_export_without_becoming_training_labels() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("dataset");
        let store = DatasetStore::open(root.clone()).unwrap();
        let record = import(&store, dir.path(), "speakers.wav");
        let timeline = vesper_core::SpeakerTimeline::from_native(
            3.0,
            &[(0.0, 2.0, 1), (1.0, 3.0, 2)],
            "model",
            "runtime",
            "cpu",
        )
        .unwrap();
        assert!(matches!(
            store.complete_speaker_timeline(&record.id, timeline.clone()),
            Err(DatasetError::MissingRawTranscript)
        ));
        let raw = store
            .set_transcripts(&record.id, "Meeting words".into(), None, empty_object())
            .unwrap();
        let mut legacy = serde_json::to_value(&raw).unwrap();
        legacy.as_object_mut().unwrap().remove("speakerTimeline");
        assert!(serde_json::from_value::<SessionRecord>(legacy)
            .unwrap()
            .speaker_timeline
            .is_none());
        let saved = store
            .complete_speaker_timeline(&record.id, timeline.clone())
            .unwrap();
        assert_eq!(saved.raw_transcript, raw.raw_transcript);
        assert_eq!(saved.review_status, ReviewStatus::Pending);
        assert!(task_eligibility(&saved).is_empty());
        assert!(store
            .complete_speaker_timeline(&record.id, timeline.clone())
            .is_err());
        drop(store);
        let store = DatasetStore::open(root).unwrap();
        assert_eq!(
            store.list_sessions().unwrap()[0].speaker_timeline,
            Some(timeline)
        );
        let export = dir.path().join("export");
        store.export_dataset(&export, false).unwrap();
        let row: Value =
            serde_json::from_str(&fs::read_to_string(export.join("manifest.jsonl")).unwrap())
                .unwrap();
        assert_eq!(
            row["speakerTimeline"]["segments"].as_array().unwrap().len(),
            2
        );
        assert!(row["asrTarget"].is_null());
    }

    #[test]
    fn like_approves_polished_kind_by_what_was_heard_and_undo_returns_to_pending() {
        let directory = tempfile::tempdir().unwrap();
        let store = DatasetStore::open(directory.path().join("dataset")).unwrap();
        let record = import(&store, directory.path(), "vote.wav");
        store
            .set_transcripts(&record.id, "Original words.".into(), None, empty_object())
            .unwrap();
        store
            .complete_cleanup(&record.id, "Polished words.".into())
            .unwrap();

        // Polished differs from raw: like approves a cleanup correction, not speech.
        let liked = store
            .approve_like(&record.id, "Polished words.".into())
            .unwrap();
        assert_eq!(liked.review_status, ReviewStatus::Approved);
        assert_eq!(
            liked.corrected_transcript.as_deref(),
            Some("Polished words.")
        );
        assert_eq!(liked.verified_transcript, None);
        assert_eq!(liked.transcript_feedback, Some(TranscriptFeedback::Like));
        assert!(matches!(
            liked.review_history.last().unwrap().target_kind,
            ReviewTargetKind::CleanupCorrection
        ));

        let undone = store.retract_approval(&record.id).unwrap();
        assert_eq!(undone.review_status, ReviewStatus::Pending);
        assert_eq!(undone.verified_transcript, None);
        assert_eq!(undone.corrected_transcript, None);
        assert_eq!(undone.transcript_feedback, None);

        // Polished identical to raw: like approves a verbatim ASR label.
        let verified = store
            .approve_like(&record.id, "Original words.".into())
            .unwrap();
        assert_eq!(
            verified.verified_transcript.as_deref(),
            Some("Original words.")
        );
        assert!(matches!(
            verified.review_history.last().unwrap().target_kind,
            ReviewTargetKind::VerbatimTranscription
        ));
    }

    #[test]
    fn correction_save_teaches_learning_loop_from_history_review() {
        let directory = tempfile::tempdir().unwrap();
        let store = DatasetStore::open(directory.path().join("dataset")).unwrap();
        let record = import(&store, directory.path(), "history-learn.wav");
        store
            .set_transcripts(
                &record.id,
                "Open read me for deployment".into(),
                None,
                empty_object(),
            )
            .unwrap();
        let profile = store
            .save_personalization(vesper_core::PersonalizationProfile {
                automatic_learning: true,
                active_project: record.personalization_project.clone(),
                ..Default::default()
            })
            .unwrap();

        let (saved, outcome) = store
            .approve_correction_and_learn(&record.id, "Open README.md for deployment".into())
            .unwrap();
        assert_eq!(saved.review_status, ReviewStatus::Approved);
        assert_eq!(
            saved.corrected_transcript.as_deref(),
            Some("Open README.md for deployment")
        );
        assert_eq!(outcome.profile_revision, profile.revision);
        assert_eq!(
            store.automatic_learning_state().unwrap().candidates.len(),
            1
        );
        assert_eq!(
            store
                .dictation_personalization_snapshot()
                .unwrap()
                .learning
                .candidates[0]
                .support[0]
                .teaching_terms,
            ["deployment"]
        );
    }

    #[test]
    fn correction_save_records_approval_when_learning_is_off() {
        let directory = tempfile::tempdir().unwrap();
        let store = DatasetStore::open(directory.path().join("dataset")).unwrap();
        let record = import(&store, directory.path(), "history-off.wav");
        store
            .set_transcripts(&record.id, "Original words.".into(), None, empty_object())
            .unwrap();

        let (saved, outcome) = store
            .approve_correction_and_learn(&record.id, "Edited words.".into())
            .unwrap();
        assert_eq!(saved.review_status, ReviewStatus::Approved);
        assert!(outcome.status.contains("Learning is off"));
        assert!(store
            .automatic_learning_state()
            .unwrap()
            .candidates
            .is_empty());
    }

    fn import(store: &DatasetStore, directory: &Path, name: &str) -> SessionRecord {
        let source = directory.join(name);
        fs::write(&source, b"original audio fixture\0\x01\x02").unwrap();
        store.import_audio(&source, &AppConfig::default()).unwrap()
    }

    #[test]
    fn feedback_persists_without_approving_labels_and_legacy_records_remain_readable() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("dataset");
        let store = DatasetStore::open(root.clone()).unwrap();
        let record = import(&store, directory.path(), "feedback.wav");
        let original = store
            .set_transcripts(&record.id, "Original words.".into(), None, empty_object())
            .unwrap();
        let mut legacy = serde_json::to_value(&original).unwrap();
        legacy.as_object_mut().unwrap().remove("transcriptFeedback");
        let restored: SessionRecord = serde_json::from_value(legacy).unwrap();
        assert_eq!(restored.transcript_feedback, None);
        assert!(serde_json::from_value::<TranscriptFeedback>(json!("unknown")).is_err());

        let liked = store
            .set_transcript_feedback(&record.id, Some(TranscriptFeedback::Like))
            .unwrap();
        assert_eq!(liked.review_status, ReviewStatus::Pending);
        assert!(task_eligibility(&liked).is_empty());
        assert_eq!(liked.raw_transcript, original.raw_transcript);
        assert_eq!(liked.review_history, original.review_history);
        drop(store);
        let store = DatasetStore::open(root).unwrap();
        assert_eq!(store.list_sessions().unwrap()[0], liked);

        store
            .complete_cleanup(&record.id, "Polished words.".into())
            .unwrap();
        let reviewed = store
            .approve_transcription(&record.id, "Original words.".into())
            .unwrap();
        assert_eq!(reviewed.transcript_feedback, Some(TranscriptFeedback::Like));
        let disliked = store
            .set_transcript_feedback(&record.id, Some(TranscriptFeedback::Dislike))
            .unwrap();
        assert_eq!(disliked.review_status, ReviewStatus::Approved);
        assert_eq!(disliked.review_history, reviewed.review_history);
        assert_eq!(task_eligibility(&disliked), task_eligibility(&reviewed));
        let cleared = store.set_transcript_feedback(&record.id, None).unwrap();
        assert_eq!(cleared.transcript_feedback, None);
        assert_eq!(cleared.cleaned_transcript, reviewed.cleaned_transcript);
        assert!(matches!(
            store.set_transcript_feedback("missing", None),
            Err(DatasetError::SessionNotFound(_))
        ));
    }

    #[test]
    fn raw_result_survives_cleanup_failure_and_cleanup_is_append_only() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("dataset");
        let store = DatasetStore::open(root.clone()).unwrap();
        let record = import(&store, directory.path(), "capture.wav");
        assert!(matches!(
            store.complete_cleanup(&record.id, "too early".into()),
            Err(DatasetError::MissingRawTranscript)
        ));
        store
            .set_transcripts(
                &record.id,
                "do not pay 50".into(),
                None,
                json!({"asr":"pinned"}),
            )
            .unwrap();
        store
            .record_pipeline_event(&record.id, "cleanup_error", "cancelled")
            .unwrap();
        drop(store);
        let store = DatasetStore::open(root).unwrap();
        let record = store.list_sessions().unwrap().remove(0);
        assert_eq!(record.raw_transcript.as_deref(), Some("do not pay 50"));
        assert!(record.cleaned_transcript.is_none());
        assert_eq!(record.pipeline_events[0].stage, "cleanup_error");
        let updated = store
            .complete_cleanup(&record.id, "Do not pay 50.".into())
            .unwrap();
        assert_eq!(updated.raw_transcript, record.raw_transcript);
        assert_eq!(updated.review_status, ReviewStatus::Pending);
        assert!(store
            .complete_cleanup(&record.id, "Pay 500.".into())
            .is_err());
    }

    #[test]
    fn imports_original_bytes_without_fabricating_transcripts_and_survives_reopen() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("dataset");
        let store = DatasetStore::open(root.clone()).unwrap();
        let record = import(&store, directory.path(), "voice take.WAV");
        assert_eq!(record.original_filename, "voice take.WAV");
        assert!(record.audio_relative_path.ends_with(".wav"));
        let bytes = fs::read(root.join(&record.audio_relative_path)).unwrap();
        assert_eq!(bytes, b"original audio fixture\0\x01\x02");
        assert_eq!(record.audio_sha256, format!("{:x}", Sha256::digest(&bytes)));
        assert_eq!(record.audio_bytes, bytes.len() as u64);
        assert!(record.raw_transcript.is_none());
        assert!(record.cleaned_transcript.is_none());
        assert!(record.corrected_transcript.is_none());
        assert!(record.verified_transcript.is_none());
        assert!(record.review_history.is_empty());
        assert_eq!(record.review_status, ReviewStatus::Pending);
        drop(store);
        assert_eq!(
            DatasetStore::open(root).unwrap().list_sessions().unwrap(),
            vec![record]
        );
    }

    #[test]
    fn failed_import_leaves_no_record_or_audio() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("dataset");
        let store = DatasetStore::open(root.clone()).unwrap();
        let source = directory.path().join("empty.wav");
        fs::write(&source, []).unwrap();
        assert!(matches!(
            store.import_audio(&source, &AppConfig::default()),
            Err(DatasetError::InvalidAudio)
        ));
        assert!(store.list_sessions().unwrap().is_empty());
        assert_eq!(fs::read_dir(root.join("audio")).unwrap().count(), 0);
        let source = directory.path().join("valid.wav");
        fs::write(&source, b"original bytes").unwrap();
        store.connection.execute_batch(
            "CREATE TRIGGER reject_import BEFORE INSERT ON sessions BEGIN SELECT RAISE(ABORT, 'forced test failure'); END;"
        ).unwrap();
        assert!(matches!(
            store.import_audio(&source, &AppConfig::default()),
            Err(DatasetError::Database(_))
        ));
        assert!(store.list_sessions().unwrap().is_empty());
        assert_eq!(fs::read_dir(root.join("audio")).unwrap().count(), 0);
    }

    #[test]
    fn export_uses_only_approved_corrections_as_labels() {
        let directory = tempfile::tempdir().unwrap();
        let store = DatasetStore::open(directory.path().join("dataset")).unwrap();
        let approved = import(&store, directory.path(), "approved.wav");
        let pending = import(&store, directory.path(), "pending.wav");
        let correction_without_prediction = import(&store, directory.path(), "correction.wav");
        store
            .approve_correction(
                &correction_without_prediction.id,
                "Edited prose without a raw prediction.".into(),
            )
            .unwrap();
        store
            .set_transcripts(
                &approved.id,
                "fifteen".into(),
                Some("Fifty.".into()),
                json!({"asrRevision": "fixture-v1"}),
            )
            .unwrap();
        store
            .approve_correction(&approved.id, "Fifteen.".into())
            .unwrap();
        store
            .set_transcripts(
                &pending.id,
                "do not send".into(),
                Some("Send.".into()),
                json!({}),
            )
            .unwrap();
        let destination = directory.path().join("training");
        let summary = store.export_dataset(&destination, true).unwrap();
        assert_eq!(summary.session_count, 1);
        let line = fs::read_to_string(destination.join("manifest.jsonl")).unwrap();
        let entry: Value = serde_json::from_str(line.trim()).unwrap();
        assert_eq!(entry["cleanupTarget"], "Fifteen.");
        assert_eq!(entry["cleanupInput"], "fifteen");
        assert!(entry["asrTarget"].is_null());
        assert_eq!(entry["taskEligibility"], json!(["cleanup"]));
        assert_eq!(entry["cleanedTranscript"], "Fifty.");
        assert!(entry.get("targetText").is_none());
        assert_eq!(entry["modelVersions"]["asrRevision"], "fixture-v1");
        assert!(destination.join(&approved.audio_relative_path).exists());
        assert!(!destination.join(&pending.audio_relative_path).exists());
        assert!(!destination
            .join(&correction_without_prediction.audio_relative_path)
            .exists());
        store
            .approve_transcription(&approved.id, "um fifteen".into())
            .unwrap();
        let archive = directory.path().join("archive");
        assert_eq!(
            store.export_dataset(&archive, false).unwrap().session_count,
            3
        );
        let entries: Vec<Value> = fs::read_to_string(archive.join("manifest.jsonl"))
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        let pending_entry = entries
            .iter()
            .find(|entry| entry["id"] == pending.id)
            .unwrap();
        assert!(pending_entry["asrTarget"].is_null());
        assert!(pending_entry["cleanupTarget"].is_null());
        assert!(pending_entry["cleanupInput"].is_null());
        assert_eq!(pending_entry["taskEligibility"], json!([]));
        let approved_entry = entries
            .iter()
            .find(|entry| entry["id"] == approved.id)
            .unwrap();
        assert_eq!(approved_entry["asrTarget"], "um fifteen");
        assert_eq!(approved_entry["cleanupTarget"], "Fifteen.");
        assert_eq!(approved_entry["taskEligibility"], json!(["asr", "cleanup"]));
        let unpaired = entries
            .iter()
            .find(|entry| entry["id"] == correction_without_prediction.id)
            .unwrap();
        assert!(unpaired["asrTarget"].is_null());
        assert!(unpaired["cleanupTarget"].is_null());
        assert_eq!(unpaired["taskEligibility"], json!([]));
    }

    #[test]
    fn rejects_stored_path_traversal_and_keeps_failed_export_unpublished() {
        let directory = tempfile::tempdir().unwrap();
        let store = DatasetStore::open(directory.path().join("dataset")).unwrap();
        let mut record = import(&store, directory.path(), "source.wav");
        record.audio_relative_path = "../source.wav".into();
        write_record(&store.connection, &record).unwrap();
        let destination = directory.path().join("export");
        assert!(matches!(
            store.export_dataset(&destination, false),
            Err(DatasetError::UnsafeAudioPath(_))
        ));
        assert!(!destination.exists());
        assert!(!fs::read_dir(directory.path()).unwrap().any(|entry| entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".vesper-export-")));
    }

    #[test]
    fn detects_modified_audio_and_refuses_to_overwrite_existing_exports() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("dataset");
        let store = DatasetStore::open(root.clone()).unwrap();
        let record = import(&store, directory.path(), "source.wav");
        let destination = directory.path().join("export");
        fs::create_dir(&destination).unwrap();
        fs::write(destination.join("keep.txt"), b"keep").unwrap();
        assert!(matches!(
            store.export_dataset(&destination, false),
            Err(DatasetError::InvalidExportDestination)
        ));
        assert_eq!(fs::read(destination.join("keep.txt")).unwrap(), b"keep");
        fs::write(root.join(record.audio_relative_path), b"changed").unwrap();
        assert!(matches!(
            store.export_dataset(&directory.path().join("other-export"), false),
            Err(DatasetError::AudioIntegrity(_))
        ));
    }

    #[test]
    fn collection_preference_is_enforced_and_original_predictions_and_reviews_are_retained() {
        let directory = tempfile::tempdir().unwrap();
        let store = DatasetStore::open(directory.path().join("dataset")).unwrap();
        let record = import(&store, directory.path(), "source.wav");
        let disabled = AppConfig {
            collect_training_data: false,
            ..AppConfig::default()
        };
        assert!(matches!(
            store.import_audio(&directory.path().join("source.wav"), &disabled),
            Err(DatasetError::CollectionDisabled)
        ));
        store
            .set_transcripts(
                &record.id,
                "original prediction".into(),
                Some("Original prediction.".into()),
                json!({"revision":"first"}),
            )
            .unwrap();
        store
            .approve_correction(&record.id, "Reviewed.".into())
            .unwrap();
        assert!(matches!(
            store.set_transcripts(&record.id, "new machine output".into(), None, json!({})),
            Err(DatasetError::PredictionsAlreadyStored(_))
        ));
        let updated = store
            .approve_correction(&record.id, "Reviewed again.".into())
            .unwrap();
        assert_eq!(updated.review_status, ReviewStatus::Approved);
        assert_eq!(
            updated.raw_transcript.as_deref(),
            Some("original prediction")
        );
        assert_eq!(updated.model_versions, json!({"revision":"first"}));
        assert_eq!(
            updated.corrected_transcript.as_deref(),
            Some("Reviewed again.")
        );
        assert_eq!(updated.review_history.len(), 2);
        assert_eq!(updated.review_history[0].text, "Reviewed.");
        assert_eq!(updated.review_history[1].text, "Reviewed again.");
        assert_eq!(
            store
                .export_dataset(&directory.path().join("training"), true)
                .unwrap()
                .session_count,
            1
        );
    }

    #[test]
    fn enforces_text_and_import_limits() {
        let directory = tempfile::tempdir().unwrap();
        let store = DatasetStore::open(directory.path().join("dataset")).unwrap();
        let record = import(&store, directory.path(), "source.wav");
        assert!(matches!(
            store.approve_transcription(&record.id, " ".into()),
            Err(DatasetError::EmptyCorrection)
        ));
        assert!(matches!(
            store.approve_correction(&record.id, "x".repeat(MAX_TRANSCRIPT_CHARS + 1)),
            Err(DatasetError::TranscriptTooLong)
        ));
        assert!(matches!(
            store.set_transcripts(&record.id, "valid".into(), None, json!([])),
            Err(DatasetError::InvalidModelVersions)
        ));
        let nonaudio = directory.path().join("notes.txt");
        fs::write(&nonaudio, b"notes").unwrap();
        assert!(matches!(
            store.import_audio(&nonaudio, &AppConfig::default()),
            Err(DatasetError::InvalidAudioExtension)
        ));
        let oversized = directory.path().join("oversized.wav");
        File::create(&oversized)
            .unwrap()
            .set_len(MAX_AUDIO_BYTES + 1)
            .unwrap();
        assert!(matches!(
            store.import_audio(&oversized, &AppConfig::default()),
            Err(DatasetError::AudioTooLarge)
        ));
        assert_eq!(store.list_sessions().unwrap().len(), 1);
    }
}
