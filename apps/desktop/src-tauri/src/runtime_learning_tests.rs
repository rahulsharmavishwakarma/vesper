//! Desktop completion/edit lifecycle with constructed ASR output and real SQLite.
//! No microphone, model execution, user profile, or inferred acceptance labels.
use super::*;
use vesper_core::AsrModel;

const HEARD: &str = "Deploy kubernets to the cluster";
const EDITED: &str = "Deploy kubernetes to the cluster";

#[test]
fn automatic_project_completion_and_delayed_edits_keep_receiving_scope_after_settings_change() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("dataset");
    let runtime = Arc::new(Runtime::default());
    let store = DatasetStore::open(root.clone()).unwrap();
    let mut profile = store.dictation_personalization().unwrap();
    profile.project_mode = vesper_core::ProjectMode::Automatic;
    profile.project_rules.push(vesper_core::ProjectRule {
        id: "editor-a".into(),
        application_id: "test.editor".into(),
        application_name: "Editor".into(),
        project: "Project A".into(),
        folder: None,
        enabled: true,
    });
    store.save_dictation_personalization(profile).unwrap();
    let mut snapshot = store.dictation_personalization_snapshot().unwrap();
    snapshot.resolve_project(Some("test.editor"), None);
    let context = WorkContext {
        operation_id: "recording-in-a".into(),
        config: AppConfig {
            cleanup_enabled: false,
            collect_training_data: false,
            language: Language::English,
            ..Default::default()
        },
        model_root: root.join("no-models"),
        dataset_root: root.clone(),
        runtime: Arc::clone(&runtime),
        inference: Arc::new(vesper_inference::InferenceManager::new().unwrap()),
        streaming_result: Arc::default(),
        personalization: Arc::default(),
        defer_speakers: Default::default(),
        pending_speakers: Default::default(),
    };
    context.personalization.set(Ok(snapshot)).unwrap();
    let mut changed = store.personalization().unwrap();
    changed.active_project = "Project B".into();
    changed.project_rules[0].project = "Changed rule".into();
    store.save_dictation_personalization(changed).unwrap();
    let audio = MonoPcm::new(vec![0.1; 1600], 16000).unwrap();
    let mut result = context
        .complete_raw(&audio, &mut None, "Open readme for deployment".into())
        .unwrap();
    context.observe_result(&mut result, false);
    runtime.publish(result.clone());
    assert_eq!(result.project_selection.project, "Project A");
    assert_eq!(
        result.project_selection.source,
        vesper_core::ProjectSource::Application
    );
    assert_eq!(result.learning_context.project, "Project A");
    runtime
        .edit_latest_draft(
            root,
            &result.result_id,
            0,
            "Open README.md for deployment".into(),
        )
        .unwrap();
    let saved = store.personalization().unwrap();
    assert_eq!(saved.active_project, "Project B");
    assert_eq!(saved.vocabulary.len(), 1);
    assert_eq!(saved.vocabulary[0].project, "Project A");
    assert!(store.list_sessions().unwrap().is_empty());
}

