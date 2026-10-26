//! Commands for the "Words Vesper found" inventory view. Spellings, counts and
//! folder-name labels only; no paths or document text cross this boundary.
use tauri::State;
use vesper_core::TermSource;
use vesper_data::{DatasetStore, TermInventorySummary};

use crate::AppStorage;

const ROW_LIMIT: usize = 200;

async fn with_store<T: Send + 'static>(
    storage: &State<'_, AppStorage>,
    f: impl FnOnce(DatasetStore) -> vesper_data::Result<T> + Send + 'static,
) -> Result<T, String> {
    let root = storage.dataset_dir.clone();
    tauri::async_runtime::spawn_blocking(move || {
        DatasetStore::open(root)
            .and_then(f)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn get_term_inventory(
    storage: State<'_, AppStorage>,
) -> Result<TermInventorySummary, String> {
    with_store(&storage, |store| store.term_inventory_summary(ROW_LIMIT)).await
}

/// Tombstone: the word is never re-added by mining.
#[tauri::command]
pub async fn remove_inventory_term(
    id: i64,
    storage: State<'_, AppStorage>,
) -> Result<TermInventorySummary, String> {
    with_store(&storage, move |store| {
        store.remove_inventory_term(id)?;
        store.term_inventory_summary(ROW_LIMIT)
    })
    .await
}

/// Forget every word from one source and pause that source.
#[tauri::command]
pub async fn forget_term_source(
    source: TermSource,
    storage: State<'_, AppStorage>,
) -> Result<TermInventorySummary, String> {
    with_store(&storage, move |store| {
        store.forget_term_source(source)?;
        store.term_inventory_summary(ROW_LIMIT)
    })
    .await
}

#[tauri::command]
pub async fn resume_term_source(
    source: TermSource,
    storage: State<'_, AppStorage>,
) -> Result<TermInventorySummary, String> {
    let summary = with_store(&storage, move |store| {
        store.resume_term_source(source)?;
        store.term_inventory_summary(ROW_LIMIT)
    })
    .await?;
    storage.term_worker.wake();
    Ok(summary)
}
