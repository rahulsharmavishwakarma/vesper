mod commands;
mod diagnostics;
mod dictation;
mod menu_bar;
mod onboarding;
mod resources;
mod runtime;
mod settings;
mod shortcuts;
mod storage;
mod updates;

use std::path::PathBuf;
use std::sync::Arc;
use tauri::Manager;
use tauri_plugin_global_shortcut::{Code, ShortcutState};

#[cfg(debug_assertions)]
fn development_model_root() -> Option<std::ffi::OsString> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../models/downloads");
    path.is_dir().then(|| path.into_os_string())
}

#[cfg(not(debug_assertions))]
fn development_model_root() -> Option<std::ffi::OsString> {
    None
}

pub struct AppStorage {
    settings: settings::SettingsStore,
    dataset_dir: PathBuf,
    model_dir: PathBuf,
    runtime: Arc<runtime::Runtime>,
    recorder: Arc<vesper_audio::AudioRecorder>,
    inference: Arc<vesper_inference::InferenceManager>,
    resources: Arc<std::sync::Mutex<resources::ResourceSampler>>,
    operation_gate: std::sync::Mutex<()>,
    shortcut_status: std::sync::Mutex<shortcuts::ShortcutStatus>,
    shortcut_recorder: std::sync::Mutex<shortcuts::Recorder>,
    dictation: dictation::Dictation,
    onboarding: onboarding::OnboardingStore,
}

