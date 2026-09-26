//! Automatic analysis of explicit in-app edits. No external-editor observation.
use super::*;
use vesper_core::{SemanticMemory, SemanticStatus};

impl State {
    pub(super) fn cancel_semantic(&mut self) {
        if let Some(flag) = self.policy_cancelled.take() {
            flag.store(true, Ordering::Release);
        }
        if let Some(flag) = self.semantic_cancelled.take() {
            flag.store(true, Ordering::Release);
        }
        self.semantic_request = None;
    }
}

impl Runtime {
    /// Failure is intentionally independent of committing the user's draft.
    /// No model loads occur here or on the background inference path.
    pub(crate) fn schedule_semantic_shadow(
        self: &Arc<Self>,
        root: PathBuf,
        config: AppConfig,
        model_root: PathBuf,
        inference: &vesper_inference::InferenceManager,
    ) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        if state.busy || self.closing.load(Ordering::Acquire) {
            return;
        }
        let Some(mut request) = state.semantic_request.take() else {
            return;
        };
        let Ok(store) = DatasetStore::open(root.clone()) else {
            return;
        };
        let Ok(profile) = store.personalization() else {
            return;
        };
        // Scope and language are authoritative recording metadata. A settings
        // change cannot reinterpret an older edit as feedback for a new scope.
        if profile.active_project != request.context.project
            || config.language != request.context.language
        {
            return;
        }
        request.memories = profile
            .vocabulary
            .iter()
            .filter(|v| {
                v.enabled
                    && vesper_core::project_matches(&v.project, &request.context.project)
                    && vesper_core::language_matches(v.language, request.context.language)
                    && v.id.len() <= 64
                    && v.phrase.len() <= 128
            })
            .take(8)
            .map(|v| SemanticMemory {
                id: v.id.clone(),
                phrase: v.phrase.clone(),
            })
            .collect();
        if !matches!(store.begin_semantic_shadow(&request), Ok(true)) {
            return;
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        state.semantic_cancelled = Some(Arc::clone(&cancelled));
        let weak_runtime = Arc::downgrade(self);
        let callback_request = request.clone();
        let callback_flag = Arc::clone(&cancelled);
        let callback_root = root.clone();
        let result_id = request.context.result_id.clone();
        if inference
            .submit_semantic_shadow(request, config, model_root, cancelled, move |mut report| {
                let Some(runtime) = weak_runtime.upgrade() else {
                    return;
                };
                let Ok(state) = runtime.state.try_lock() else {
                    return;
                };
                let current = state.latest.as_ref().is_some_and(|r| {
                    r.result_id == callback_request.context.result_id
                        && r.draft_revision == callback_request.result_revision
                        && r.learning_context.learning_epoch
                            == callback_request.context.learning_epoch
                });
                if callback_flag.load(Ordering::Acquire)
                    || state.busy
                    || !current
                    || runtime.closing.load(Ordering::Acquire)
                {
                    report.status = SemanticStatus::Cancelled;
                    report.proposal = None;
                }
                // Hold the result lock through completion, serializing with new edits,
                // Undo and capture. The DB transaction separately checks epoch/reservation.
                if let Ok(store) = DatasetStore::open_shadow_completion(callback_root) {
                    let _ = store.finish_semantic_shadow(&callback_request, &report);
                }
            })
            .is_err()
        {
            state.cancel_semantic();
            let _ = store.invalidate_semantic_result(&result_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn settings_and_exit_cancel_background_work_without_changing_the_draft() {
        let runtime = Runtime::default();
        let flag = Arc::new(AtomicBool::new(false));
        runtime.state.lock().unwrap().semantic_cancelled = Some(Arc::clone(&flag));
        runtime.clear_learning_context();
        assert!(flag.load(Ordering::Acquire));
        let flag = Arc::new(AtomicBool::new(false));
        runtime.state.lock().unwrap().semantic_cancelled = Some(Arc::clone(&flag));
        runtime.prepare_exit().unwrap();
        assert!(flag.load(Ordering::Acquire));
        assert!(runtime.begin("recording", "late").is_err());
    }
    #[test]
    #[ignore = "installed writer and temporary data store; no microphone or user data"]
    fn installed_writer_edit_completion_and_forget_race() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let model_root = PathBuf::from(std::env::var("TRANSCRIBE_MODEL_DIR").unwrap());
        let store = DatasetStore::open(root.clone()).unwrap();
        let profile = store.dictation_personalization().unwrap();
        let runtime = Arc::new(Runtime::default());
        let manager = vesper_inference::InferenceManager::new().unwrap();
        let config = AppConfig {
            collect_training_data: false,
            ..Default::default()
        };
        manager
            .rewrite(
                "Buy rice",
                &config,
                &model_root,
                Arc::new(AtomicBool::new(false)),
            )
            .unwrap();
        let result = PipelineResult {
            result_id: "native-shadow".into(),
            raw_transcript: "Buy rice".into(),
            cleaned_transcript: None,
            speaker_timeline: None,
            personalized_transcript: None,
            personalization_evidence: None,
            draft_transcript: None,
            draft_revision: 0,
            learning_outcome: None,
            learning_context: vesper_core::LearningContext {
                result_id: "native-shadow".into(),
                source_key: "authored-native".into(),
                episode_id: "native-shadow".into(),
                learning_epoch: profile.learning_epoch,
                learning_enabled: true,
                language: Language::English,
                ..Default::default()
            },
            session: None,
            warnings: vec![],
            duration_seconds: 1.0,
        };
        runtime.publish(result);
        runtime
            .edit_latest_draft(root.clone(), "native-shadow", 0, "Buy rice and eggs".into())
            .unwrap();
        let after_spelling = store.personalization().unwrap();
        runtime.schedule_semantic_shadow(
            root.clone(),
            config.clone(),
            model_root.clone(),
            &manager,
        );
        let started = std::time::Instant::now();
        let report = loop {
            let records = store.semantic_shadow_records().unwrap();
            if let Some(record) = records
                .first()
                .filter(|r| r.report.status != SemanticStatus::Pending)
            {
                break record.report.clone();
            }
            assert!(
                started.elapsed() < std::time::Duration::from_secs(10),
                "missing native completion"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        };
        assert_eq!(store.personalization().unwrap(), after_spelling);
        assert_eq!(
            runtime
                .latest()
                .unwrap()
                .unwrap()
                .draft_transcript
                .as_deref(),
            Some("Buy rice and eggs")
        );
        assert!(store.list_sessions().unwrap().is_empty());
        assert!(!matches!(
            report.status,
            SemanticStatus::Unsupported | SemanticStatus::Failed
        ));
        runtime
            .edit_latest_draft(root.clone(), "native-shadow", 1, "Buy rice and tea".into())
            .unwrap();
        runtime.schedule_semantic_shadow(root, config, model_root, &manager);
        let started = std::time::Instant::now();
        while manager.semantic_queue_evidence()["running"] != true {
            assert!(started.elapsed() < std::time::Duration::from_secs(2));
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        store
            .forget_automatic_learning(store.personalization().unwrap().revision)
            .unwrap();
        runtime.clear_learning_context();
        drop(manager);
        assert!(store.semantic_shadow_records().unwrap().is_empty());
        assert_eq!(
            runtime
                .latest()
                .unwrap()
                .unwrap()
                .draft_transcript
                .as_deref(),
            Some("Buy rice and tea")
        );
        eprintln!(
            "Native edit report status: {:?}; Forget while running preserved deletion",
            report.status
        );
    }

    #[test]
    fn automatic_edit_schedules_shadow_without_audio_collection_and_capture_cancels() {
        let dir = tempfile::tempdir().unwrap();
        let store = DatasetStore::open(dir.path().into()).unwrap();
        let profile = store.dictation_personalization().unwrap();
        let runtime = Arc::new(Runtime::default());
        runtime.publish(PipelineResult {
            result_id: "r".into(),
            raw_transcript: "read me".into(),
            cleaned_transcript: None,
            speaker_timeline: None,
            personalized_transcript: None,
            personalization_evidence: None,
            draft_transcript: None,
            draft_revision: 0,
            learning_outcome: None,
            learning_context: vesper_core::LearningContext {
                result_id: "r".into(),
                source_key: "source".into(),
                episode_id: "r".into(),
                learning_epoch: profile.learning_epoch,
                learning_enabled: true,
                language: Language::English,
                ..Default::default()
            },
            session: None,
            warnings: vec![],
            duration_seconds: 1.0,
        });
        let edited = runtime
            .edit_latest_draft(dir.path().into(), "r", 0, "README".into())
            .unwrap();
        let manager = vesper_inference::InferenceManager::new().unwrap();
        runtime.schedule_semantic_shadow(
            dir.path().into(),
            AppConfig::default(),
            dir.path().into(),
            &manager,
        );
        assert_eq!(manager.semantic_queue_evidence()["submitted"], 1);
        let flag = runtime
            .state
            .lock()
            .unwrap()
            .semantic_cancelled
            .clone()
            .unwrap();
        runtime.begin("recording", "new dictation").unwrap();
        assert!(flag.load(Ordering::Acquire));
        drop(manager); // joins completion; no detached persistence callbacks
        assert_eq!(edited.draft_transcript.as_deref(), Some("README"));
        assert!(store.list_sessions().unwrap().is_empty());
        assert_eq!(store.semantic_shadow_records().unwrap().len(), 1);
        runtime.finish(&Ok::<_, String>(()));
        runtime
            .undo_latest_learning("r", dir.path().into())
            .unwrap();
        assert!(store.semantic_shadow_records().unwrap().is_empty());
    }
}
