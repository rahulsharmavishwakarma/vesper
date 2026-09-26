use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
};

use crate::diagnostics::{Diagnostics, Stage};
use serde::Serialize;
use vesper_audio::MonoPcm;
use vesper_core::{AppConfig, Language};
use vesper_data::{DatasetStore, SessionRecord};

#[cfg(test)]
#[path = "runtime_learning_tests.rs"]
mod learning_tests;

#[cfg(test)]
#[path = "runtime_learning_qualification_tests.rs"]
mod learning_qualification_tests;

#[path = "runtime_policy.rs"]
mod policy;
#[path = "runtime_semantic.rs"]
mod semantic;

#[cfg(test)]
#[path = "runtime_diarization_tests.rs"]
mod diarization_tests;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PipelineResult {
    pub result_id: String,
    pub raw_transcript: String,
    pub speaker_timeline: Option<vesper_core::SpeakerTimeline>,
    pub cleaned_transcript: Option<String>,
    pub personalized_transcript: Option<String>,
    pub personalization_evidence: Option<vesper_core::MemoryEvidence>,
    pub draft_transcript: Option<String>,
    pub draft_revision: u64,
    pub learning_outcome: Option<vesper_core::LearningOutcome>,
    #[serde(skip)]
    pub learning_context: vesper_core::LearningContext,
    pub session: Option<SessionRecord>,
    pub warnings: Vec<String>,
    pub duration_seconds: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    fn editable_result(profile: &vesper_core::PersonalizationProfile, id: &str) -> PipelineResult {
        PipelineResult {
            result_id: id.into(),
            raw_transcript: "Open read me in the repo".into(),
            cleaned_transcript: None,
            speaker_timeline: None,
            personalized_transcript: None,
            personalization_evidence: None,
            draft_transcript: None,
            draft_revision: 0,
            learning_outcome: None,
            learning_context: vesper_core::LearningContext {
                result_id: id.into(),
                source_key: format!("audio-{id}"),
                episode_id: id.into(),
                language: Language::English,
                project: "repo".into(),
                model_id: "fixture".into(),
                learning_epoch: profile.learning_epoch,
                learning_enabled: profile.automatic_learning,
            },
            session: None,
            warnings: vec![],
            duration_seconds: 1.0,
        }
    }
    #[test]
    fn automatic_draft_edits_are_current_revision_checked_and_recoverable_without_audio() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("data");
        let store = DatasetStore::open(root.clone()).unwrap();
        let profile = store
            .save_personalization(vesper_core::PersonalizationProfile {
                automatic_learning: true,
                active_project: "repo".into(),
                ..Default::default()
            })
            .unwrap();
        let runtime = Runtime::default();
        runtime.publish(editable_result(&profile, "current"));
        assert!(runtime
            .edit_latest_draft(root.clone(), "old", 0, "wrong".into())
            .is_err());
        let result = runtime
            .edit_latest_draft(
                root.clone(),
                "current",
                0,
                "Open README.md in the repo".into(),
            )
            .unwrap();
        assert_eq!(result.draft_revision, 1);
        assert_eq!(result.learning_outcome.unwrap().activated, ["README.md"]);
        assert_eq!(
            runtime
                .latest()
                .unwrap()
                .unwrap()
                .draft_transcript
                .as_deref(),
            Some("Open README.md in the repo")
        );
        assert!(runtime
            .edit_latest_draft(root.clone(), "current", 0, "stale".into())
            .is_err());
        assert!(store.list_sessions().unwrap().is_empty());
        let undone = runtime
            .undo_latest_learning("current", root.clone())
            .unwrap();
        assert_eq!(
            undone.draft_transcript.as_deref(),
            Some("Open README.md in the repo")
        );
        assert!(!store.personalization().unwrap().vocabulary[0].enabled);
        runtime.begin("recording", "new capture").unwrap();
        assert!(runtime
            .edit_latest_draft(root, "current", 1, "late".into())
            .is_err());
    }
    #[test]
    fn automatic_learning_off_creates_no_store_for_an_unsaved_draft() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("not-created");
        let runtime = Runtime::default();
        runtime.publish(editable_result(&Default::default(), "current"));
        let result = runtime
            .edit_latest_draft(root.clone(), "current", 0, String::new())
            .unwrap();
        assert_eq!(result.draft_transcript.as_deref(), Some(""));
        assert!(!root.exists());
        let wire = serde_json::to_string(&result).unwrap();
        assert!(!wire.contains("sourceKey"));
        assert!(!wire.contains("audio-current"));
    }
    #[test]
    fn automatic_retry_grouping_is_ephemeral_scoped_and_does_not_classify_repetition_as_repair() {
        let runtime = Runtime::default();
        let profile = vesper_core::PersonalizationProfile {
            automatic_learning: true,
            ..Default::default()
        };
        let mut first = editable_result(&profile, "first").learning_context;
        let mut second = editable_result(&profile, "second").learning_context;
        let raw = "Open the atlas deployment";
        assert_eq!(
            runtime.connect_learning_attempt(&mut first, raw, true),
            None
        );
        assert_eq!(
            runtime.connect_learning_attempt(&mut second, raw, true),
            Some(vesper_core::RetryRelation::Uncertain)
        );
        assert_eq!(second.episode_id, first.episode_id);
        let mut third = editable_result(&profile, "third").learning_context;
        third.project = "other".into();
        runtime.connect_learning_attempt(&mut third, raw, true);
        assert_eq!(third.episode_id, "third");
        runtime.clear_learning_context();
        assert!(runtime.state.lock().unwrap().recent_learning.is_none());
        runtime.connect_learning_attempt(&mut first, raw, false);
        assert!(runtime.state.lock().unwrap().recent_learning.is_none());
    }
    #[test]
    fn undo_without_collection_survives_polling_and_rejects_a_stale_result() {
        let runtime = Runtime::default();
        runtime.publish(PipelineResult {
            result_id: "current".into(),
            raw_transcript: "cube control".into(),
            cleaned_transcript: None,
            speaker_timeline: None,
            personalized_transcript: Some("kubectl".into()),
            personalization_evidence: Some(Default::default()),
            draft_transcript: None,
            draft_revision: 0,
            learning_outcome: None,
            learning_context: Default::default(),
            session: None,
            warnings: vec![],
            duration_seconds: 1.0,
        });
        assert!(runtime.undo_unsaved_personalization("older").is_err());
        assert!(runtime
            .latest()
            .unwrap()
            .unwrap()
            .personalized_transcript
            .is_some());
        runtime.undo_unsaved_personalization("current").unwrap();
        let latest = runtime.latest().unwrap().unwrap();
        assert!(latest.personalized_transcript.is_none());
        assert_eq!(latest.raw_transcript, "cube control");
    }
    #[test]
    fn raw_preview_is_read_only_and_scoped_to_the_active_operation() {
        let runtime = Runtime::default();
        let result = editable_result(&Default::default(), "raw-result");
        let preview = || {
            serde_json::to_value(
                runtime
                    .status(
                        vesper_models::Installation {
                            ready: true,
                            model_directory: "models".into(),
                            files: vec![],
                            supported: true,
                            message: String::new(),
                        },
                        0.0,
                        0,
                    )
                    .unwrap(),
            )
            .unwrap()["transcriptPreview"]
                .clone()
        };
        runtime.publish_transcript_preview("first", &result);
        assert!(preview().is_null());
        runtime.begin("transcribing", "Working").unwrap();
        runtime.set_operation_id("first");
        runtime.publish(result.clone());
        // A failed native stream does not invalidate a completed fallback result.
        runtime.invalidate_partial("first");
        runtime.publish_transcript_preview("first", &result);
        assert_eq!(preview()["operationId"], "first");
        assert_eq!(preview()["resultId"], result.result_id);
        assert_eq!(preview()["rawTranscript"], result.raw_transcript);
        let root = tempfile::tempdir().unwrap();
        assert!(runtime
            .edit_latest_draft(
                root.path().into(),
                &result.result_id,
                0,
                "early edit".into()
            )
            .is_err());
        runtime.progress("rewriting", "Polishing");
        let mut cleaned = result.clone();
        cleaned.cleaned_transcript = Some("Open README in the repo.".into());
        runtime.publish(cleaned);
        assert_eq!(preview()["rawTranscript"], result.raw_transcript);
        runtime.cancel_and_take_recording().unwrap();
        assert!(preview().is_null());
        runtime.publish_transcript_preview("first", &result);
        assert!(preview().is_null());
        runtime.finish(&Err::<(), _>("cancelled".into()));
        assert_eq!(
            runtime.latest().unwrap().unwrap().raw_transcript,
            result.raw_transcript
        );
        runtime.begin("transcribing", "New file").unwrap();
        runtime.set_operation_id("second");
        runtime.publish_transcript_preview("first", &result);
        assert!(preview().is_null());
        runtime.publish_transcript_preview("second", &result);
        assert_eq!(preview()["operationId"], "second");
        runtime.finish(&Ok::<(), String>(()));
        assert!(preview().is_null());
        runtime.publish_transcript_preview("second", &result);
        assert!(preview().is_null());
    }

    #[test]
    fn segmented_progress_is_scoped_and_cannot_overwrite_cancellation_or_completion() {
        use vesper_inference::TranscriptionProgress;
        let runtime = Runtime::default();
        let sample = TranscriptionProgress {
            processed_samples: 40,
            total_samples: 100,
        };
        runtime.begin("transcribing", "Starting").unwrap();
        runtime.start_transcription("old", "Speech model");
        runtime.transcription_progress("old", sample);
        assert_eq!(runtime.state.lock().unwrap().message, "Transcribing · 40%");
        assert!(runtime.state.lock().unwrap().latest.is_none());
        runtime.cancel_and_take_recording().unwrap();
        runtime.transcription_progress("old", sample);
        runtime.start_transcription("old", "Speech model");
        assert_eq!(runtime.state.lock().unwrap().stage, "cancelling");
        runtime.finish(&Err::<(), _>("cancelled".into()));
        runtime.begin("transcribing", "New recording").unwrap();
        runtime.start_transcription("new", "Speech model");
        runtime.transcription_progress("old", sample);
        assert_eq!(
            runtime.state.lock().unwrap().message,
            "Transcribing with Speech model…"
        );
        runtime.transcription_progress("new", sample);
        assert_eq!(runtime.state.lock().unwrap().message, "Transcribing · 40%");
        runtime.progress("rewriting", "Polishing");
        runtime.transcription_progress("new", sample);
        assert_eq!(runtime.state.lock().unwrap().message, "Polishing");
        runtime.finish(&Ok::<(), String>(()));
        runtime.transcription_progress("new", sample);
        assert_eq!(runtime.state.lock().unwrap().message, "Ready");
    }

    #[test]
    fn failed_stream_invalidates_visible_partial_even_if_completed_fallback_fails() {
        for explicit_failure in [true, false] {
            let directory = tempfile::tempdir().unwrap();
            let runtime = Arc::new(Runtime::default());
            let context = WorkContext {
                operation_id: "failed-live".into(),
                config: AppConfig {
                    cleanup_enabled: false,
                    collect_training_data: false,
                    ..Default::default()
                },
                model_root: directory.path().join("missing-models"),
                dataset_root: directory.path().join("data"),
                runtime: Arc::clone(&runtime),
                inference: Arc::new(vesper_inference::InferenceManager::new().unwrap()),
                streaming_result: Arc::default(),
                personalization: Arc::default(),
            };
            runtime.begin("recording", "Listening").unwrap();
            runtime.set_operation_id(&context.operation_id);
            runtime.set_recording(context.clone());
            let mut partial = vesper_inference::StreamingTranscript {
                operation_id: context.operation_id.clone(),
                sequence: 1,
                committed_text: "invalid draft".into(),
                provisional_text: String::new(),
                audio_processed_seconds: 1.0,
                emitted_at_unix_ms: 1,
                is_final: false,
            };
            assert!(runtime.publish_partial(partial.clone()));
            runtime.invalidate_partial("an-older-operation");
            assert!(runtime.state.lock().unwrap().partial.is_some());
            let (sender, receiver) = mpsc::sync_channel(1);
            context.set_streaming_result(receiver);
            if explicit_failure {
                sender
                    .send(Err("injected frontend failure".into()))
                    .unwrap();
            }
            drop(sender);
            // Exercise the real fallback. Missing ASR files must not restore the draft.
            let audio = vesper_audio::CapturedAudio::new(vec![0.0; 1600], 16000, 1).unwrap();
            assert!(context.process_capture(audio).is_err());
            {
                let state = runtime.state.lock().unwrap();
                assert!(state.partial.is_none());
                assert_eq!(
                    state.partial_invalidated_for.as_deref(),
                    Some("failed-live")
                );
                assert_eq!(
                    state.transcription_operation.as_deref(),
                    Some("failed-live")
                );
            }
            partial.sequence += 1;
            assert!(!runtime.publish_partial(partial));
            runtime.finish(&Ok::<(), String>(()));
            runtime.begin("recording", "Next recording").unwrap();
            assert!(runtime
                .state
                .lock()
                .unwrap()
                .partial_invalidated_for
                .is_none());
        }
    }

    #[test]
    fn model_removal_shares_the_recording_and_installation_operation_boundary() {
        let runtime = Runtime::default();
        for active in ["recording", "transcribing", "rewriting", "installing"] {
            runtime.begin(active, "Working").unwrap();
            assert!(runtime.begin("removing", "Removing files").is_err());
            runtime.finish(&Ok::<(), String>(()));
        }
        runtime.begin("removing", "Removing files").unwrap();
        assert!(runtime.begin("recording", "Starting").is_err());
        assert!(runtime.begin("installing", "Downloading").is_err());
        runtime.finish(&Err::<(), String>("File permission error".into()));
        assert!(runtime.begin("recording", "Retry").is_ok());
    }

    #[test]
    fn download_progress_is_scoped_to_the_operation_and_cancellation_is_sticky() {
        let runtime = Runtime::default();
        let sample = vesper_models::InstallProgress {
            phase: vesper_models::InstallPhase::Downloading,
            file_name: "synthetic.gguf".into(),
            file_index: 1,
            file_count: 1,
            completed_bytes: 50,
            total_bytes: 100,
        };
        runtime.begin("installing", "Preparing").unwrap();
        runtime.begin_download("old", Some("model".into()), None);
        runtime.download_progress("old", &sample);
        assert_eq!(
            runtime
                .state
                .lock()
                .unwrap()
                .download
                .as_ref()
                .unwrap()
                .progress
                .as_ref()
                .unwrap()
                .completed_bytes,
            50
        );
        runtime.cancel_and_take_recording().unwrap();
        runtime.download_progress("old", &sample);
        assert_eq!(runtime.state.lock().unwrap().stage, "cancelling");
        runtime.finish(&Err::<(), _>("cancelled".into()));
        assert!(runtime.state.lock().unwrap().download.is_none());
        runtime.begin("installing", "Preparing").unwrap();
        runtime.begin_download("new", None, None);
        runtime.download_progress("old", &sample);
        assert!(runtime
            .state
            .lock()
            .unwrap()
            .download
            .as_ref()
            .unwrap()
            .progress
            .is_none());
        runtime.download_progress("new", &sample);
        runtime.finish(&Ok::<(), String>(()));
        assert!(runtime.state.lock().unwrap().download.is_none());
    }

    #[test]
    fn empty_transcript_guidance_distinguishes_quiet_audio_and_auto_language() {
        let quiet = MonoPcm::new(vec![0.0001; 16_000], 16_000).unwrap();
        assert!(no_speech_message(&quiet, Language::English).contains("very quiet"));

        let audible = MonoPcm::new(vec![0.02; 16_000], 16_000).unwrap();
        assert!(
            no_speech_message(&audible, Language::Auto).contains("automatic language detection")
        );
        assert!(no_speech_message(&audible, Language::English).contains("selected microphone"));
    }

    #[test]
    fn microphone_level_is_only_reported_while_recording_and_is_finite() {
        let runtime = Runtime::default();
        let installation = || vesper_models::Installation {
            ready: true,
            model_directory: "models".into(),
            files: vec![],
            supported: true,
            message: String::new(),
        };
        assert_eq!(
            runtime
                .status(installation(), 0.75, 10)
                .unwrap()
                .audio_level,
            0.0
        );
        runtime.begin("recording", "Recording").unwrap();
        assert_eq!(
            runtime
                .status(installation(), 0.75, 10)
                .unwrap()
                .audio_level,
            0.75
        );
        assert_eq!(
            runtime.status(installation(), 2.0, 10).unwrap().audio_level,
            1.0
        );
        for value in [f32::NAN, f32::INFINITY, -1.0] {
            assert_eq!(
                runtime
                    .status(installation(), value, 10)
                    .unwrap()
                    .audio_level,
                0.0
            );
        }
        let serialized =
            serde_json::to_value(runtime.status(installation(), 0.5, 10).unwrap()).unwrap();
        assert_eq!(serialized["audioLevel"], 0.5);
        runtime.progress("transcribing", "Processing");
        assert_eq!(
            runtime
                .status(installation(), 0.75, 10)
                .unwrap()
                .audio_level,
            0.0
        );
        runtime.finish(&Ok::<(), String>(()));
        assert_eq!(
            runtime
                .status(installation(), 0.75, 10)
                .unwrap()
                .audio_level,
            0.0
        );
    }
    #[test]
    fn operations_exclude_each_other_and_idle_cancel_is_a_noop() {
        let runtime = Runtime::default();
        assert!(!runtime.cancel_and_take_recording().unwrap().0);
        assert!(!runtime.cancelled.load(Ordering::Relaxed));
        runtime.begin("recording", "recording").unwrap();
        assert!(runtime.begin("installing", "installing").is_err());
        assert!(runtime.require_idle().is_err());
        assert!(runtime.cancel_and_take_recording().unwrap().0);
        runtime.finish(&Ok::<(), String>(()));
        assert!(!runtime.is_busy());
        runtime.begin("transcribing", "transcribing").unwrap();
        assert!(!runtime.cancelled.load(Ordering::Relaxed));
    }

    #[test]
    fn automatic_learning_runs_for_fresh_dictation_without_settings_or_audio_collection() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("dataset");
        let runtime = Arc::new(Runtime::default());
        let inference = Arc::new(vesper_inference::InferenceManager::new().unwrap());
        let before = "Deploy kubernets to the cluster";
        let after = "Deploy kubernetes to the cluster";
        for attempt in 0..3 {
            let mut config = AppConfig {
                cleanup_enabled: false,
                collect_training_data: false,
                language: Language::English,
                ..Default::default()
            };
            // Third recording uses the other recognizer, without changing memory ownership.
            config.models.asr = if attempt == 2 {
                vesper_core::AsrModel::Nemotron35AsrStreaming06b
            } else {
                vesper_core::AsrModel::CohereTranscribe
            };
            let context = WorkContext {
                operation_id: format!("attempt-{attempt}"),
                config,
                model_root: directory.path().join("missing-models"),
                dataset_root: root.clone(),
                runtime: Arc::clone(&runtime),
                inference: Arc::clone(&inference),
                streaming_result: Arc::default(),
                personalization: Arc::default(),
            };
            let audio = MonoPcm::new(vec![0.2 + attempt as f32 * 0.1; 1600], 16000).unwrap();
            let mut result = context
                .complete_raw(&audio, &mut None, before.into())
                .unwrap();
            assert!(result.learning_context.learning_enabled);
            context.observe_result(&mut result, false);
            runtime.publish(result.clone());
            if attempt < 2 {
                runtime
                    .edit_latest_draft(root.clone(), &result.result_id, 0, after.into())
                    .unwrap();
            } else {
                assert_eq!(result.personalized_transcript.as_deref(), Some(after));
            }
        }
        let store = DatasetStore::open(root.clone()).unwrap();
        assert_eq!(store.personalization().unwrap().vocabulary.len(), 1);
        assert!(!store.automatic_learning_state().unwrap().events.is_empty());
        assert!(store.list_sessions().unwrap().is_empty());
        assert_eq!(std::fs::read_dir(root.join("audio")).unwrap().count(), 0);
    }

    #[test]
    fn final_memory_survives_model_selection_without_cleanup_or_installed_weights() {
        use vesper_core::{AsrModel, PersonalizationProfile, VocabularyEntry};
        let directory = tempfile::tempdir().unwrap();
        let store = DatasetStore::open(directory.path().join("dataset")).unwrap();
        let profile = store
            .save_dictation_personalization(PersonalizationProfile {
                vocabulary: vec![VocabularyEntry {
                    id: "term".into(),
                    phrase: "kubectl".into(),
                    aliases: vec!["cube control".into()],
                    enabled: true,
                    ..Default::default()
                }],
                ..Default::default()
            })
            .unwrap();
        let audio = MonoPcm::new(vec![0.25; 1600], 16000).unwrap();
        for model in [
            AsrModel::CohereTranscribe,
            AsrModel::Nemotron35AsrStreaming06b,
        ] {
            let mut config = AppConfig {
                cleanup_enabled: false,
                collect_training_data: false,
                ..Default::default()
            };
            config.models.asr = model;
            let context = WorkContext {
                operation_id: model.id().into(),
                config,
                model_root: directory.path().join("missing-models"),
                dataset_root: directory.path().join("dataset"),
                runtime: Arc::new(Runtime::default()),
                inference: Arc::new(vesper_inference::InferenceManager::new().unwrap()),
                streaming_result: Arc::new(Mutex::new(None)),
                personalization: Arc::default(),
            };
            context.runtime.begin("transcribing", "Working").unwrap();
            context.runtime.set_operation_id(&context.operation_id);
            let result = context
                .complete_raw(&audio, &mut None, "run cube control".into())
                .unwrap();
            // The common completion path previews raw text before personalization,
            // for both completed-only and native-streaming model selections.
            let preview = context
                .runtime
                .state
                .lock()
                .unwrap()
                .transcript_preview
                .clone()
                .unwrap();
            assert_eq!(preview.raw_transcript, "run cube control");
            assert_eq!(preview.operation_id, context.operation_id);
            assert_eq!(preview.result_id, result.result_id);
            context.runtime.finish(&Ok::<(), String>(()));
            assert_eq!(result.raw_transcript, "run cube control");
            assert!(result.cleaned_transcript.is_none());
            assert_eq!(
                result.personalized_transcript.as_deref(),
                Some("run kubectl")
            );
            assert_eq!(
                result.personalization_evidence.unwrap().profile_revision,
                profile.revision
            );
            assert_eq!(store.personalization().unwrap(), profile);
            assert!(store.list_sessions().unwrap().is_empty());
        }
    }

    #[test]
    fn closing_preserves_audio_without_inference_and_collection_off_leaves_no_files() {
        let directory = tempfile::tempdir().unwrap();
        let mut context = WorkContext {
            operation_id: "test".into(),
            config: AppConfig::default(),
            model_root: directory.path().join("missing-models"),
            dataset_root: directory.path().join("dataset"),
            runtime: Arc::new(Runtime::default()),
            inference: Arc::new(vesper_inference::InferenceManager::new().unwrap()),
            streaming_result: Arc::new(Mutex::new(None)),
            personalization: Arc::default(),
        };
        let make_audio = || vesper_audio::CapturedAudio::new(vec![0.25; 1600], 16000, 1).unwrap();
        context.preserve_on_close(make_audio()).unwrap();
        let records = DatasetStore::open(context.dataset_root.clone())
            .unwrap()
            .list_sessions()
            .unwrap();
        assert_eq!(records.len(), 1);
        assert!(records[0].raw_transcript.is_none());
        assert_eq!(records[0].pipeline_events[0].stage, "interrupted");
        context.config.collect_training_data = false;
        context.dataset_root = directory.path().join("disabled");
        context.preserve_on_close(make_audio()).unwrap();
        assert!(!context.dataset_root.exists());
    }

    #[test]
    fn stale_watcher_cannot_claim_a_new_recording() {
        let runtime = Arc::new(Runtime::default());
        let context = |id: &str| WorkContext {
            operation_id: id.into(),
            config: AppConfig::default(),
            model_root: PathBuf::new(),
            dataset_root: PathBuf::new(),
            runtime: Arc::clone(&runtime),
            inference: Arc::new(vesper_inference::InferenceManager::new().unwrap()),
            streaming_result: Arc::new(Mutex::new(None)),
            personalization: Arc::default(),
        };
        runtime.begin("recording", "old").unwrap();
        runtime.set_recording(context("old"));
        assert!(runtime.take_recording_for("old").is_some());
        runtime.finish(&Ok::<(), String>(()));
        runtime.begin("recording", "new").unwrap();
        runtime.set_recording(context("new"));
        assert!(runtime.take_recording_for("old").is_none());
        assert!(runtime.is_recording("new"));
        let (_, recording) = runtime.cancel_and_take_recording().unwrap();
        assert_eq!(recording.unwrap().operation_id, "new");
        assert!(runtime.is_busy());
        runtime.finish(&Ok::<(), String>(()));
        assert!(!runtime.is_busy());
        runtime.closing.store(true, Ordering::Relaxed);
        assert!(runtime.begin("recording", "blocked during close").is_err());
    }

    #[test]
    fn capture_archive_failure_keeps_a_recovery_wav() {
        let directory = tempfile::tempdir().unwrap();
        let blocked_root = directory.path().join("not-a-directory");
        std::fs::write(&blocked_root, "block dataset creation").unwrap();
        let context = WorkContext {
            operation_id: "recovery".into(),
            config: AppConfig::default(),
            model_root: PathBuf::new(),
            dataset_root: blocked_root,
            runtime: Arc::new(Runtime::default()),
            inference: Arc::new(vesper_inference::InferenceManager::new().unwrap()),
            streaming_result: Arc::new(Mutex::new(None)),
            personalization: Arc::default(),
        };
        let audio = vesper_audio::CapturedAudio::new(vec![0.25; 1600], 16000, 1).unwrap();
        let error = context.archive_capture(&audio).unwrap_err();
        let path = PathBuf::from(
            error
                .split("Recording retained for recovery at ")
                .nth(1)
                .unwrap(),
        );
        assert!(path.is_file());
        assert_eq!(
            vesper_audio::decode_file(&path, 1).unwrap().samples().len(),
            1600
        );
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    #[ignore = "installed Qwen/Cohere weights and verified FLEURS fixture; no recording/downloads"]
    fn new_offline_asr_pipeline_preserves_audio_and_provenance() {
        use std::time::Instant;
        use vesper_core::{AsrModel, Language};
        let models = PathBuf::from(vesper_core::environment::var_os("VESPER_MODEL_DIR").unwrap());
        let source =
            PathBuf::from(vesper_core::environment::var_os("VESPER_ASR_FIXTURES").unwrap())
                .join("fleurs-en_us.wav");
        let source = vesper_audio::decode_file(source, 0).unwrap();
        let fixture = tempfile::tempdir().unwrap();
        let audio = fixture.path().join("fleurs-en_us-repeated-3.wav");
        vesper_audio::CapturedAudio::new(source.samples().repeat(3), 16000, 1)
            .unwrap()
            .write_wav(&audio)
            .unwrap();
        for (model, artifact_count) in [(AsrModel::Qwen3Asr06b, 6), (AsrModel::CohereTranscribe, 4)]
        {
            let directory = tempfile::tempdir().unwrap();
            let mut config = AppConfig::default();
            config.models.asr = model;
            config.language = Language::English;
            config.cleanup_enabled = false;
            let context = WorkContext {
                operation_id: "public-offline-asr-qualification".into(),
                config,
                model_root: models.clone(),
                dataset_root: directory.path().join("dataset"),
                runtime: Arc::new(Runtime::default()),
                inference: Arc::new(vesper_inference::InferenceManager::new().unwrap()),
                streaming_result: Arc::new(Mutex::new(None)),
                personalization: Arc::default(),
            };
            context.runtime.begin("transcribing", "test").unwrap();
            let started = Instant::now();
            let result = context.process_file(&audio).unwrap();
            let file_seconds = started.elapsed().as_secs_f64();
            assert!(!result.raw_transcript.trim().is_empty());
            assert!(result.warnings.is_empty());
            assert!(result.cleaned_transcript.is_none());
            let session = result.session.as_ref().unwrap();
            assert_eq!(
                std::fs::read(&audio).unwrap(),
                std::fs::read(context.dataset_root.join(&session.audio_relative_path)).unwrap()
            );
            assert_eq!(
                session.raw_transcript.as_deref(),
                Some(result.raw_transcript.as_str())
            );
            assert_eq!(
                session.model_versions["artifacts"]
                    .as_array()
                    .unwrap()
                    .len(),
                artifact_count
            );
            let provenance = &session.model_versions["inference"]["asr"];
            assert_eq!(provenance["modelId"], model.id());
            assert_eq!(provenance["maxRecordingSeconds"], 0);
            assert_eq!(
                provenance["segmentation"]["policy"],
                "balanced-low-energy-v1"
            );
            assert!(result.duration_seconds > 10.0);
            assert!(provenance
                .to_string()
                .contains("11afbd009a7f8c08f4bcf2fc1b265d0df4670fbf"));
            assert!(session.verified_transcript.is_none());
            let mono = vesper_audio::decode_file(&audio, 0).unwrap();
            let captured =
                vesper_audio::CapturedAudio::new(mono.samples().to_vec(), 16000, 1).unwrap();
            // Calls the same post-stop path using public PCM, without microphone/UI time.
            let stopped = Instant::now();
            let capture_result = context.process_capture(captured).unwrap();
            let stop_seconds = stopped.elapsed().as_secs_f64();
            assert_eq!(capture_result.raw_transcript, result.raw_transcript);
            assert!(capture_result.warnings.is_empty());
            let captured_archive = context
                .dataset_root
                .join(&capture_result.session.as_ref().unwrap().audio_relative_path);
            assert_eq!(
                vesper_audio::decode_file(captured_archive, 0)
                    .unwrap()
                    .samples(),
                mono.samples()
            );
            let store = DatasetStore::open(context.dataset_root.clone()).unwrap();
            let export = store
                .export_dataset(&directory.path().join("export"), false)
                .unwrap();
            for line in std::fs::read_to_string(export.manifest_path)
                .unwrap()
                .lines()
            {
                let row: serde_json::Value = serde_json::from_str(line).unwrap();
                assert!(row["asrTarget"].is_null());
                assert!(row["cleanupTarget"].is_null());
            }
            println!(
                "{}",
                serde_json::json!({"model":model.id(), "filePipelineSeconds":file_seconds,
                "postStopPipelineSeconds":stop_seconds,"cleanupEnabled":false,"publicFixture":"fleurs-en_us.wav repeated 3 times", "audioSeconds":result.duration_seconds})
            );
        }
    }

    /// Explicit real-model smoke; no microphone, network, or machine-created labels.
    #[test]
    #[ignore = "requires downloaded model files and the public speech fixture"]
    fn real_file_pipeline_preserves_audio_predictions_and_export_provenance() {
        let directory = tempfile::tempdir().unwrap();
        let model_root = PathBuf::from(
            vesper_core::environment::var_os("VESPER_SMOKE_MODELS")
                .or_else(|| std::env::var_os("STT_SMOKE_MODELS"))
                .expect("VESPER_SMOKE_MODELS"),
        );
        let audio = PathBuf::from(
            vesper_core::environment::var_os("VESPER_SMOKE_AUDIO")
                .or_else(|| std::env::var_os("STT_SMOKE_AUDIO"))
                .expect("VESPER_SMOKE_AUDIO"),
        );
        let context = WorkContext {
            operation_id: "test".into(),
            config: AppConfig::default(),
            model_root,
            dataset_root: directory.path().join("dataset"),
            runtime: Arc::new(Runtime::default()),
            inference: Arc::new(vesper_inference::InferenceManager::new().unwrap()),
            streaming_result: Arc::new(Mutex::new(None)),
            personalization: Arc::default(),
        };
        context.runtime.begin("transcribing", "test").unwrap();
        context
            .runtime
            .diagnostics
            .begin(&context.operation_id, &context.config, false);
        let result = context.process_file(&audio).unwrap();
        context.runtime.finish(&Ok::<(), String>(()));
        let timings = context.runtime.diagnostics.snapshot().unwrap();
        assert_eq!(timings.outcome, "completed");
        assert_eq!(timings.speech_mode, "offline");
        assert!(timings.active_stage.is_none());
        assert!(timings.transcribe_ms.is_some_and(|ms| ms > 0.0));
        assert!(timings.cleanup_ms.is_some_and(|ms| ms > 0.0));
        assert!(timings.save_ms.is_some_and(|ms| ms > 0.0));
        println!("stage_timings={}", serde_json::to_string(&timings).unwrap());
        assert_eq!(
            result.raw_transcript,
            "Ask not what your country can do for you. Ask what you can do for your country."
        );
        assert!(
            result
                .cleaned_transcript
                .as_ref()
                .is_some_and(|text| !text.trim().is_empty()),
            "the complete pipeline smoke must exercise successful cleanup: {:?}",
            result.warnings
        );
        assert!(
            result.warnings.is_empty(),
            "pipeline warnings: {:?}",
            result.warnings
        );
        let session = result.session.as_ref().unwrap();
        assert_eq!(session.cleaned_transcript, result.cleaned_transcript);
        assert_eq!(
            std::fs::read(&audio).unwrap(),
            std::fs::read(context.dataset_root.join(&session.audio_relative_path)).unwrap()
        );
        assert_eq!(
            session.raw_transcript.as_deref(),
            Some(result.raw_transcript.as_str())
        );
        let artifacts = session.model_versions["artifacts"].as_array().unwrap();
        assert_eq!(artifacts.len(), 5);
        assert!(artifacts
            .iter()
            .any(|artifact| { artifact["modelId"] == "CohereLabs/cohere-transcribe-03-2026" }));
        assert_eq!(
            session.model_versions["inference"]["asr"]["modelId"],
            "CohereLabs/cohere-transcribe-03-2026"
        );
        assert_eq!(
            session.model_versions["inference"]["asr"]["runtime"],
            "sherpa-onnx"
        );
        assert!(session.verified_transcript.is_none());
        let store = DatasetStore::open(context.dataset_root.clone()).unwrap();
        let export = store
            .export_dataset(&directory.path().join("export"), false)
            .unwrap();
        let manifest = std::fs::read_to_string(export.manifest_path).unwrap();
        let value: serde_json::Value = serde_json::from_str(manifest.trim()).unwrap();
        assert!(value["asrTarget"].is_null());
        assert!(value["cleanupTarget"].is_null());
        println!("{}", serde_json::to_string_pretty(&result).unwrap());
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadStatus {
    pub operation_id: String,
    pub model_id: Option<String>,
    pub quantization: Option<vesper_core::RewriteQuantization>,
    pub progress: Option<vesper_models::InstallProgress>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptPreview {
    pub operation_id: String,
    pub result_id: String,
    pub raw_transcript: String,
}

#[derive(Default)]
struct State {
    operation_id: Option<String>,
    busy: bool,
    stage: String,
    message: String,
    latest: Option<PipelineResult>,
    recording: Option<WorkContext>,
    partial: Option<vesper_inference::StreamingTranscript>,
    partial_invalidated_for: Option<String>,
    transcript_preview: Option<TranscriptPreview>,
    download: Option<DownloadStatus>,
    transcription_operation: Option<String>,
    recent_learning: Option<RecentLearning>,
    semantic_request: Option<vesper_core::SemanticAnalysisRequest>,
    semantic_cancelled: Option<Arc<AtomicBool>>,
    semantic_earlier: Option<String>,
    policy_cancelled: Option<Arc<AtomicBool>>,
}

struct RecentLearning {
    context: vesper_core::LearningContext,
    raw: String,
    at: std::time::Instant,
}

#[derive(Default)]
pub struct Runtime {
    state: Mutex<State>,
    policy_worker: policy::PolicyWorker,
    pub diagnostics: Arc<Diagnostics>,
    pub cancelled: Arc<AtomicBool>,
    pub closing: AtomicBool,
    pub external_generation: std::sync::atomic::AtomicU64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeStatus {
    pub operation_id: Option<String>,
    pub busy: bool,
    pub stage: String,
    pub message: String,
    pub model_directory: String,
    pub models_ready: bool,
    pub max_recording_seconds: u32,
    pub audio_level: f32,
    pub partial: Option<vesper_inference::StreamingTranscript>,
    pub partial_invalidated_for: Option<String>,
    pub transcript_preview: Option<TranscriptPreview>,
    pub download: Option<DownloadStatus>,
}

impl Runtime {
    pub fn set_operation_id(&self, id: &str) {
        if let Ok(mut state) = self.state.lock() {
            state.operation_id = Some(id.into());
        }
    }
    pub fn operation_id(&self) -> Option<String> {
        self.state.lock().ok()?.operation_id.clone()
    }
    pub fn begin(&self, stage: &str, message: &str) -> Result<(), String> {
        let mut state = self.state.lock().map_err(|e| e.to_string())?;
        if self.closing.load(Ordering::Relaxed) {
            return Err("The application is closing.".into());
        }
        if state.busy {
            return Err("Another operation is running. Stop or cancel it first.".into());
        }
        state.cancel_semantic();
        self.external_generation.fetch_add(1, Ordering::AcqRel);
        self.cancelled.store(false, Ordering::Relaxed);
        state.busy = true;
        state.stage = stage.into();
        state.message = message.into();
        state.partial = None;
        state.partial_invalidated_for = None;
        state.transcript_preview = None;
        state.download = None;
        state.transcription_operation = None;
        Ok(())
    }
    pub fn begin_download(
        &self,
        operation_id: &str,
        model_id: Option<String>,
        quantization: Option<vesper_core::RewriteQuantization>,
    ) {
        if let Ok(mut state) = self.state.lock() {
            state.download = Some(DownloadStatus {
                operation_id: operation_id.into(),
                model_id,
                quantization,
                progress: None,
            });
        }
    }
    pub fn download_progress(&self, operation_id: &str, progress: &vesper_models::InstallProgress) {
        if let Ok(mut state) = self.state.lock() {
            if !state.busy || self.cancelled.load(Ordering::Relaxed) {
                return;
            }
            let Some(download) = state
                .download
                .as_mut()
                .filter(|download| download.operation_id == operation_id)
            else {
                return;
            };
            download.progress = Some(progress.clone());
            state.message = format!(
                "{}: {} / {} bytes",
                progress.file_name, progress.completed_bytes, progress.total_bytes
            );
        }
    }
    pub fn require_idle(&self) -> Result<(), String> {
        if self.state.lock().map_err(|e| e.to_string())?.busy {
            Err("Wait for the current operation before changing settings.".into())
        } else {
            Ok(())
        }
    }
    pub fn prepare_exit(&self) -> Result<(), String> {
        let mut state = self.state.lock().map_err(|error| error.to_string())?;
        if state.busy {
            return Err("Finish or cancel the current operation before quitting.".into());
        }
        // The state lock pairs with begin(), so no operation can start between
        // checking idle and marking shutdown. Hiding a window never calls this.
        state.cancel_semantic();
        self.closing.store(true, Ordering::Relaxed);
        Ok(())
    }
    pub fn cancel_and_take_recording(&self) -> Result<(bool, Option<WorkContext>), String> {
        let mut state = self.state.lock().map_err(|e| e.to_string())?;
        if !state.busy {
            return Ok((false, None));
        }
        self.cancelled.store(true, Ordering::Relaxed);
        state.transcript_preview = None;
        state.stage = "cancelling".into();
        state.message = "Cancelling after the current native operation finishes…".into();
        Ok((true, state.recording.take()))
    }
    pub fn progress(&self, stage: &str, message: impl Into<String>) {
        if let Ok(mut state) = self.state.lock() {
            state.stage = stage.into();
            state.message = message.into();
        }
    }
    fn start_transcription(&self, operation_id: &str, model: &str) {
        if let Ok(mut state) = self.state.lock() {
            if state.busy && !self.cancelled.load(Ordering::Relaxed) {
                state.transcription_operation = Some(operation_id.into());
                state.stage = "transcribing".into();
                state.message = format!("Transcribing with {model}…");
            }
        }
    }
    fn transcription_progress(
        &self,
        operation_id: &str,
        progress: vesper_inference::TranscriptionProgress,
    ) {
        if let Ok(mut state) = self.state.lock() {
            if !state.busy
                || state.transcription_operation.as_deref() != Some(operation_id)
                || state.stage != "transcribing"
                || self.cancelled.load(Ordering::Relaxed)
                || progress.total_samples == 0
            {
                return;
            }
            let percent = progress.processed_samples.min(progress.total_samples) as f64
                / progress.total_samples as f64
                * 100.0;
            state.message = format!("Transcribing · {:.0}%", percent.floor());
        }
    }
    pub fn finish<T>(&self, result: &Result<T, String>) {
        if let Ok(mut state) = self.state.lock() {
            self.diagnostics
                .finish(if self.cancelled.load(Ordering::Relaxed) {
                    "cancelled"
                } else if result.is_ok() {
                    "completed"
                } else {
                    "error"
                });
            state.busy = false;
            state.operation_id = None;
            state.recording = None;
            state.partial = None;
            state.partial_invalidated_for = None;
            state.transcript_preview = None;
            state.download = None;
            state.transcription_operation = None;
            if self.cancelled.load(Ordering::Relaxed) {
                state.stage = "cancelled".into();
                state.message =
                    "Operation cancelled. Any completed transcription remains available.".into();
            } else {
                match result {
                    Ok(_) => {
                        state.stage = "idle".into();
                        state.message = "Ready".into();
                    }
                    Err(error) => {
                        state.stage = "error".into();
                        state.message = error.clone();
                    }
                }
            }
        }
    }
    pub fn latest(&self) -> Result<Option<PipelineResult>, String> {
        Ok(self.state.lock().map_err(|e| e.to_string())?.latest.clone())
    }
    pub fn clear_learning_context(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.cancel_semantic();
            state.semantic_earlier = None;
            state.recent_learning = None;
        }
    }

    fn connect_learning_attempt(
        &self,
        context: &mut vesper_core::LearningContext,
        raw: &str,
        capture: bool,
    ) -> Option<vesper_core::RetryRelation> {
        let mut state = self.state.lock().ok()?;
        state.semantic_earlier = None;
        if !capture || !context.learning_enabled || raw.len() > vesper_core::MAX_LEARNING_TEXT_BYTES
        {
            state.recent_learning = None;
            return None;
        }
        let relation = state.recent_learning.as_ref().map(|prior| {
            let elapsed = prior.at.elapsed().as_millis().min(u64::MAX as u128) as u64;
            let same = prior.context.project == context.project
                && prior.context.language == context.language
                && prior.context.learning_epoch == context.learning_epoch;
            let relation = vesper_core::retry_relation(&prior.raw, raw, elapsed, same);
            if relation == vesper_core::RetryRelation::PossibleRepair
                || vesper_core::correlated_attempts(&prior.raw, raw, elapsed, same)
            {
                context.episode_id = prior.context.episode_id.clone();
            }
            relation
        });
        state.semantic_earlier = state
            .recent_learning
            .as_ref()
            .filter(|prior| {
                prior.at.elapsed() <= std::time::Duration::from_secs(120)
                    && prior.context.project == context.project
                    && prior.context.language == context.language
                    && prior.context.learning_epoch == context.learning_epoch
                    && prior.context.result_id != context.result_id
                    && prior.raw.len() <= 1024
            })
            .map(|prior| prior.raw.clone());
        state.recent_learning = Some(RecentLearning {
            context: context.clone(),
            raw: raw.into(),
            at: std::time::Instant::now(),
        });
        relation
    }

    /// Serializes edit validation, storage and publication against new operations.
    pub fn edit_latest_draft(
        &self,
        root: PathBuf,
        result_id: &str,
        revision: u64,
        text: String,
    ) -> Result<PipelineResult, String> {
        if text.chars().count() > vesper_data::MAX_TRANSCRIPT_CHARS {
            return Err("Draft is too long.".into());
        }
        let mut state = self.state.lock().map_err(|e| e.to_string())?;
        if state.busy {
            return Err("Wait for the current recording to finish before editing.".into());
        }
        let earlier = state.semantic_earlier.clone();
        let result = state
            .latest
            .as_mut()
            .filter(|r| r.result_id == result_id && r.draft_revision == revision)
            .ok_or("The result changed; refresh before editing.")?;
        let before = result
            .draft_transcript
            .as_deref()
            .or(result.personalized_transcript.as_deref())
            .or(result.cleaned_transcript.as_deref())
            .unwrap_or(&result.raw_transcript);
        if before == text {
            return Ok(result.clone());
        }
        let semantic_before = before.to_owned();
        let next_revision = revision.checked_add(1).ok_or("Draft revision exhausted.")?;
        // Snapshot before this edit teaches any memory; failure never blocks the draft.
        let policy_job = if result.learning_context.learning_enabled {
            DatasetStore::open(root.clone()).ok().and_then(|store| {
                store
                    .prepare_policy_shadow(
                        &result.learning_context,
                        next_revision,
                        result.session.as_ref().map(|s| s.id.clone()),
                        result
                            .cleaned_transcript
                            .as_deref()
                            .unwrap_or(&result.raw_transcript),
                        &text,
                    )
                    .ok()
                    .flatten()
            })
        } else {
            None
        };
        let outcome = if let Some(record) = &result.session {
            let (saved, outcome) = DatasetStore::open(root.clone())
                .and_then(|store| {
                    store.edit_session_draft(
                        &record.id,
                        revision,
                        before,
                        &text,
                        &result.learning_context,
                    )
                })
                .map_err(|e| e.to_string())?;
            result.session = Some(saved);
            outcome
        } else if result.learning_context.learning_enabled {
            DatasetStore::open(root.clone())
                .and_then(|store| {
                    store.learn_from_edit(&result.learning_context, revision, before, &text)
                })
                .map_err(|e| e.to_string())?
        } else {
            vesper_core::LearningOutcome {
                status: "Draft updated for this session. Learning is off.".into(),
                ..Default::default()
            }
        };
        result.draft_transcript = Some(text);
        result.draft_revision = next_revision;
        result.learning_outcome = Some(outcome);
        let result = result.clone();
        state.cancel_semantic();
        let request = vesper_core::SemanticAnalysisRequest {
            schema_version: vesper_core::SEMANTIC_SCHEMA_VERSION,
            origin: vesper_core::SemanticOrigin::InAppDraftEdit,
            session_id: result.session.as_ref().map(|s| s.id.clone()),
            context: result.learning_context.clone(),
            result_revision: next_revision,
            before: semantic_before,
            after: result.draft_transcript.clone().unwrap_or_default(),
            earlier,
            memories: vec![],
        };
        if request.validate().is_ok() {
            state.semantic_request = Some(request);
        }
        if let Some(job) = policy_job {
            if DatasetStore::open(root.clone())
                .and_then(|store| store.reserve_policy_shadow(&job))
                .unwrap_or(false)
            {
                let flag = Arc::new(AtomicBool::new(false));
                state.policy_cancelled = Some(Arc::clone(&flag));
                self.policy_worker.submit((root, job, flag));
            }
        }
        Ok(result)
    }
    pub fn publish(&self, result: PipelineResult) {
        if let Ok(mut state) = self.state.lock() {
            state.cancel_semantic();
            state.latest = Some(result);
        }
    }
    fn publish_transcript_preview(&self, operation_id: &str, result: &PipelineResult) {
        if let Ok(mut state) = self.state.lock() {
            if state.busy
                && state.operation_id.as_deref() == Some(operation_id)
                && !self.cancelled.load(Ordering::Relaxed)
            {
                state.transcript_preview = Some(TranscriptPreview {
                    operation_id: operation_id.into(),
                    result_id: result.result_id.clone(),
                    raw_transcript: result.raw_transcript.clone(),
                });
            }
        }
    }
    pub fn undo_personalization(&self, record: &SessionRecord) {
        if let Ok(mut state) = self.state.lock() {
            if let Some(latest) = state
                .latest
                .as_mut()
                .filter(|r| r.session.as_ref().is_some_and(|s| s.id == record.id))
            {
                latest.personalized_transcript = None;
                latest.session = Some(record.clone());
                state.cancel_semantic();
            }
        }
    }
    #[cfg(test)]
    pub fn undo_unsaved_personalization(&self, result_id: &str) -> Result<(), String> {
        let mut state = self.state.lock().map_err(|e| e.to_string())?;
        let result = state
            .latest
            .as_mut()
            .filter(|r| r.result_id == result_id && r.session.is_none())
            .ok_or("The unsaved result is no longer current.")?;
        result.personalized_transcript = None;
        Ok(())
    }

    pub fn undo_latest_learning(
        &self,
        result_id: &str,
        root: PathBuf,
    ) -> Result<PipelineResult, String> {
        let mut state = self.state.lock().map_err(|e| e.to_string())?;
        if state.busy {
            return Err("Wait for recording to finish before undoing.".into());
        }
        let result = state
            .latest
            .as_mut()
            .filter(|r| r.result_id == result_id)
            .ok_or("The result is no longer current.")?;
        let count = DatasetStore::open(root)
            .and_then(|s| s.undo_learning_for_result(result_id))
            .map_err(|e| e.to_string())?;
        result.learning_outcome = Some(vesper_core::LearningOutcome {
            suspended: count,
            status: "Learned spelling paused. Your draft is unchanged.".into(),
            ..Default::default()
        });
        let result = result.clone();
        state.cancel_semantic();
        Ok(result)
    }

    pub fn undo_unsaved_with_learning(&self, result_id: &str, root: PathBuf) -> Result<(), String> {
        let mut state = self.state.lock().map_err(|e| e.to_string())?;
        if state.busy {
            return Err("Wait for recording to finish before undoing.".into());
        }
        let result = state
            .latest
            .as_mut()
            .filter(|r| r.result_id == result_id && r.session.is_none())
            .ok_or("The unsaved result is no longer current.")?;
        if let Some(evidence) = &result.personalization_evidence {
            if root.join("sessions.sqlite3").exists() {
                DatasetStore::open(root.clone())
                    .and_then(|s| {
                        s.suspend_automatic_memories(
                            &evidence
                                .applied
                                .iter()
                                .map(|a| a.entry_id.clone())
                                .collect::<Vec<_>>(),
                        )
                    })
                    .map_err(|e| e.to_string())?;
            }
        }
        if root.join("sessions.sqlite3").exists() {
            DatasetStore::open(root)
                .and_then(|s| s.invalidate_semantic_result(result_id))
                .map_err(|e| e.to_string())?;
        }
        result.personalized_transcript = None;
        state.cancel_semantic();
        Ok(())
    }
    pub fn publish_partial(&self, partial: vesper_inference::StreamingTranscript) -> bool {
        if let Ok(mut state) = self.state.lock() {
            let active = state
                .recording
                .as_ref()
                .is_some_and(|context| context.operation_id == partial.operation_id);
            let newer = state
                .partial
                .as_ref()
                .is_none_or(|current| current.sequence < partial.sequence);
            if active
                && newer
                && state.partial_invalidated_for.as_deref() != Some(&partial.operation_id)
            {
                state.partial = Some(partial);
                return true;
            }
        }
        false
    }
    pub(crate) fn invalidate_partial(&self, operation_id: &str) {
        if let Ok(mut state) = self.state.lock() {
            if state.operation_id.as_deref() == Some(operation_id) {
                state.partial = None;
                state.partial_invalidated_for = Some(operation_id.into());
            }
        }
    }
    pub fn set_recording(&self, context: WorkContext) {
        if let Ok(mut state) = self.state.lock() {
            state.recording = Some(context);
        }
    }
    pub fn take_recording(&self) -> Option<WorkContext> {
        self.state.lock().ok()?.recording.take()
    }
    pub fn take_recording_for(&self, operation_id: &str) -> Option<WorkContext> {
        let mut state = self.state.lock().ok()?;
        if state
            .recording
            .as_ref()
            .is_some_and(|context| context.operation_id == operation_id)
        {
            state.recording.take()
        } else {
            None
        }
    }
    pub fn is_recording(&self, operation_id: &str) -> bool {
        self.state.lock().is_ok_and(|state| {
            state
                .recording
                .as_ref()
                .is_some_and(|context| context.operation_id == operation_id)
        })
    }
    #[cfg(test)]
    #[cfg(test)]
    pub fn is_busy(&self) -> bool {
        self.state.lock().is_ok_and(|state| state.busy)
    }
    pub fn activity(&self) -> Result<serde_json::Value, String> {
        let state = self.state.lock().map_err(|error| error.to_string())?;
        Ok(
            serde_json::json!({ "busy":state.busy, "stage":if state.stage.is_empty() { "idle" } else { &state.stage } }),
        )
    }
    pub fn status(
        &self,
        installation: vesper_models::Installation,
        audio_level: f32,
        max_recording_seconds: u32,
    ) -> Result<RuntimeStatus, String> {
        let state = self.state.lock().map_err(|e| e.to_string())?;
        Ok(RuntimeStatus {
            operation_id: state.operation_id.clone(),
            busy: state.busy,
            stage: if state.stage.is_empty() {
                "idle".into()
            } else {
                state.stage.clone()
            },
            message: state.message.clone(),
            model_directory: installation.model_directory,
            models_ready: installation.ready,
            max_recording_seconds,
            audio_level: if state.busy && state.stage == "recording" && audio_level.is_finite() {
                audio_level.clamp(0.0, 1.0)
            } else {
                0.0
            },
            partial: state.partial.clone(),
            partial_invalidated_for: state.partial_invalidated_for.clone(),
            transcript_preview: state.transcript_preview.clone(),
            download: state.download.clone(),
        })
    }
}

#[derive(Clone)]
pub struct WorkContext {
    pub operation_id: String,
    pub config: AppConfig,
    pub model_root: PathBuf,
    pub dataset_root: PathBuf,
    pub runtime: Arc<Runtime>,
    pub inference: Arc<vesper_inference::InferenceManager>,
    pub(crate) streaming_result: Arc<Mutex<Option<StreamingResultReceiver>>>,
    pub(crate) personalization:
        Arc<std::sync::OnceLock<Result<vesper_data::DictationPersonalizationSnapshot, String>>>,
}

type StreamingResultReceiver = mpsc::Receiver<Result<String, String>>;

impl WorkContext {
    pub(crate) fn personalization(&self) -> Result<&vesper_core::PersonalizationProfile, String> {
        self.personalization_snapshot()
            .map(|snapshot| &snapshot.profile)
    }

    fn personalization_snapshot(
        &self,
    ) -> Result<&vesper_data::DictationPersonalizationSnapshot, String> {
        self.personalization
            .get_or_init(|| {
                DatasetStore::open(self.dataset_root.clone())
                    .and_then(|store| store.dictation_personalization_snapshot())
                    .map_err(|e| e.to_string())
            })
            .as_ref()
            .map_err(Clone::clone)
    }

    pub(crate) fn set_streaming_result(&self, result: StreamingResultReceiver) {
        if let Ok(mut slot) = self.streaming_result.lock() {
            *slot = Some(result);
        }
    }

    fn take_streaming_result(&self) -> Option<StreamingResultReceiver> {
        self.streaming_result.lock().ok()?.take()
    }

    /// Synchronize cancellation before the shared operation flag can be reset by
    /// a later job. The stream's cancellation result itself is expected here.
    pub(crate) fn wait_for_stream_shutdown(&self) -> Result<(), String> {
        if let Some(result) = self.take_streaming_result() {
            let _ = result.recv().map_err(|error| error.to_string())?;
        }
        Ok(())
    }
    pub fn archive(&self, path: &Path) -> Result<Option<SessionRecord>, String> {
        if !self.config.collect_training_data {
            return Ok(None);
        }
        let _timer = self
            .runtime
            .diagnostics
            .measure(&self.operation_id, Stage::Save);
        self.archive_inner(path)
    }
    fn archive_inner(&self, path: &Path) -> Result<Option<SessionRecord>, String> {
        if !self.config.collect_training_data {
            return Ok(None);
        }
        DatasetStore::open(self.dataset_root.clone())
            .and_then(|store| store.import_audio(path, &self.config))
            .map(Some)
            .map_err(|e| e.to_string())
    }

    pub fn process_file(&self, path: &Path) -> Result<PipelineResult, String> {
        self.runtime
            .progress("transcribing", "Preserving and decoding audio…");
        let mut session = self.archive(path)?;
        let stored_path = session
            .as_ref()
            .map(|s| self.dataset_root.join(&s.audio_relative_path));
        let decoded = vesper_audio::decode_file(
            stored_path.as_deref().unwrap_or(path),
            self.config.effective_recording_seconds(),
        )
        .map_err(|e| e.to_string());
        match decoded {
            Ok(audio) => {
                let mut result = self.process(&audio, session)?;
                self.observe_result(&mut result, false);
                self.runtime.publish(result.clone());
                Ok(result)
            }
            Err(error) => {
                self.event(&mut session, "error", &error);
                Err(error)
            }
        }
    }

    pub fn process_capture(
        &self,
        audio: vesper_audio::CapturedAudio,
    ) -> Result<PipelineResult, String> {
        let warning = audio.capture_warning().map(str::to_owned);
        let mut session = self.archive_capture(&audio)?;
        if let Some(warning) = &warning {
            self.event(&mut session, "capture_warning", warning);
        }
        match audio.as_mono_16k().map_err(|e| e.to_string()) {
            Ok(mono) => {
                let mut result = if let Some(streaming) = self.take_streaming_result() {
                    let timer = self
                        .runtime
                        .diagnostics
                        .measure(&self.operation_id, Stage::Transcribe);
                    let finalized = streaming
                        .recv()
                        .unwrap_or_else(|error| Err(error.to_string()));
                    drop(timer);
                    match finalized {
                        Ok(raw) => self.process_raw(&mono, session, raw)?,
                        Err(error) if self.runtime.cancelled.load(Ordering::Relaxed) => {
                            return Err(error)
                        }
                        Err(error) => {
                            self.runtime.invalidate_partial(&self.operation_id);
                            self.runtime
                                .diagnostics
                                .mark_offline_fallback(&self.operation_id);
                            self.event(&mut session, "streaming_error", &error);
                            let mut fallback = self.process(&mono, session)?;
                            fallback.warnings.insert(
                                0,
                                format!(
                                    "Live transcription failed; completed-recording fallback was used: {error}"
                                ),
                            );
                            fallback
                        }
                    }
                } else {
                    self.process(&mono, session)?
                };
                if let Some(warning) = warning {
                    result.warnings.insert(0, warning);
                }
                self.observe_result(&mut result, true);
                self.runtime.publish(result.clone());
                Ok(result)
            }
            Err(error) => {
                self.event(&mut session, "error", &error);
                Err(error)
            }
        }
    }

    fn archive_capture(
        &self,
        audio: &vesper_audio::CapturedAudio,
    ) -> Result<Option<SessionRecord>, String> {
        if self.config.collect_training_data {
            let _timer = self
                .runtime
                .diagnostics
                .measure(&self.operation_id, Stage::Save);
            let directory = tempfile::tempdir().map_err(|e| e.to_string())?;
            let path = directory
                .path()
                .join(format!("recording-{}.wav", uuid::Uuid::new_v4()));
            audio.write_wav(&path).map_err(|e| e.to_string())?;
            match self.archive_inner(&path) {
                Ok(session) => Ok(session),
                Err(error) => {
                    let _recovery = directory.keep();
                    Err(format!(
                        "{error}. Recording retained for recovery at {}",
                        path.display()
                    ))
                }
            }
        } else {
            Ok(None)
        }
    }

    pub fn preserve_on_close(&self, audio: vesper_audio::CapturedAudio) -> Result<(), String> {
        let mut session = self.archive_capture(&audio)?;
        self.event(
            &mut session,
            "interrupted",
            "Recording preserved when the window was closed, before transcription.",
        );
        if let Some(warning) = audio.capture_warning() {
            self.event(&mut session, "capture_warning", warning);
        }
        Ok(())
    }

    fn event(&self, session: &mut Option<SessionRecord>, stage: &str, message: &str) {
        if let Some(record) = session.as_ref() {
            let _timer = self
                .runtime
                .diagnostics
                .measure(&self.operation_id, Stage::Save);
            if let Ok(updated) = DatasetStore::open(self.dataset_root.clone())
                .and_then(|store| store.record_pipeline_event(&record.id, stage, message))
            {
                *session = Some(updated);
            }
        }
    }

    pub fn process(
        &self,
        audio: &MonoPcm,
        mut session: Option<SessionRecord>,
    ) -> Result<PipelineResult, String> {
        let result = self.process_inner(audio, &mut session);
        if let Err(error) = &result {
            self.event(&mut session, "error", error);
        }
        result
    }

    fn process_inner(
        &self,
        audio: &MonoPcm,
        session: &mut Option<SessionRecord>,
    ) -> Result<PipelineResult, String> {
        self.runtime
            .start_transcription(&self.operation_id, self.config.asr_name());
        let timer = self
            .runtime
            .diagnostics
            .measure(&self.operation_id, Stage::Transcribe);
        let raw = self
            .inference
            .transcribe_personalized(
                audio,
                &self.config,
                &self.model_root,
                &self.operation_id,
                Arc::clone(&self.runtime.cancelled),
                {
                    let runtime = Arc::clone(&self.runtime);
                    let operation_id = self.operation_id.clone();
                    Arc::new(move |progress| {
                        runtime.transcription_progress(&operation_id, progress)
                    })
                },
                self.personalization()?,
            )
            .map_err(|e| e.to_string())?;
        drop(timer);
        self.complete_raw(audio, session, raw)
    }

    fn process_raw(
        &self,
        audio: &MonoPcm,
        mut session: Option<SessionRecord>,
        raw: String,
    ) -> Result<PipelineResult, String> {
        let result = self.complete_raw(audio, &mut session, raw);
        if let Err(error) = &result {
            self.event(&mut session, "error", error);
        }
        result
    }

    fn complete_raw(
        &self,
        audio: &MonoPcm,
        session: &mut Option<SessionRecord>,
        raw: String,
    ) -> Result<PipelineResult, String> {
        if raw.trim().is_empty() {
            return Err(no_speech_message(audio, self.config.language));
        }
        let mut result = PipelineResult {
            result_id: uuid::Uuid::new_v4().to_string(),
            raw_transcript: raw.clone(),
            cleaned_transcript: None,
            speaker_timeline: None,
            personalized_transcript: None,
            personalization_evidence: None,
            draft_transcript: None,
            draft_revision: 0,
            learning_outcome: None,
            learning_context: Default::default(),
            session: session.clone(),
            warnings: vec![],
            duration_seconds: audio.duration_seconds(),
        };
        if let Some(warning) = self.inference.execution_evidence(&self.operation_id)
            ["audioFrontend"]["warning"]
            .as_str()
        {
            result.warnings.push(warning.to_owned());
        }
        if let Some(record) = session.as_ref() {
            let _timer = self
                .runtime
                .diagnostics
                .measure(&self.operation_id, Stage::Save);
            let mut provenance = vesper_models::provenance(&self.config);
            provenance["inference"] = vesper_inference::model_versions(&self.config);
            provenance["execution"] = self.inference.execution_evidence(&self.operation_id);
            match DatasetStore::open(self.dataset_root.clone())
                .and_then(|store| store.set_transcripts(&record.id, raw.clone(), None, provenance))
            {
                Ok(updated) => *session = Some(updated),
                Err(error) => result
                    .warnings
                    .push(format!("Raw transcription could not be saved: {error}")),
            }
        }
        result.session = session.clone();
        // Publish the durable raw result before a potentially slow or failed cleanup.
        self.runtime.publish(result.clone());
        self.runtime
            .publish_transcript_preview(&self.operation_id, &result);
        if vesper_models::diarization::available(&self.model_root, self.config.compute_backend) {
            self.runtime
                .progress("identifying_speakers", "Identifying speakers locally…");
            match self.inference.analyze_speakers(
                audio,
                &self.config,
                &self.model_root,
                &self.operation_id,
                Arc::clone(&self.runtime.cancelled),
            ) {
                Ok(timeline) => {
                    if timeline.clipped_segments > 0 {
                        self.event(
                            session,
                            "speaker_boundary_adjusted",
                            "Speaker frame padding was clipped to the recording's exact duration.",
                        );
                    }
                    if let Some(record) = session.as_ref() {
                        match DatasetStore::open(self.dataset_root.clone()).and_then(|store| {
                            store.complete_speaker_timeline(&record.id, timeline.clone())
                        }) {
                            Ok(updated) => *session = Some(updated),
                            Err(error) => result
                                .warnings
                                .push(format!("Speaker timeline could not be saved: {error}")),
                        }
                    }
                    result.speaker_timeline = Some(timeline);
                }
                Err(vesper_inference::InferenceError::Cancelled) => {
                    return Err("Transcription cancelled.".into())
                }
                Err(error) => {
                    self.event(session, "speaker_analysis_failed", &error.to_string());
                    result.warnings.push(format!(
                        "Transcript retained. Speaker analysis did not finish: {error}"
                    ));
                }
            }
            result.session = session.clone();
            self.runtime.publish(result.clone());
        }
        // Freeze eligibility from the raw hypothesis. Cleanup cannot generate its own cues.
        let post_asr_profile = self
            .personalization_snapshot()?
            .post_asr_profile(&raw, self.config.language);
        if self.config.cleanup_enabled && !raw.trim().is_empty() {
            self.runtime.progress(
                "rewriting",
                format!(
                    "Polishing the transcript with {}…",
                    self.config.rewriter_name()
                ),
            );
            let timer = self
                .runtime
                .diagnostics
                .measure(&self.operation_id, Stage::Cleanup);
            let cleaned = self.inference.rewrite_personalized(
                &raw,
                &self.config,
                &self.model_root,
                &self.operation_id,
                Arc::clone(&self.runtime.cancelled),
                &post_asr_profile,
            );
            drop(timer);
            match cleaned {
                Ok(cleaned) => {
                    if let Some(record) = session.as_ref() {
                        let _timer = self
                            .runtime
                            .diagnostics
                            .measure(&self.operation_id, Stage::Save);
                        match DatasetStore::open(self.dataset_root.clone())
                            .and_then(|store| store.complete_cleanup(&record.id, cleaned.clone()))
                        {
                            Ok(updated) => *session = Some(updated),
                            Err(error) => result
                                .warnings
                                .push(format!("Polished transcript could not be saved: {error}")),
                        }
                    }
                    result.cleaned_transcript = Some(cleaned);
                }
                Err(error) => {
                    let message = format!(
                        "Raw transcription retained. Text polishing did not finish: {error}"
                    );
                    self.event(session, "cleanup_error", &message);
                    result.warnings.push(message);
                }
            }
        }
        if self.runtime.cancelled.load(Ordering::Relaxed) {
            return Err("Transcription cancelled.".into());
        }
        let profile = self.personalization()?;
        result.learning_context = vesper_core::LearningContext {
            result_id: result.result_id.clone(),
            source_key: if profile.automatic_learning {
                vesper_data::learning_audio_key(audio.samples())
            } else {
                "disabled".into()
            },
            episode_id: result.result_id.clone(),
            language: self.config.language,
            project: profile.active_project.clone(),
            model_id: self.config.asr_name().into(),
            learning_epoch: profile.learning_epoch,
            learning_enabled: profile.automatic_learning,
        };
        let baseline = result.cleaned_transcript.as_deref().unwrap_or(&raw);
        let personalized = post_asr_profile.personalize(baseline, self.config.language);
        if personalized.text != baseline {
            result.personalized_transcript = Some(personalized.text.clone());
        }
        result.personalization_evidence = Some(personalized.evidence.clone());
        if let Some(record) = session.as_ref() {
            match DatasetStore::open(self.dataset_root.clone()).and_then(|store| {
                store.complete_personalization(
                    &record.id,
                    personalized.text,
                    personalized.evidence,
                    profile.active_project.clone(),
                )
            }) {
                Ok(updated) => *session = Some(updated),
                Err(error) => result.warnings.push(format!(
                    "Remembered correction result could not be saved: {error}"
                )),
            }
        }
        if let Some(record) = session.as_ref() {
            let evidence = self.inference.execution_evidence(&self.operation_id);
            match DatasetStore::open(self.dataset_root.clone())
                .and_then(|store| store.record_inference_evidence(&record.id, evidence))
            {
                Ok(updated) => *session = Some(updated),
                Err(error) => result.warnings.push(format!(
                    "Inference measurements could not be saved: {error}"
                )),
            }
        }
        self.event(
            session,
            "completed",
            "Transcription processing finished; human labels remain unverified.",
        );
        result.session = session.clone();
        self.runtime.publish(result.clone());
        Ok(result)
    }

    fn observe_result(&self, result: &mut PipelineResult, capture: bool) {
        let retry = self.runtime.connect_learning_attempt(
            &mut result.learning_context,
            &result.raw_transcript,
            capture,
        );
        if !result.learning_context.learning_enabled {
            return;
        }
        if let Err(error) = DatasetStore::open(self.dataset_root.clone()).and_then(|s| {
            s.observe_learning(&result.learning_context, &result.raw_transcript, retry)
        }) {
            result.warnings.push(format!(
                "Local learning evidence could not be saved: {error}"
            ));
        }
    }
}

fn no_speech_message(audio: &MonoPcm, language: Language) -> String {
    let samples = audio.samples();
    let peak = samples
        .iter()
        .fold(0.0_f32, |current, sample| current.max(sample.abs()));
    let rms = if samples.is_empty() {
        0.0
    } else {
        (samples.iter().map(|sample| sample * sample).sum::<f32>() / samples.len() as f32).sqrt()
    };

    if peak < 0.01 || rms < 0.001 {
        "No speech was recognized because the microphone signal was very quiet. Select the correct microphone and watch the microphone indicator respond while you speak.".into()
    } else if language == Language::Auto {
        "No speech was recognized. The microphone captured sound, but automatic language detection could not identify the speech. Select the language you are speaking in Settings and try again.".into()
    } else {
        "No speech was recognized. Confirm that the selected microphone reacts to your voice, move closer to it, and try again.".into()
    }
}