#[test]
fn automatic_app_context_guard_applies_to_later_raw_context_and_freezes_per_operation() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("dataset");
    let runtime = Arc::new(Runtime::default());
    let store = DatasetStore::open(root.clone()).unwrap();
    let mut profile = store.dictation_personalization().unwrap();
    profile.active_project = "repo".into();
    store.save_dictation_personalization(profile).unwrap();
    let frozen = WorkContext {
        operation_id: "already-started".into(),
        config: AppConfig {
            cleanup_enabled: false,
            collect_training_data: false,
            language: Language::English,
            ..Default::default()
        },
        model_root: root.join("no-models"),
        dataset_root: root.clone(),
        runtime: Arc::clone(&runtime),
        inference: Arc::new(vesper_inference::InferenceManager::new().unwrap()),
        streaming_result: Arc::default(),
        personalization: Arc::default(),
        defer_speakers: Default::default(),
        pending_speakers: Default::default(),
    };
    assert!(frozen.personalization().unwrap().vocabulary.is_empty());
    let heard = "Open readme for deployment";
    let edited = "Open README.md for deployment";
    let result = complete(
        &root,
        &runtime,
        1,
        AsrModel::CohereTranscribe,
        Language::English,
        heard,
    );
    runtime
        .edit_latest_draft(root.clone(), &result.result_id, 0, edited.into())
        .unwrap();
    let audio = MonoPcm::new(vec![0.1; 1600], 16000).unwrap();
    assert!(frozen
        .complete_raw(&audio, &mut None, heard.into())
        .unwrap()
        .personalized_transcript
        .is_none());
    let later = complete(
        &root,
        &runtime,
        2,
        AsrModel::CohereTranscribe,
        Language::English,
        heard,
    );
    assert_eq!(later.personalized_transcript.as_deref(), Some(edited));
    for (i, raw) in [
        "Open readme for shopping",
        "Open readme in repo",
        "Open 'readme' for deployment",
        "Open readme for 'deployment'",
    ]
    .iter()
    .enumerate()
    {
        let output = complete(
            &root,
            &runtime,
            3 + i as u16,
            AsrModel::CohereTranscribe,
            Language::English,
            raw,
        );
        assert!(output.personalized_transcript.is_none(), "{raw}");
        assert!(output.personalization_evidence.unwrap().applied.is_empty());
    }
    let snapshot = store.dictation_personalization_snapshot().unwrap();
    let cleanup_input = snapshot.post_asr_profile("Open readme for shopping", Language::English);
    assert!(!cleanup_input.vocabulary[0].enabled);
    assert_eq!(
        cleanup_input.personalize(heard, Language::English).text,
        heard,
        "cleanup cannot manufacture eligibility"
    );
    let positive_input = snapshot.post_asr_profile(heard, Language::English);
    assert_eq!(
        positive_input
            .personalize(
                "Do not open readme for deployment on 21 June",
                Language::English
            )
            .text,
        "Do not open README.md for deployment on 21 June"
    );
    let newly_corrected = complete(
        &root,
        &runtime,
        20,
        AsrModel::CohereTranscribe,
        Language::English,
        "Open readme for shopping",
    );
    runtime
        .edit_latest_draft(
            root.clone(),
            &newly_corrected.result_id,
            0,
            "Open README.md for shopping".into(),
        )
        .unwrap();
    assert!(store.personalization().unwrap().revision > snapshot.profile.revision);
    assert!(
        !snapshot
            .post_asr_profile("Open readme for shopping", Language::English)
            .vocabulary[0]
            .enabled
    );
    let learned_context = complete(
        &root,
        &runtime,
        21,
        AsrModel::CohereTranscribe,
        Language::English,
        "Open readme for shopping",
    );
    assert_eq!(
        learned_context.personalized_transcript.as_deref(),
        Some("Open README.md for shopping")
    );
    let mut changed = store.dictation_personalization().unwrap();
    changed.active_project = "shopping".into();
    store.save_dictation_personalization(changed).unwrap();
    let different_project = complete(
        &root,
        &runtime,
        10,
        AsrModel::CohereTranscribe,
        Language::English,
        heard,
    );
    assert!(different_project.personalized_transcript.is_none());
}

fn complete(
    root: &Path,
    runtime: &Arc<Runtime>,
    source: u16,
    model: AsrModel,
    language: Language,
    text: &str,
) -> PipelineResult {
    let mut config = AppConfig {
        cleanup_enabled: false,
        collect_training_data: false,
        language,
        ..Default::default()
    };
    config.models.asr = model;
    let context = WorkContext {
        operation_id: format!("learning-source-{source}"),
        config,
        model_root: root.join("no-models"),
        dataset_root: root.to_owned(),
        runtime: Arc::clone(runtime),
        inference: Arc::new(vesper_inference::InferenceManager::new().unwrap()),
        streaming_result: Arc::default(),
        personalization: Arc::default(),
        defer_speakers: Default::default(),
        pending_speakers: Default::default(),
    };
    let audio = MonoPcm::new(vec![f32::from(source) / 100.0; 1600], 16000).unwrap();
    let mut result = context
        .complete_raw(&audio, &mut None, text.into())
        .unwrap();
    context.observe_result(&mut result, false);
    runtime.publish(result.clone());
    result
}

