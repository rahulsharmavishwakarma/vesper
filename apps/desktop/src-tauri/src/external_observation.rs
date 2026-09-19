//! Optional native insertion observation. External candidates never enter the learner.
use crate::{runtime::PipelineResult, AppStorage};
use std::sync::atomic::Ordering;
use tauri::Manager;
use transcribe_data::{DatasetStore, ExternalShadowRecord};

pub fn cancel(app: &tauri::AppHandle) {
    app.state::<AppStorage>()
        .runtime
        .external_generation
        .fetch_add(1, Ordering::AcqRel);
    let _ = app.run_on_main_thread(transcribe_desktop_platform::cancel_observation);
}
pub async fn reserve(
    app: &tauri::AppHandle,
    result: &PipelineResult,
    text: &str,
) -> Option<ExternalShadowRecord> {
    let storage = app.state::<AppStorage>();
    if !storage.settings.load().ok()?.text_edit_observation {
        return None;
    }
    let root = storage.dataset_dir.clone();
    let context = result.learning_context.clone();
    let text = text.to_owned();
    let session_id = result.session.as_ref().map(|s| s.id.clone());
    tauri::async_runtime::spawn_blocking(move || {
        DatasetStore::open_shadow_completion(root)
            .ok()?
            .reserve_external_shadow(&context, session_id, &text)
            .ok()
            .flatten()
    })
    .await
    .ok()
    .flatten()
}
/// Called on the main thread immediately before paste dispatch.
pub fn arm(app: &tauri::AppHandle, id: u64, reservation: ExternalShadowRecord) {
    let storage = app.state::<AppStorage>();
    if !storage
        .settings
        .load()
        .is_ok_and(|s| s.text_edit_observation)
    {
        return;
    }
    let generation = storage.runtime.external_generation.load(Ordering::Acquire);
    let desktop = app.clone();
    transcribe_desktop_platform::observe_next_paste(id, move |edit| {
        tauri::async_runtime::spawn_blocking(move || {
            let storage = desktop.state::<AppStorage>();
            if storage.runtime.external_generation.load(Ordering::Acquire) != generation
                || storage.runtime.closing.load(Ordering::Acquire)
                || !storage
                    .settings
                    .load()
                    .is_ok_and(|s| s.text_edit_observation)
            {
                return;
            }
            if let Ok(store) = DatasetStore::open_shadow_completion(storage.dataset_dir.clone()) {
                // Exact persisted receipt + learning epoch protect Undo/Forget races.
                let _ = store.finish_external_shadow(&reservation, &edit.before, &edit.after);
            }
        });
    });
}
