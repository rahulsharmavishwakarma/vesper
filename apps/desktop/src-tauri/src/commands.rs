use serde::Serialize;
use transcribe_core::{
    AppConfig, HuggingFaceProfile, ModelRuntime, ModelSpec, RewriteQuantization,
};
use transcribe_data::{DatasetStore, ExportSummary, SessionRecord, TranscriptFeedback};

use crate::runtime::{PipelineResult, RuntimeStatus, WorkContext};
use crate::{replace_global_shortcut, AppStorage};
use std::{
    path::Path,
    sync::{atomic::Ordering, Arc},
    time::Duration,
};
use tauri::{Emitter, Manager};
use tauri_plugin_notification::NotificationExt;

fn notify_transcription(
    app: &tauri::AppHandle,
    enabled: bool,
    result: &Result<PipelineResult, String>,
) {
    if !enabled {
        return;
    }
    let (title, body) = match result {
        Ok(_) => ("transcription ready", "Your words are ready in Vesper."),
        Err(_) => ("transcription could not finish", "Open Vesper for details."),
    };
    let _ = app.notification().builder().title(title).body(body).show();
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    name: &'static str,
    version: &'static str,
    build_type: &'static str,
    os: &'static str,
    arch: &'static str,
}

/// Compiled metadata remains available even when settings or models cannot load.
#[tauri::command]
pub fn get_app_info() -> AppInfo {
    AppInfo {
        name: "Vesper",
        version: env!("CARGO_PKG_VERSION"),
        build_type: if cfg!(debug_assertions) {
            "development"
        } else {
            "release"
        },
        os: std::env::consts::OS,
        arch: std::env::consts::ARCH,
    }
}

#[derive(Serialize)]
pub struct AppStatus {
    name: &'static str,
    version: &'static str,
    inference_ready: bool,
    asr_model: String,
    rewrite_model: String,
    message: &'static str,
    shortcut_error: Option<String>,
}

#[tauri::command]
pub fn get_app_status(storage: tauri::State<'_, AppStorage>) -> Result<AppStatus, String> {
    let config = storage.settings.load()?;
    Ok(AppStatus {
        name: "Vesper",
        version: env!("CARGO_PKG_VERSION"),
        inference_ready: transcribe_models::inspect(
            &config,
            &transcribe_models::model_directory(&config, &storage.model_dir),
        )
        .ready,
        asr_model: config.asr_name().into(),
        rewrite_model: config.rewriter_name().into(),
        message:
            "Local speech transcription and text polishing. Install the selected models to begin.",
        shortcut_error: storage
            .shortcut_status
            .lock()
            .map_err(|e| e.to_string())?
            .error
            .clone(),
    })
}

#[tauri::command]
pub fn get_model_catalog() -> Vec<ModelSpec> {
    transcribe_core::default_model_catalog()
}

#[tauri::command]
pub fn get_adapter_registry() -> &'static [transcribe_core::AdapterRegistration] {
    transcribe_core::ADAPTERS
}

#[tauri::command]
pub fn get_selected_adapters(
    storage: tauri::State<'_, AppStorage>,
) -> Result<serde_json::Value, String> {
    let config = storage.settings.load()?;
    Ok(serde_json::json!({"speech":config.speech_adapter(), "text":config.text_adapter()}))
}