fn teach(root: &Path, runtime: &Runtime, result: &PipelineResult) -> PipelineResult {
    runtime
        .edit_latest_draft(root.to_owned(), &result.result_id, 0, EDITED.into())
        .unwrap()
}

#[test]
fn automatic_app_learning_survives_restart_model_switch_and_forget_epoch() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("dataset");
    let runtime = Arc::new(Runtime::default());
    for source in [1, 2] {
        let result = complete(
            &root,
            &runtime,
            source,
            AsrModel::CohereTranscribe,
            Language::English,
            HEARD,
        );
        teach(&root, &runtime, &result);
    }
    drop(runtime);
    let runtime = Arc::new(Runtime::default());
    // Learned from Cohere's errors: Nemotron does not use the alias, Cohere still does.
    let result = complete(
        &root,
        &runtime,
        3,
        AsrModel::Nemotron35AsrStreaming06b,
        Language::English,
        HEARD,
    );
    assert_eq!(result.personalized_transcript, None);
    let result = complete(
        &root,
        &runtime,
        4,
        AsrModel::CohereTranscribe,
        Language::English,
        HEARD,
    );
    assert_eq!(result.personalized_transcript.as_deref(), Some(EDITED));
    let store = DatasetStore::open(root.clone()).unwrap();
    let profile = store.personalization().unwrap();
    let forgotten = store.forget_automatic_learning(profile.revision).unwrap();
    runtime.clear_learning_context();
    assert!(forgotten.automatic_learning);
    assert!(forgotten.vocabulary.is_empty());
    let old_result_edit = runtime
        .edit_latest_draft(root.clone(), &result.result_id, 0, HEARD.into())
        .unwrap();
    // Forget invalidates learning evidence, while the user's draft stays editable.
    assert_eq!(old_result_edit.draft_transcript.as_deref(), Some(HEARD));
    assert!(old_result_edit
        .learning_outcome
        .unwrap()
        .activated
        .is_empty());
    assert!(store.automatic_learning_state().unwrap().events.is_empty());
    for source in [4, 5] {
        let fresh = complete(
            &root,
            &runtime,
            source,
            AsrModel::CohereTranscribe,
            Language::English,
            HEARD,
        );
        assert!(fresh.personalized_transcript.is_none());
        teach(&root, &runtime, &fresh);
    }
    let fresh = complete(
        &root,
        &runtime,
        6,
        AsrModel::CohereTranscribe,
        Language::English,
        HEARD,
    );
    assert_eq!(fresh.personalized_transcript.as_deref(), Some(EDITED));
    let foreign = complete(
        &root,
        &runtime,
        7,
        AsrModel::CohereTranscribe,
        Language::Hindi,
        HEARD,
    );
    assert!(foreign.personalized_transcript.is_none());
    assert!(store.list_sessions().unwrap().is_empty());
    assert_eq!(std::fs::read_dir(root.join("audio")).unwrap().count(), 0);
}

#[test]
fn automatic_app_silence_and_duplicate_audio_do_not_supply_independent_edit_support() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("dataset");
    let runtime = Arc::new(Runtime::default());
    for source in 1..=6 {
        complete(
            &root,
            &runtime,
            source,
            AsrModel::CohereTranscribe,
            Language::English,
            HEARD,
        );
    }
    let store = DatasetStore::open(root.clone()).unwrap();
    assert!(store.personalization().unwrap().vocabulary.is_empty());
    assert!(store
        .automatic_learning_state()
        .unwrap()
        .candidates
        .is_empty());
    for _ in 0..3 {
        let result = complete(
            &root,
            &runtime,
            10,
            AsrModel::CohereTranscribe,
            Language::English,
            HEARD,
        );
        assert!(teach(&root, &runtime, &result)
            .learning_outcome
            .unwrap()
            .activated
            .is_empty());
    }
    assert_eq!(
        store.automatic_learning_state().unwrap().candidates[0]
            .support
            .len(),
        1
    );
    assert!(store.personalization().unwrap().vocabulary.is_empty());
    let distinct = complete(
        &root,
        &runtime,
        11,
        AsrModel::CohereTranscribe,
        Language::English,
        HEARD,
    );
    assert_eq!(
        teach(&root, &runtime, &distinct)
            .learning_outcome
            .unwrap()
            .activated,
        ["kubernetes"]
    );
    runtime
        .undo_latest_learning(&distinct.result_id, root.clone())
        .unwrap();
    let later = complete(
        &root,
        &runtime,
        12,
        AsrModel::CohereTranscribe,
        Language::English,
        HEARD,
    );
    assert!(later.personalized_transcript.is_none());
    // The undone spelling leaves the vocabulary instead of lingering disabled.
    assert!(store.personalization().unwrap().vocabulary.is_empty());
}