pub fn replace_global_shortcut(
    app: &tauri::AppHandle,
    previous: Option<&vesper_core::AppConfig>,
    next: &vesper_core::AppConfig,
) -> Result<(), String> {
    let previous = previous
        .map(|config| shortcuts::canonical_bindings(config.recording_shortcuts()))
        .transpose()?;
    let previous = previous
        .as_ref()
        .map(|keys| keys.each_ref().map(String::as_str));
    let next = shortcuts::canonical_bindings(next.recording_shortcuts())?;
    let result = app
        .state::<AppStorage>()
        .shortcut_status
        .lock()
        .map_err(|e| e.to_string())?
        .replace(
            app,
            previous.as_ref().map_or(&[], |keys| keys.as_slice()),
            &next.each_ref().map(String::as_str),
        );
    if result.is_ok() {
        dictation::reset_shortcut_keys(app);
    }
    result
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        // Update backend (#65): initialized but inert. No updater endpoint or
        // public key is configured until key enrollment, and the frontend
        // makes no updater calls until the update UI (#66) lands.
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if shortcuts::capture_event(app, shortcut, event.state) {
                        return;
                    }
                    if shortcut.key == Code::Escape {
                        if event.state == ShortcutState::Pressed {
                            dictation::cancel(app);
                        }
                    } else {
                        dictation::shortcut(app, shortcut, event.state);
                    }
                })
                .build(),
        )
        .setup(|app| {
            let config_root = storage::compatible_app_root(app.path().app_config_dir()?)?;
            let data_root = storage::compatible_app_root(app.path().app_data_dir()?)?;
            let model_dir = storage::model_root(
                &data_root,
                vesper_core::environment::nonempty_var_os("VESPER_MODEL_DIR"),
                std::env::var_os("UKTI_MODEL_DIR"),
                std::env::var_os("STT_MODEL_DIR").or_else(development_model_root),
            );
            // Decide first-run state before anything below writes into the
            // profile (the speaker runtime install fills the model folder).
            let onboarding = onboarding::OnboardingStore::open(
                config_root.join("onboarding.json"),
                onboarding::existing_profile(
                    &config_root.join("settings.json"),
                    &model_dir,
                    &data_root.join("dataset"),
                ),
            );
            // First-run pre-check: install the bundled speaker runtime when it
            // is missing so fresh installs can produce speaker timelines. The
            // weights still download through the regular audio setup flow.
            // Never fatal: without the stage the app runs normally.
            match vesper_models::diarization::ensure_bundled_runtime(
                &model_dir,
                &std::sync::atomic::AtomicBool::new(false),
            ) {
                Ok(true) => {}
                Ok(false) if vesper_models::diarization::supported() => {
                    eprintln!("speaker runtime unavailable: no bundled runtime for this build")
                }
                Ok(false) => {}
                Err(error) => eprintln!("speaker runtime install failed: {error}"),
            }
            let settings = settings::SettingsStore::new(config_root.join("settings.json"));
            let shortcut = settings.startup_config();
            app.manage(AppStorage {
                settings,
                dataset_dir: data_root.join("dataset"),
                model_dir,
                runtime: Arc::new(runtime::Runtime::default()),
                recorder: Arc::new(vesper_audio::AudioRecorder::new()?),
                inference: Arc::new(vesper_inference::InferenceManager::new()?),
                resources: Arc::new(std::sync::Mutex::new(resources::ResourceSampler::default())),
                operation_gate: std::sync::Mutex::new(()),
                shortcut_status: std::sync::Mutex::new(shortcuts::ShortcutStatus::default()),
                shortcut_recorder: std::sync::Mutex::new(shortcuts::Recorder::default()),
                dictation: dictation::Dictation::default(),
                onboarding,
            });
            if let Some(window) = app.get_webview_window("quick-capture") {
                vesper_desktop_platform::initialize(&window).map_err(std::io::Error::other)?;
            }
            let capture_design = app
                .state::<AppStorage>()
                .settings
                .load()
                .unwrap_or_default()
                .capture_design;
            dictation::apply_capture_design(app.handle(), capture_design);
            dictation::apply_output(app.handle());
            menu_bar::setup(app.handle())?;
            let _ = replace_global_shortcut(app.handle(), None, &shortcut);
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == "main"
                && matches!(
                    event,
                    tauri::WindowEvent::Focused(false)
                        | tauri::WindowEvent::CloseRequested { .. }
                        | tauri::WindowEvent::Destroyed
                )
            {
                if let Some(storage) = window.try_state::<AppStorage>() {
                    if let Ok(mut recorder) = storage.shortcut_recorder.lock() {
                        recorder.cancel();
                    }
                }
            }
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                // Keep both WebViews alive for the menu and global shortcut. Closing
                // a window must not cancel a recording or discard an unsaved draft.
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            updates::check_prerelease_update,
            dictation::get_dictation_status,
            dictation::dictation_permissions,
            dictation::toggle_dictation,
            dictation::cancel_dictation,
            dictation::hide_dictation,
            dictation::start_capture_drag,
            dictation::copy_dictation,
            menu_bar::take_menu_navigation,
            onboarding::get_onboarding,
            onboarding::save_onboarding_step,
            onboarding::complete_onboarding,
            commands::get_app_info,
            commands::read_project_application,
            commands::resolve_project_folder,
            commands::get_app_status,
            commands::get_engine_diagnostics,
            commands::get_accelerator_capabilities,
            commands::get_model_catalog,
            commands::get_adapter_registry,
            commands::get_selected_adapters,
            commands::get_personalization,
            commands::get_automatic_learning,
            commands::get_learning_shadow_diagnostics,
            commands::forget_automatic_learning,
            commands::suspend_automatic_candidate,
            commands::approve_automatic_candidate,
            commands::edit_latest_draft,
            commands::undo_latest_learning,
            commands::save_personalization,
            commands::remember_cleanup,
            commands::get_correction_suggestions,
            commands::remember_correction,
            commands::get_learning_dashboard,
            commands::undo_personalization,
            commands::undo_unsaved_personalization,
            commands::get_settings,
            commands::retry_global_shortcut,
            shortcuts::begin_shortcut_recording,
            shortcuts::end_shortcut_recording,
            commands::save_settings,
            commands::reset_settings,
            commands::import_model_profile,
            commands::clear_model_profile,
            commands::list_sessions,
            commands::import_audio,
            commands::approve_correction,
            commands::approve_correction_and_learn,
            commands::approve_like,
            commands::retract_approval,
            commands::session_audio_data_url,
            commands::approve_transcription,
            commands::set_transcript_feedback,
            commands::export_dataset,
            commands::get_runtime_status,
            commands::get_audio_features,
            commands::get_latest_result,
            commands::list_audio_devices,
            commands::get_model_installation,
            commands::list_model_installations,
            commands::install_models,
            commands::install_model,
            commands::remove_model,
            commands::start_recording,
            commands::stop_recording,
            commands::cancel_operation,
            commands::transcribe_file,
        ])
        .build(tauri::generate_context!())
        .expect("failed to build the Vesper desktop application")
        .run(|app, event| match event {
            tauri::RunEvent::ExitRequested { api, .. } => {
                if menu_bar::prevent_exit(app) {
                    api.prevent_exit();
                }
            }
            #[cfg(target_os = "macos")]
            tauri::RunEvent::Reopen { .. } => {
                let _ = menu_bar::show_main(app, None);
            }
            tauri::RunEvent::Exit => {
                vesper_desktop_platform::shutdown();
                menu_bar::stop(app);
            }
            _ => {}
        });
}