#[tauri::command]
pub async fn get_personalization(
    storage: tauri::State<'_, AppStorage>,
) -> Result<transcribe_core::PersonalizationProfile, String> {
    let root = storage.dataset_dir.clone();
    tauri::async_runtime::spawn_blocking(move || {
        DatasetStore::open(root)
            .and_then(|s| s.dictation_personalization())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn save_personalization(
    profile: transcribe_core::PersonalizationProfile,
    app: tauri::AppHandle,
    storage: tauri::State<'_, AppStorage>,
) -> Result<transcribe_core::PersonalizationProfile, String> {
    crate::external_observation::cancel(&app);
    let root = storage.dataset_dir.clone();
    let saved = tauri::async_runtime::spawn_blocking(move || {
        DatasetStore::open(root)
            .and_then(|s| s.save_dictation_personalization(profile))
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;
    storage.runtime.clear_learning_context();
    Ok(saved)
}

#[tauri::command]
pub async fn get_automatic_learning(
    storage: tauri::State<'_, AppStorage>,
) -> Result<transcribe_data::AutomaticLearningState, String> {
    let root = storage.dataset_dir.clone();
    tauri::async_runtime::spawn_blocking(move || {
        DatasetStore::open(root)
            .and_then(|s| s.automatic_learning_state())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Local diagnostic evidence only; this endpoint cannot promote a policy or memory.
#[tauri::command]
pub async fn get_learning_shadow_diagnostics(
    storage: tauri::State<'_, AppStorage>,
) -> Result<serde_json::Value, String> {
    let root = storage.dataset_dir.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let store=DatasetStore::open(root).map_err(|e| e.to_string())?;
        let policy=store.policy_shadow_state().map_err(|e| e.to_string())?;
        let external=store.external_shadow_records().map_err(|e| e.to_string())?;
        Ok(serde_json::json!({"policy":policy,"external":{"retained":external.len(),"completed":external.iter().filter(|r| r.after.is_some()).count()},"activePolicyChanged":false}))
    }).await.map_err(|e|e.to_string())?
}

#[tauri::command]
pub async fn forget_automatic_learning(
    revision: u64,
    app: tauri::AppHandle,
    storage: tauri::State<'_, AppStorage>,
) -> Result<transcribe_core::PersonalizationProfile, String> {
    crate::external_observation::cancel(&app);
    let root = storage.dataset_dir.clone();
    let profile = tauri::async_runtime::spawn_blocking(move || {
        DatasetStore::open(root)
            .and_then(|s| s.forget_automatic_learning(revision))
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;
    storage.runtime.clear_learning_context();
    Ok(profile)
}

#[tauri::command]
pub async fn edit_latest_draft(
    result_id: String,
    revision: u64,
    text: String,
    storage: tauri::State<'_, AppStorage>,
) -> Result<PipelineResult, String> {
    let root = storage.dataset_dir.clone();
    let runtime = Arc::clone(&storage.runtime);
    let inference = Arc::clone(&storage.inference);
    let config = storage.settings.load().ok();
    let model_root = storage.model_dir.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let result = runtime.edit_latest_draft(root.clone(), &result_id, revision, text)?;
        if let Some(config) = config {
            let model_root = transcribe_models::model_directory(&config, &model_root);
            runtime.schedule_semantic_shadow(root, config, model_root, &inference);
        }
        Ok(result)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn undo_latest_learning(
    result_id: String,
    storage: tauri::State<'_, AppStorage>,
) -> Result<PipelineResult, String> {
    let root = storage.dataset_dir.clone();
    let runtime = Arc::clone(&storage.runtime);
    tauri::async_runtime::spawn_blocking(move || runtime.undo_latest_learning(&result_id, root))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn remember_cleanup(
    id: String,
    storage: tauri::State<'_, AppStorage>,
) -> Result<transcribe_core::PersonalizationProfile, String> {
    let root = storage.dataset_dir.clone();
    tauri::async_runtime::spawn_blocking(move || {
        DatasetStore::open(root)
            .and_then(|s| {
                let intent = s.personalization()?.output_intent;
                s.remember_cleanup(&id, intent)
            })
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn get_correction_suggestions(
    id: String,
    storage: tauri::State<'_, AppStorage>,
) -> Result<Vec<transcribe_data::CorrectionSuggestion>, String> {
    let root = storage.dataset_dir.clone();
    tauri::async_runtime::spawn_blocking(move || {
        DatasetStore::open(root)
            .and_then(|s| s.correction_suggestions(&id))
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn remember_correction(
    request: transcribe_data::RememberCorrection,
    storage: tauri::State<'_, AppStorage>,
) -> Result<transcribe_core::PersonalizationProfile, String> {
    let root = storage.dataset_dir.clone();
    tauri::async_runtime::spawn_blocking(move || {
        DatasetStore::open(root)
            .and_then(|s| s.remember_correction(request))
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn get_learning_dashboard(
    storage: tauri::State<'_, AppStorage>,
) -> Result<transcribe_data::LearningDashboard, String> {
    let root = storage.dataset_dir.clone();
    tauri::async_runtime::spawn_blocking(move || {
        DatasetStore::open(root)
            .and_then(|s| s.learning_dashboard())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn undo_personalization(
    id: String,
    storage: tauri::State<'_, AppStorage>,
) -> Result<transcribe_data::SessionRecord, String> {
    let root = storage.dataset_dir.clone();
    let record = tauri::async_runtime::spawn_blocking(move || {
        DatasetStore::open(root)
            .and_then(|s| s.undo_personalization(&id))
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;
    storage.runtime.undo_personalization(&record);
    Ok(record)
}

#[tauri::command]
pub async fn undo_unsaved_personalization(
    result_id: String,
    storage: tauri::State<'_, AppStorage>,
) -> Result<(), String> {
    let root = storage.dataset_dir.clone();
    let runtime = Arc::clone(&storage.runtime);
    tauri::async_runtime::spawn_blocking(move || {
        runtime.undo_unsaved_with_learning(&result_id, root)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn get_accelerator_capabilities(
) -> Result<transcribe_inference::AcceleratorCapabilities, String> {
    tauri::async_runtime::spawn_blocking(|| {
        transcribe_inference::accelerator_capabilities().clone()
    })
    .await
    .map_err(|error| error.to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineDiagnostics {
    resources: crate::resources::ResourceSample,
    activity: serde_json::Value,
    timings: Option<crate::diagnostics::JobTimings>,
    execution: serde_json::Value,
}

#[tauri::command]
pub async fn get_engine_diagnostics(
    storage: tauri::State<'_, AppStorage>,
) -> Result<EngineDiagnostics, String> {
    let resources = Arc::clone(&storage.resources);
    let runtime = Arc::clone(&storage.runtime);
    let inference = Arc::clone(&storage.inference);
    tauri::async_runtime::spawn_blocking(move || {
        let sample = resources
            .lock()
            .map_err(|error| error.to_string())?
            .sample();
        Ok(EngineDiagnostics {
            resources: sample,
            activity: runtime.activity()?,
            timings: runtime.diagnostics.snapshot(),
            execution: inference.latest_execution_evidence(),
        })
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub fn get_settings(storage: tauri::State<'_, AppStorage>) -> Result<AppConfig, String> {
    storage.settings.load()
}

#[tauri::command]
pub fn retry_global_shortcut(
    app: tauri::AppHandle,
    storage: tauri::State<'_, AppStorage>,
) -> Result<(), String> {
    let _gate = storage.operation_gate.lock().map_err(|e| e.to_string())?;
    storage.runtime.require_idle()?;
    let shortcut = storage.settings.load()?;
    replace_global_shortcut(&app, Some(&shortcut), &shortcut)
}

#[tauri::command]
pub fn save_settings(
    settings: AppConfig,
    app: tauri::AppHandle,
    storage: tauri::State<'_, AppStorage>,
) -> Result<AppConfig, String> {
    let _gate = storage.operation_gate.lock().map_err(|e| e.to_string())?;
    storage.runtime.require_idle()?;
    settings.validate().map_err(|e| e.to_string())?;
    let previous = storage.settings.load()?;
    replace_global_shortcut(&app, Some(&previous), &settings)?;
    match storage.settings.save(&settings) {
        Ok(saved) => {
            crate::external_observation::cancel(&app);
            crate::dictation::apply_capture_design(&app, saved.capture_design);
            storage.runtime.clear_learning_context();
            let _ = storage.inference.unload();
            Ok(saved)
        }
        Err(error) => {
            let _ = replace_global_shortcut(&app, Some(&settings), &previous);
            Err(error)
        }
    }
}

#[tauri::command]
pub fn reset_settings(
    app: tauri::AppHandle,
    storage: tauri::State<'_, AppStorage>,
) -> Result<AppConfig, String> {
    let _gate = storage.operation_gate.lock().map_err(|e| e.to_string())?;
    storage.runtime.require_idle()?;
    let reset = reset_settings_with_shortcut(&storage.settings, |previous, next| {
        replace_global_shortcut(&app, previous, next)
    });
    if let Ok(saved) = &reset {
        crate::external_observation::cancel(&app);
        crate::dictation::apply_capture_design(&app, saved.capture_design);
        storage.runtime.clear_learning_context();
        let _ = storage.inference.unload();
    }
    reset
}

fn reset_settings_with_shortcut(
    settings: &crate::settings::SettingsStore,
    mut replace: impl FnMut(Option<&AppConfig>, &AppConfig) -> Result<(), String>,
) -> Result<AppConfig, String> {
    let previous = settings.load();
    let defaults = AppConfig::default();
    let previous_shortcut = previous.as_ref().unwrap_or(&defaults);
    let registration = replace(Some(previous_shortcut), &defaults);
    if previous.is_ok() {
        registration?;
    }
    // When recovering invalid preferences, a shortcut conflict must not prevent
    // manual recording from becoming available. The replacement callback keeps
    // the registration error in ShortcutStatus for the existing warning UI.
    match settings.save(&defaults) {
        Ok(saved) => Ok(saved),
        Err(error) => {
            let _ = replace(Some(&defaults), previous_shortcut);
            Err(error)
        }
    }
}

/// Import validates and selects a data-only model profile. Installation remains
/// an explicit separate action and no repository code is ever executed.
#[tauri::command]
pub fn import_model_profile(
    profile_json: String,
    storage: tauri::State<'_, AppStorage>,
) -> Result<AppConfig, String> {
    let _gate = storage.operation_gate.lock().map_err(|e| e.to_string())?;
    storage.runtime.require_idle()?;
    let mut config = storage.settings.load()?;
    apply_model_profile(&mut config, &profile_json)?;
    let saved = storage.settings.save(&config)?;
    let _ = storage.inference.unload();
    Ok(saved)
}

fn apply_model_profile(config: &mut AppConfig, profile_json: &str) -> Result<(), String> {
    if profile_json.len() > 32_768 {
        return Err("Model profile JSON must not exceed 32 KiB".into());
    }
    let profile: HuggingFaceProfile = serde_json::from_str(profile_json)
        .map_err(|error| format!("Model profile could not be decoded: {error}"))?;
    profile.validate().map_err(|error| error.to_string())?;
    let mut updated = config.clone();
    match profile.runtime {
        ModelRuntime::SherpaOnnxTransducer => updated.models.custom_asr = Some(profile),
        ModelRuntime::LlamaCpp => updated.models.custom_rewriter = Some(profile),
    }
    updated.validate().map_err(|error| error.to_string())?;
    *config = updated;
    Ok(())
}

#[tauri::command]
pub fn clear_model_profile(
    role: String,
    storage: tauri::State<'_, AppStorage>,
) -> Result<AppConfig, String> {
    let _gate = storage.operation_gate.lock().map_err(|e| e.to_string())?;
    storage.runtime.require_idle()?;
    let mut config = storage.settings.load()?;
    match role.as_str() {
        "asr" => config.models.custom_asr = None,
        "rewriter" => config.models.custom_rewriter = None,
        _ => return Err("Model role must be asr or rewriter".into()),
    }
    let saved = storage.settings.save(&config)?;
    let _ = storage.inference.unload();
    Ok(saved)
}

#[tauri::command]
pub async fn list_sessions(
    storage: tauri::State<'_, AppStorage>,
) -> Result<Vec<SessionRecord>, String> {
    let root = storage.dataset_dir.clone();
    tauri::async_runtime::spawn_blocking(move || {
        DatasetStore::open(root)
            .and_then(|store| store.list_sessions())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn import_audio(
    path: String,
    storage: tauri::State<'_, AppStorage>,
) -> Result<SessionRecord, String> {
    let root = storage.dataset_dir.clone();
    let config = storage.settings.load()?;
    tauri::async_runtime::spawn_blocking(move || {
        DatasetStore::open(root)
            .and_then(|store| store.import_audio(std::path::Path::new(&path), &config))
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn approve_correction(
    id: String,
    corrected_text: String,
    storage: tauri::State<'_, AppStorage>,
) -> Result<SessionRecord, String> {
    let root = storage.dataset_dir.clone();
    tauri::async_runtime::spawn_blocking(move || {
        DatasetStore::open(root)
            .and_then(|store| store.approve_correction(&id, corrected_text))
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn approve_transcription(
    id: String,
    verbatim_text: String,
    storage: tauri::State<'_, AppStorage>,
) -> Result<SessionRecord, String> {
    let root = storage.dataset_dir.clone();
    tauri::async_runtime::spawn_blocking(move || {
        DatasetStore::open(root)
            .and_then(|store| store.approve_transcription(&id, verbatim_text))
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn set_transcript_feedback(
    id: String,
    feedback: Option<TranscriptFeedback>,
    storage: tauri::State<'_, AppStorage>,
) -> Result<SessionRecord, String> {
    let root = storage.dataset_dir.clone();
    tauri::async_runtime::spawn_blocking(move || {
        DatasetStore::open(root)
            .and_then(|store| store.set_transcript_feedback(&id, feedback))
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn export_dataset(
    destination: String,
    approved_only: bool,
    storage: tauri::State<'_, AppStorage>,
) -> Result<ExportSummary, String> {
    let root = storage.dataset_dir.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let parent = std::path::PathBuf::from(destination);
        if !parent.is_dir() {
            return Err("Choose an existing directory for the export".to_owned());
        }
        let export_dir = parent.join(format!("transcribe-export-{}", uuid::Uuid::new_v4()));
        DatasetStore::open(root)
            .and_then(|store| store.export_dataset(&export_dir, approved_only))
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

fn work_context(storage: &AppStorage) -> Result<WorkContext, String> {
    let config = storage.settings.load()?;
    config.validate().map_err(|e| e.to_string())?;
    Ok(WorkContext {
        operation_id: uuid::Uuid::new_v4().to_string(),
        model_root: transcribe_models::model_directory(&config, &storage.model_dir),
        config,
        dataset_root: storage.dataset_dir.clone(),
        runtime: Arc::clone(&storage.runtime),
        inference: Arc::clone(&storage.inference),
        streaming_result: Arc::new(std::sync::Mutex::new(None)),
        personalization: Arc::default(),
    })
}

fn begin_context(
    storage: &AppStorage,
    stage: &str,
    message: &str,
    models_required: bool,
) -> Result<WorkContext, String> {
    let _gate = storage.operation_gate.lock().map_err(|e| e.to_string())?;
    let context = work_context(storage)?;
    if models_required {
        require_models(&context)?;
    }
    context.runtime.begin(stage, message)?;
    context.runtime.set_operation_id(&context.operation_id);
    if models_required {
        context.runtime.diagnostics.begin(
            &context.operation_id,
            &context.config,
            matches!(stage, "recording" | "starting"),
        );
    }
    Ok(context)
}

#[tauri::command]
pub fn get_audio_features(
    storage: tauri::State<'_, AppStorage>,
) -> transcribe_audio::AudioFeatures {
    // Meter-only reads must not inspect model files or wait on inference/status locks.
    storage.recorder.current_features()
}

#[tauri::command]
pub fn get_runtime_status(storage: tauri::State<'_, AppStorage>) -> Result<RuntimeStatus, String> {
    let context = work_context(&storage)?;
    storage.runtime.status(
        transcribe_models::inspect(&context.config, &context.model_root),
        storage.recorder.current_level(),
        context.config.effective_recording_seconds(),
    )
}

#[tauri::command]
pub fn get_latest_result(
    storage: tauri::State<'_, AppStorage>,
) -> Result<Option<PipelineResult>, String> {
    storage.runtime.latest()
}

#[tauri::command]
pub async fn list_audio_devices() -> Result<Vec<transcribe_audio::InputDevice>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        transcribe_audio::list_input_devices().map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn get_model_installation(
    storage: tauri::State<'_, AppStorage>,
) -> Result<transcribe_models::Installation, String> {
    let context = work_context(&storage)?;
    Ok(transcribe_models::inspect(
        &context.config,
        &context.model_root,
    ))
}

#[tauri::command]
pub fn list_model_installations(
    storage: tauri::State<'_, AppStorage>,
) -> Result<Vec<transcribe_models::ModelInstallation>, String> {
    let config = storage.settings.load()?;
    let root = transcribe_models::model_directory(&config, &storage.model_dir);
    Ok(transcribe_models::list_model_installations(&config, &root))
}

#[tauri::command]
pub async fn install_models(
    storage: tauri::State<'_, AppStorage>,
) -> Result<transcribe_models::Installation, String> {
    let context = begin_context(&storage, "installing", "Preparing model download…", false)?;
    if let Err(error) = context.inference.unload() {
        let result: Result<(), String> = Err(error.to_string());
        context.runtime.finish(&result);
        return Err(error.to_string());
    }
    context
        .runtime
        .begin_download(&context.operation_id, None, None);
    let runtime = Arc::clone(&context.runtime);
    let result = tauri::async_runtime::spawn_blocking(move || {
        transcribe_models::install(
            &context.config,
            &context.model_root,
            &context.runtime.cancelled,
            |progress| {
                context
                    .runtime
                    .download_progress(&context.operation_id, progress)
            },
        )
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())
    .and_then(|result| result);
    runtime.finish(&result);
    result
}

#[tauri::command]
pub async fn install_model(
    model_id: String,
    quantization: Option<RewriteQuantization>,
    storage: tauri::State<'_, AppStorage>,
) -> Result<transcribe_models::ModelInstallation, String> {
    let context = begin_context(&storage, "installing", "Preparing model download…", false)?;
    if let Err(error) = context.inference.unload() {
        let result: Result<(), String> = Err(error.to_string());
        context.runtime.finish(&result);
        return Err(error.to_string());
    }
    context
        .runtime
        .begin_download(&context.operation_id, Some(model_id.clone()), quantization);
    let runtime = Arc::clone(&context.runtime);
    let result = tauri::async_runtime::spawn_blocking(move || {
        transcribe_models::install_model(
            &context.config,
            &context.model_root,
            &model_id,
            quantization,
            &context.runtime.cancelled,
            |progress| {
                context
                    .runtime
                    .download_progress(&context.operation_id, progress)
            },
        )
        .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| error.to_string())
    .and_then(|result| result);
    runtime.finish(&result);
    result
}

#[tauri::command]
pub async fn remove_model(
    model_id: String,
    role: transcribe_core::ModelRole,
    quantization: Option<RewriteQuantization>,
    model_directory: String,
    storage: tauri::State<'_, AppStorage>,
) -> Result<transcribe_models::ModelRemoval, String> {
    let context = begin_context(&storage, "removing", "Removing model files…", false)?;
    // Reuse the install boundary: no file changes while recording/inference is
    // active, and resident handles are released before changing model weights.
    if let Err(error) = context.inference.unload() {
        let result: Result<(), String> = Err(error.to_string());
        context.runtime.finish(&result);
        return Err(error.to_string());
    }
    let runtime = Arc::clone(&context.runtime);
    let result = tauri::async_runtime::spawn_blocking(move || {
        transcribe_models::remove_model(
            &context.config,
            &context.model_root,
            std::path::Path::new(&model_directory),
            &model_id,
            role,
            quantization,
        )
        .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| error.to_string())
    .and_then(|result| result);
    runtime.finish(&result);
    result
}

fn require_models(context: &WorkContext) -> Result<(), String> {
    let installation = transcribe_models::inspect(&context.config, &context.model_root);
    if !installation.ready {
        Err(installation.message)
    } else {
        Ok(())
    }
}

#[tauri::command]
pub async fn transcribe_file(
    path: String,
    app: tauri::AppHandle,
    storage: tauri::State<'_, AppStorage>,
) -> Result<PipelineResult, String> {
    let context = begin_context(&storage, "transcribing", "Opening audio…", true)?;
    let notifications_enabled = context.config.notifications_enabled;
    let runtime = Arc::clone(&context.runtime);
    let result =
        tauri::async_runtime::spawn_blocking(move || context.process_file(Path::new(&path)))
            .await
            .map_err(|e| e.to_string())
            .and_then(|result| result);
    runtime.finish(&result);
    notify_transcription(&app, notifications_enabled, &result);
    result
}

#[tauri::command]
pub async fn start_recording(
    device_id: Option<String>,
    app: tauri::AppHandle,
    storage: tauri::State<'_, AppStorage>,
) -> Result<String, String> {
    start_recording_inner(device_id, app, storage, None).await
}

pub(crate) async fn start_recording_inner(
    device_id: Option<String>,
    app: tauri::AppHandle,
    storage: tauri::State<'_, AppStorage>,
    dictation_id: Option<u64>,
) -> Result<String, String> {
    if dictation_id.is_some_and(|id| !crate::dictation::start_allowed(&storage, id)) {
        return Err("Recording cancelled.".into());
    }
    crate::external_observation::cancel(&app);
    let context = begin_context(&storage, "starting", "Starting microphone…", true)?;
    if dictation_id.is_some_and(|id| !crate::dictation::bind(&storage, id, &context.operation_id)) {
        let result = Err("Recording cancelled.".into());
        context.runtime.finish(&result);
        return result;
    }
    let recorder = Arc::clone(&storage.recorder);
    let max_seconds = context.config.effective_recording_seconds();
    let setup_context = context.clone();
    let setup_app = app.clone();
    let started = tauri::async_runtime::spawn_blocking(move || {
        let live = setup_context
            .config
            .speech_adapter()
            .capabilities
            .native_streaming;
        if !live {
            if dictation_id.is_some_and(|id| !crate::dictation::start_allowed(&setup_app.state(), id)) {
                return Err("Shortcut released before the microphone was ready. Hold again and wait for Listening.".into());
            }
            recorder
                .start(device_id, max_seconds)
                .map_err(|error| error.to_string())?;
            setup_context.runtime.set_recording(setup_context.clone());
            return Ok(None);
        }
        setup_context
            .runtime
            .progress("starting", "Loading the resident streaming recognizer…");
        let callback_runtime = Arc::clone(&setup_context.runtime);
        let callback_app = setup_app.clone();
        let updates: transcribe_inference::StreamingUpdateSink = Arc::new(move |update| {
            if callback_runtime.publish_partial(update.clone()) {
                let _ = callback_app.emit("transcription-partial", update);
            }
        });
        let stream = setup_context
            .inference
            .start_stream_personalized(
                &setup_context.config,
                &setup_context.model_root,
                &setup_context.operation_id,
                Arc::clone(&setup_context.runtime.cancelled),
                updates,
                setup_context.personalization()?,
            )
            .map_err(|error| error.to_string())?;
        if dictation_id.is_some_and(|id| !crate::dictation::start_allowed(&setup_app.state(), id)) {
            let _ = stream.cancel();
            return Err("Shortcut released before the microphone was ready. Hold again and wait for Listening.".into());
        }
        let chunks = match recorder.start_streaming(device_id, max_seconds) {
            Ok(chunks) => chunks,
            Err(error) => {
                let _ = stream.cancel();
                return Err(error.to_string());
            }
        };
        let (completed, result) = std::sync::mpsc::sync_channel(1);
        let cancelled = Arc::clone(&setup_context.runtime.cancelled);
        setup_context.runtime.set_recording(setup_context.clone());
        std::thread::Builder::new()
            .name("transcribe-live-audio".into())
            .spawn(move || {
                let result = forward_live_audio(chunks, stream, cancelled);
                if result.is_err() {
                    setup_context.runtime.invalidate_partial(&setup_context.operation_id);
                }
                let _ = completed.send(result);
            })
            .map_err(|error| error.to_string())?;
        Ok(Some(result))
    })
    .await
    .map_err(|e| e.to_string())
    .and_then(|result| result);
    let streaming_result = match started {
        Ok(result) => result,
        Err(error) => {
            let _ = storage.recorder.cancel();
            let failure: Result<(), String> = Err(error.clone());
            context.runtime.finish(&failure);
            return Err(error);
        }
    };
    if let Some(result) = streaming_result {
        context.set_streaming_result(result);
    }
    if context.runtime.cancelled.load(Ordering::Relaxed) {
        if context.runtime.closing.load(Ordering::Relaxed) {
            let preserved = storage
                .recorder
                .stop()
                .map_err(|e| e.to_string())
                .and_then(|audio| context.preserve_on_close(audio));
            if let Err(error) = preserved {
                let result = Err(error);
                context.runtime.finish(&result);
                return result;
            }
        } else {
            let _ = storage.recorder.cancel();
        }
        let result = Err("Recording cancelled".into());
        context.runtime.finish(&result);
        return result;
    }
    let operation_id = context.operation_id.clone();
    context.runtime.progress(
        "recording",
        if max_seconds == 0 {
            "Recording. Press Stop when finished.".into()
        } else {
            format!("Recording. This adapter stops after {max_seconds} seconds.")
        },
    );
    let recorder = Arc::clone(&storage.recorder);
    let runtime = Arc::clone(&context.runtime);
    let notifications_enabled = context.config.notifications_enabled;
    // Capture limits are enforced by the audio worker; this watcher also runs the
    // transcription if the window is minimized or the frontend stops polling.
    std::thread::spawn(move || {
        while runtime.is_recording(&context.operation_id) {
            std::thread::sleep(Duration::from_millis(250));
            if !runtime.is_recording(&context.operation_id) {
                break;
            }
            let status = recorder.status();
            if status.limit_reached
                || status.error.is_some()
                || runtime.cancelled.load(Ordering::Relaxed)
            {
                let Some(context) = runtime.take_recording_for(&context.operation_id) else {
                    break;
                };
                if runtime.cancelled.load(Ordering::Relaxed) {
                    let result = if runtime.closing.load(Ordering::Relaxed) {
                        recorder
                            .stop()
                            .map_err(|e| e.to_string())
                            .and_then(|audio| context.preserve_on_close(audio))
                    } else {
                        recorder.cancel().map_err(|e| e.to_string())
                    };
                    tauri::async_runtime::block_on(crate::dictation::completed(
                        &app,
                        &context.operation_id,
                        &Err("Recording cancelled.".into()),
                    ));
                    runtime.finish(&result);
                } else {
                    runtime.progress("transcribing", "Recording finished. Processing audio…");
                    let operation_id = context.operation_id.clone();
                    let result = recorder
                        .stop()
                        .map_err(|e| e.to_string())
                        .and_then(|audio| context.process_capture(audio));
                    tauri::async_runtime::block_on(crate::dictation::completed(
                        &app,
                        &operation_id,
                        &result,
                    ));
                    runtime.finish(&result);
                    notify_transcription(&app, notifications_enabled, &result);
                }
                break;
            }
        }
    });
    Ok(operation_id)
}

fn forward_live_audio(
    chunks: transcribe_audio::AudioChunkStream,
    stream: transcribe_inference::StreamingSession,
    cancelled: Arc<std::sync::atomic::AtomicBool>,
) -> Result<String, String> {
    let (chunks, interrupted) = chunks.into_parts();
    for chunk in chunks {
        if cancelled.load(Ordering::Relaxed) {
            let _ = stream.cancel();
            return Err("Streaming transcription cancelled".into());
        }
        if let Err(error) = stream.push(chunk.samples, chunk.sample_rate) {
            return Err(error.to_string());
        }
    }
    if interrupted.load(Ordering::Acquire) {
        let _ = stream.cancel();
        return Err("Live audio queue fell behind; using the complete recording fallback".into());
    }
    if cancelled.load(Ordering::Relaxed) {
        stream
            .cancel()
            .and(Err(transcribe_inference::InferenceError::Cancelled))
    } else {
        stream.finish()
    }
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn stop_recording(
    app: tauri::AppHandle,
    storage: tauri::State<'_, AppStorage>,
) -> Result<PipelineResult, String> {
    stop_recording_inner(app, storage, None).await
}

pub(crate) async fn stop_recording_inner(
    app: tauri::AppHandle,
    storage: tauri::State<'_, AppStorage>,
    expected: Option<u64>,
) -> Result<PipelineResult, String> {
    let context = {
        let _gate = storage.operation_gate.lock().map_err(|e| e.to_string())?;
        if expected.is_some_and(|id| !crate::dictation::matches_operation(&storage, id)) {
            return Err("This recording is no longer active.".into());
        }
        storage
            .runtime
            .take_recording()
            .ok_or("Recording already stopped; processing may still be running.")?
    };
    let recorder = Arc::clone(&storage.recorder);
    let runtime = Arc::clone(&context.runtime);
    let notifications_enabled = context.config.notifications_enabled;
    let operation_id = context.operation_id.clone();
    runtime.progress("transcribing", "Finishing recording…");
    let result = tauri::async_runtime::spawn_blocking(move || {
        recorder
            .stop()
            .map_err(|e| e.to_string())
            .and_then(|audio| context.process_capture(audio))
    })
    .await
    .map_err(|e| e.to_string())
    .and_then(|result| result);
    crate::dictation::completed(&app, &operation_id, &result).await;
    runtime.finish(&result);
    notify_transcription(&app, notifications_enabled, &result);
    result
}

#[tauri::command]
pub async fn cancel_operation(
    app: tauri::AppHandle,
    storage: tauri::State<'_, AppStorage>,
) -> Result<(), String> {
    cancel_operation_inner(app, storage, None).await
}

pub(crate) async fn cancel_operation_inner(
    app: tauri::AppHandle,
    storage: tauri::State<'_, AppStorage>,
    expected: Option<u64>,
) -> Result<(), String> {
    let (busy, recording) = {
        let _gate = storage.operation_gate.lock().map_err(|e| e.to_string())?;
        if expected.is_some_and(|id| !crate::dictation::matches_operation(&storage, id)) {
            return Ok(());
        }
        crate::dictation::suppress(&storage);
        storage.runtime.cancel_and_take_recording()?
    };
    if !busy {
        return Ok(());
    }
    if let Some(recording) = recording {
        let operation_id = recording.operation_id.clone();
        let recorder = Arc::clone(&storage.recorder);
        let result = tauri::async_runtime::spawn_blocking(move || {
            recorder
                .cancel()
                .map_err(|e| e.to_string())
                .and_then(|()| recording.wait_for_stream_shutdown())
        })
        .await
        .map_err(|e| e.to_string())?;
        crate::dictation::completed(&app, &operation_id, &Err("Recording cancelled.".into())).await;
        storage.runtime.finish(&result);
        result
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod settings_tests {
    use super::*;
    use crate::settings::SettingsStore;
    use std::fs;

    #[test]
    fn explicit_reset_repairs_corruption_even_when_default_shortcut_is_unavailable() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("settings.json");
        let store = SettingsStore::new(path.clone());
        let defaults = AppConfig::default();
        let invalid = AppConfig {
            schema_version: 0,
            ..defaults.clone()
        };
        for bytes in [
            b"broken JSON".to_vec(),
            serde_json::to_vec(&invalid).unwrap(),
        ] {
            fs::write(&path, &bytes).unwrap();
            let mut calls = 0;
            let saved = reset_settings_with_shortcut(&store, |previous, next| {
                calls += 1;
                assert_eq!(
                    previous.map(|s| s.global_shortcut.as_str()),
                    Some(defaults.global_shortcut.as_str())
                );
                assert_eq!(next.global_shortcut, defaults.global_shortcut);
                Err("shortcut in use".into())
            })
            .unwrap();
            assert_eq!(calls, 1);
            assert_eq!(saved, defaults);
            assert_eq!(store.load().unwrap(), defaults);
        }
    }

    #[test]
    fn valid_preferences_and_shortcut_are_preserved_if_reset_registration_fails() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("settings.json");
        let store = SettingsStore::new(path.clone());
        let previous = AppConfig {
            global_shortcut: "CommandOrControl+Shift+D".into(),
            ..AppConfig::default()
        };
        store.save(&previous).unwrap();
        let bytes = fs::read(&path).unwrap();
        let mut attempts = Vec::new();
        assert!(reset_settings_with_shortcut(&store, |old, new| {
            attempts.push((
                old.map(|s| s.global_shortcut.clone()),
                new.global_shortcut.clone(),
            ));
            Err("shortcut in use".into())
        })
        .is_err());
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert_eq!(
            attempts,
            vec![(
                Some(previous.global_shortcut.clone()),
                AppConfig::default().global_shortcut
            )]
        );

        let saved = reset_settings_with_shortcut(&store, |old, new| {
            assert_eq!(
                old.map(|s| s.global_shortcut.as_str()),
                Some(previous.global_shortcut.as_str())
            );
            assert_eq!(new.global_shortcut, AppConfig::default().global_shortcut);
            Ok(())
        })
        .unwrap();
        assert_eq!(saved, AppConfig::default());
        assert_eq!(store.load().unwrap(), saved);
    }

    #[test]
    fn failed_reset_save_restores_previous_shortcut() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("settings.json");
        let store = SettingsStore::new(path.clone());
        let previous = AppConfig {
            global_shortcut: "CommandOrControl+Shift+D".into(),
            ..AppConfig::default()
        };
        store.save(&previous).unwrap();
        let mut attempts = Vec::new();
        let result = reset_settings_with_shortcut(&store, |old, new| {
            attempts.push((
                old.map(|s| s.global_shortcut.clone()),
                new.global_shortcut.clone(),
            ));
            if attempts.len() == 1 {
                // Force an atomic replacement failure without permissions or
                // platform-specific filesystem behavior.
                fs::remove_file(&path).unwrap();
                fs::create_dir(&path).unwrap();
            }
            Ok(())
        });
        assert!(result.unwrap_err().contains("Could not save settings"));
        let default_shortcut = AppConfig::default().global_shortcut;
        assert_eq!(
            attempts,
            vec![
                (
                    Some(previous.global_shortcut.clone()),
                    default_shortcut.clone()
                ),
                (Some(default_shortcut), previous.global_shortcut),
            ]
        );
    }
}

#[cfg(test)]
mod profile_tests {
    use super::*;

    fn profile_json() -> String {
        serde_json::json!({
            "id":"test-cleanup","name":"Test cleanup","repository":"owner/cleanup",
            "revision":"a".repeat(40),"runtime":"llama_cpp","license":"fixture",
            "files":[{"role":"weights","path":"custom.gguf","sha256":"b".repeat(64),"sizeBytes":10}]
        })
        .to_string()
    }

    #[test]
    fn importing_selects_profile_and_roundtrips_settings_without_downloads() {
        let directory = tempfile::tempdir().unwrap();
        let store = crate::settings::SettingsStore::new(directory.path().join("settings.json"));
        let mut config = AppConfig::default();
        apply_model_profile(&mut config, &profile_json()).unwrap();
        assert_eq!(
            config.models.custom_rewriter.as_ref().unwrap().repository,
            "owner/cleanup"
        );
        assert!(config.models.custom_asr.is_none());
        store.save(&config).unwrap();
        assert_eq!(store.load().unwrap(), config);
        let before = config.clone();
        assert!(apply_model_profile(&mut config, "{}").is_err());
        assert_eq!(config, before);
        assert!(apply_model_profile(&mut config, &" ".repeat(32769)).is_err());
        assert_eq!(config, before);
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
    }
}