#[test]
fn automatic_app_failed_edit_keeps_revision_and_retries_once_without_duplicate_feedback() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("dataset");
    let runtime = Arc::new(Runtime::default());
    let result = complete(
        &root,
        &runtime,
        1,
        AsrModel::CohereTranscribe,
        Language::English,
        HEARD,
    );
    let invalid_root = dir.path().join("file-not-directory");
    std::fs::write(&invalid_root, b"deliberate storage failure").unwrap();
    assert!(runtime
        .edit_latest_draft(invalid_root, &result.result_id, 0, EDITED.into())
        .is_err());
    let unchanged = runtime.latest().unwrap().unwrap();
    assert_eq!(unchanged.draft_revision, 0);
    assert!(unchanged.draft_transcript.is_none());
    let edited = teach(&root, &runtime, &result);
    assert_eq!(edited.draft_revision, 1);
    let store = DatasetStore::open(root.clone()).unwrap();
    let events = store.automatic_learning_state().unwrap().events.len();
    runtime
        .edit_latest_draft(root.clone(), &edited.result_id, 1, EDITED.into())
        .unwrap();
    assert_eq!(
        store.automatic_learning_state().unwrap().events.len(),
        events
    );
    assert!(runtime
        .edit_latest_draft(root, &edited.result_id, 0, "stale".into())
        .is_err());
    assert_eq!(
        runtime
            .latest()
            .unwrap()
            .unwrap()
            .draft_transcript
            .as_deref(),
        Some(EDITED)
    );
}

#[test]
fn mined_terms_are_normalized_by_the_gate_and_a_user_revert_suspends_them() {
    use vesper_core::TermSource;
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("dataset");
    let runtime = Arc::new(Runtime::default());
    let store = DatasetStore::open(root.clone()).unwrap();
    store
        .replace_mined_sources(
            &[TermSource::Identifier],
            "",
            &[vesper_data::term_mining::MinedTerm::new(
                "sherpa_onnx",
                TermSource::Identifier,
                "",
                3,
            )],
            0,
        )
        .unwrap();
    let heard = "Link sherpa onnx in the build";
    let fixed = "Link sherpa_onnx in the build";
    let result = complete(
        &root,
        &runtime,
        1,
        AsrModel::CohereTranscribe,
        Language::English,
        heard,
    );
    assert_eq!(result.personalized_transcript.as_deref(), Some(fixed));
    let evidence = result.personalization_evidence.clone().unwrap();
    assert_eq!(evidence.gate.len(), 1);
    assert_eq!(evidence.gate[0].heard, "sherpa onnx");
    // Ordinary speech in the same recording context is never rewritten.
    let ordinary = complete(
        &root,
        &runtime,
        2,
        AsrModel::CohereTranscribe,
        Language::English,
        "I see the point you are making",
    );
    assert_eq!(ordinary.personalized_transcript, None);
    // The user puts the heard words back: that term stops applying.
    let result = complete(
        &root,
        &runtime,
        3,
        AsrModel::CohereTranscribe,
        Language::English,
        heard,
    );
    runtime
        .edit_latest_draft(root.clone(), &result.result_id, 0, heard.into())
        .unwrap();
    let later = complete(
        &root,
        &runtime,
        4,
        AsrModel::CohereTranscribe,
        Language::English,
        heard,
    );
    assert_eq!(later.personalized_transcript, None);
}
