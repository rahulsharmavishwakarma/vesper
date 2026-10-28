//! Settings commands for automatic workspaces. Detection itself runs at
//! shortcut capture start in `dictation.rs`; these commands only list and
//! manage visible workspaces. Every store call runs off the main thread.
use crate::AppStorage;
use vesper_data::{DatasetStore, WorkspaceSummary, WorkspaceUpdate};

async fn with_store<T: Send + 'static>(
    storage: tauri::State<'_, AppStorage>,
    work: impl FnOnce(DatasetStore) -> vesper_data::Result<T> + Send + 'static,
) -> Result<T, String> {
    let root = storage.dataset_dir.clone();
    tauri::async_runtime::spawn_blocking(move || {
        DatasetStore::open(root)
            .and_then(work)
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn list_workspaces(
    storage: tauri::State<'_, AppStorage>,
) -> Result<Vec<WorkspaceSummary>, String> {
    with_store(storage, |store| store.list_workspaces()).await
}

#[tauri::command]
pub async fn rename_workspace(
    key: String,
    label: String,
    storage: tauri::State<'_, AppStorage>,
) -> Result<WorkspaceUpdate, String> {
    with_store(storage, move |store| store.rename_workspace(&key, &label)).await
}

#[tauri::command]
pub async fn merge_workspace(
    from: String,
    into: String,
    storage: tauri::State<'_, AppStorage>,
) -> Result<WorkspaceUpdate, String> {
    with_store(storage, move |store| store.merge_workspace(&from, &into)).await
}

#[tauri::command]
pub async fn delete_workspace(
    key: String,
    storage: tauri::State<'_, AppStorage>,
) -> Result<WorkspaceUpdate, String> {
    with_store(storage, move |store| store.delete_workspace(&key)).await
}
