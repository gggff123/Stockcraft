//! Tauri command surface. Every command validates its inputs and returns `AppResult<T>`.

use std::path::{Path, PathBuf};

use tauri::{AppHandle, Manager, State};

use crate::database::apply_settings_patch;
use crate::error::{AppError, AppResult};
use crate::models::{
    AppSettings, Asset, CacheStats, Collection, Diagnostics, DownloadRecord, HistoryItem,
    ProviderDiagnostic, ProviderInfo, ProviderState, ProviderStatusInfo, SearchRequest,
    SearchStart,
};
use crate::state::AppState;
use crate::util::{path_within, validate_remote_url};

/* ------------------------------------------------------------------- search */

#[tauri::command]
pub async fn start_search(
    state: State<'_, AppState>,
    request: SearchRequest,
) -> AppResult<SearchStart> {
    state.search.start(request)
}

#[tauri::command]
pub async fn cancel_search(state: State<'_, AppState>, search_id: String) -> AppResult<()> {
    state.search.cancel(&search_id).await
}

#[tauri::command]
pub async fn load_more(
    state: State<'_, AppState>,
    search_id: String,
    provider: String,
) -> AppResult<ProviderStatusInfo> {
    state.search.load_more(&search_id, &provider).await
}

/* ---------------------------------------------------------------- providers */

#[tauri::command]
pub fn list_providers(state: State<'_, AppState>) -> Vec<ProviderInfo> {
    state.provider_infos()
}

#[tauri::command]
pub fn provider_status(state: State<'_, AppState>) -> Vec<ProviderStatusInfo> {
    state.provider_statuses()
}

#[tauri::command]
pub fn reset_provider_health(
    state: State<'_, AppState>,
    provider: Option<String>,
) -> Vec<ProviderStatusInfo> {
    state.health.reset(provider.as_deref());
    {
        let mut stored = state
            .statuses
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        match provider {
            Some(provider) => {
                stored.remove(&provider);
            }
            None => stored.clear(),
        }
    }
    crate::state::invalidate_default_statuses(&state);
    state.provider_statuses()
}

#[tauri::command]
pub async fn get_asset_details(
    state: State<'_, AppState>,
    provider: String,
    asset_id: String,
) -> AppResult<Asset> {
    let asset_id = asset_id.trim();
    if asset_id.is_empty() {
        return Err(AppError::validation("asset id is required"));
    }
    let canonical = if asset_id.contains(':') {
        asset_id.to_string()
    } else {
        format!("{provider}:{asset_id}")
    };
    if let Some(asset) = state.db.get_saved_asset(&canonical)? {
        return Ok(asset);
    }
    let adapter = state
        .registry
        .get(&provider)
        .ok_or_else(|| AppError::NotFound(format!("unknown provider: {provider}")))?;
    let ctx = crate::providers::ProviderContext {
        http: std::sync::Arc::clone(&state.http),
        settings: state.settings_snapshot(),
    };
    adapter.details(&ctx, asset_id).await
}

/* ---------------------------------------------------------------- downloads */

#[tauri::command]
pub async fn download_asset(
    state: State<'_, AppState>,
    asset: Asset,
    target_dir: Option<String>,
) -> AppResult<i64> {
    state.downloads.start(&asset, target_dir)
}

#[tauri::command]
pub fn cancel_download(state: State<'_, AppState>, download_id: i64) -> AppResult<()> {
    state.downloads.cancel(download_id)
}

#[tauri::command]
pub fn list_downloads(
    state: State<'_, AppState>,
    limit: Option<i64>,
) -> AppResult<Vec<DownloadRecord>> {
    let mut records = state.db.list_downloads(limit.unwrap_or(60))?;
    let settings = state.settings_snapshot();
    if settings.cache_thumbnails {
        let base = crate::cache::ThumbnailCache::base_url();
        for record in &mut records {
            if let Some(url) = record.thumbnail_url.as_deref() {
                if !url.starts_with(base) && validate_remote_url(url).is_ok() {
                    record.thumbnail_url = Some(state.cache.proxy_url(url));
                }
            }
        }
    }
    Ok(records)
}

#[tauri::command]
pub fn clear_download_history(state: State<'_, AppState>) -> AppResult<i64> {
    Ok(state.db.clear_downloads()? as i64)
}

#[tauri::command]
pub fn download_path(state: State<'_, AppState>, download_id: i64) -> AppResult<String> {
    Ok(state.db.get_download(download_id)?.file_path)
}

#[tauri::command]
pub fn open_path(state: State<'_, AppState>, path: String) -> AppResult<()> {
    let candidate = PathBuf::from(path.trim());
    if path.trim().is_empty() || !candidate.exists() {
        return Err(AppError::NotFound("file or folder does not exist".into()));
    }
    if !is_managed_path(&state, &candidate)? {
        return Err(AppError::Policy(
            "Stockcraft only opens files in its download folder".into(),
        ));
    }
    let target = if candidate.is_dir() {
        candidate
    } else {
        candidate
            .parent()
            .map(Path::to_path_buf)
            .ok_or_else(|| AppError::NotFound("file has no parent folder".into()))?
    };
    opener::open(target).map_err(|error| AppError::Io(format!("could not open folder: {error}")))
}

fn is_managed_path(state: &AppState, candidate: &Path) -> AppResult<bool> {
    let settings = state.settings_snapshot();
    let root = crate::downloads::resolve_download_dir(None, &settings)?;
    let default_root = crate::downloads::default_download_dir();
    if path_within(candidate, &root) || path_within(candidate, &default_root) {
        return Ok(true);
    }
    let canonical = candidate.canonicalize()?;
    for record in state.db.list_downloads(500)? {
        let recorded = PathBuf::from(&record.file_path);
        if recorded == candidate
            || recorded
                .canonicalize()
                .map(|path| path == canonical)
                .unwrap_or(false)
        {
            return Ok(true);
        }
    }
    Ok(false)
}

#[tauri::command]
pub fn open_external_url(url: String) -> AppResult<()> {
    let url = validate_remote_url(&url)?;
    opener::open(url.as_str())
        .map_err(|error| AppError::Io(format!("could not open link: {error}")))
}

#[tauri::command]
pub fn validate_external_url(url: String) -> bool {
    validate_remote_url(&url).is_ok()
}

#[tauri::command]
pub async fn pick_folder(title: Option<String>) -> AppResult<Option<String>> {
    let title = title.unwrap_or_else(|| "Choose a folder".into());
    let picked = tauri::async_runtime::spawn_blocking(move || {
        rfd::FileDialog::new()
            .set_title(title)
            .pick_folder()
            .map(|path| path.to_string_lossy().to_string())
    })
    .await
    .map_err(|error| AppError::internal(format!("folder picker failed: {error}")))?;
    Ok(picked)
}

/* ----------------------------------------------------------------- settings */

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> AppSettings {
    state.settings_snapshot()
}

#[tauri::command]
pub fn update_settings(
    state: State<'_, AppState>,
    patch: serde_json::Value,
) -> AppResult<AppSettings> {
    let known = state.registry.ids();
    let mut settings = state.settings_snapshot();
    apply_settings_patch(&mut settings, &patch, &known)?;
    if !settings.download_dir.trim().is_empty() {
        let dir = PathBuf::from(settings.download_dir.trim());
        std::fs::create_dir_all(&dir).map_err(|error| {
            AppError::validation(format!("could not use that download folder: {error}"))
        })?;
        if !dir.is_dir() {
            return Err(AppError::validation("download folder is not a directory"));
        }
    }
    state.db.save_settings(&settings)?;
    *state
        .settings
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = settings.clone();
    crate::state::invalidate_default_statuses(&state);
    Ok(settings)
}

/* ------------------------------------------------------------------ library */

#[tauri::command]
pub fn toggle_favorite(state: State<'_, AppState>, asset: Asset) -> AppResult<bool> {
    state.db.toggle_favorite(&asset)
}

#[tauri::command]
pub fn list_favorites(
    state: State<'_, AppState>,
    query: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
) -> AppResult<Vec<Asset>> {
    state.db.list_favorites(
        query.as_deref().unwrap_or(""),
        limit.unwrap_or(200),
        offset.unwrap_or(0),
    )
}

#[tauri::command]
pub fn favorite_ids(state: State<'_, AppState>) -> AppResult<Vec<String>> {
    state.db.favorite_ids()
}

#[tauri::command]
pub fn list_library(
    state: State<'_, AppState>,
    query: Option<String>,
    collection_id: Option<i64>,
    limit: Option<i64>,
    offset: Option<i64>,
) -> AppResult<Vec<Asset>> {
    state.db.list_library(
        query.as_deref().unwrap_or(""),
        collection_id,
        limit.unwrap_or(200),
        offset.unwrap_or(0),
    )
}

#[tauri::command]
pub fn list_collections(state: State<'_, AppState>) -> AppResult<Vec<Collection>> {
    state.db.list_collections()
}

#[tauri::command]
pub fn create_collection(
    state: State<'_, AppState>,
    name: String,
    description: Option<String>,
) -> AppResult<Collection> {
    state
        .db
        .create_collection(&name, description.as_deref().unwrap_or(""))
}

#[tauri::command]
pub fn rename_collection(
    state: State<'_, AppState>,
    id: i64,
    name: String,
    description: Option<String>,
) -> AppResult<Collection> {
    state
        .db
        .rename_collection(id, &name, description.as_deref().unwrap_or(""))
}

#[tauri::command]
pub fn delete_collection(state: State<'_, AppState>, id: i64) -> AppResult<()> {
    state.db.delete_collection(id)
}

#[tauri::command]
pub fn add_to_collection(
    state: State<'_, AppState>,
    collection_id: i64,
    asset: Asset,
) -> AppResult<Collection> {
    state.db.add_to_collection(collection_id, &asset)
}

#[tauri::command]
pub fn remove_from_collection(
    state: State<'_, AppState>,
    collection_id: i64,
    asset_id: String,
) -> AppResult<Collection> {
    state.db.remove_from_collection(collection_id, &asset_id)
}

/* ------------------------------------------------------------------ history */

#[tauri::command]
pub fn search_history(
    state: State<'_, AppState>,
    limit: Option<i64>,
) -> AppResult<Vec<HistoryItem>> {
    state.db.list_history(limit.unwrap_or(12))
}

#[tauri::command]
pub fn clear_search_history(state: State<'_, AppState>) -> AppResult<()> {
    state.db.clear_history()
}

#[tauri::command]
pub fn delete_history_item(state: State<'_, AppState>, id: i64) -> AppResult<()> {
    state.db.delete_history_item(id)
}

/* -------------------------------------------------------------------- cache */

#[tauri::command]
pub fn clear_cache(state: State<'_, AppState>, scope: String) -> AppResult<u64> {
    let freed = match scope.as_str() {
        "thumbnails" => state.cache.clear(),
        "metadata" => state.db.clear_metadata()?,
        "all" => state.cache.clear() + state.db.clear_metadata()?,
        other => {
            return Err(AppError::validation(format!(
                "unknown cache scope: {other}"
            )))
        }
    };
    Ok(freed)
}

#[tauri::command]
pub fn cache_stats(state: State<'_, AppState>) -> AppResult<CacheStats> {
    let mut stats = state.db.cache_stats()?;
    let (bytes, files) = state.cache.stats();
    stats.thumbnail_bytes = bytes;
    stats.thumbnail_files = files;
    Ok(stats)
}

#[tauri::command]
pub fn get_thumbnail_base(state: State<'_, AppState>) -> String {
    if state.settings_snapshot().cache_thumbnails {
        crate::cache::ThumbnailCache::base_url().to_string()
    } else {
        String::new()
    }
}

#[tauri::command]
pub fn get_diagnostics(app: AppHandle, state: State<'_, AppState>) -> AppResult<Diagnostics> {
    let mut counts = state.db.cache_stats()?;
    let (bytes, files) = state.cache.stats();
    counts.thumbnail_bytes = bytes;
    counts.thumbnail_files = files;
    let providers = state
        .provider_statuses()
        .into_iter()
        .map(|status| ProviderDiagnostic {
            id: status.provider,
            name: status.name,
            state: status.state,
            message: status.message,
            failures: status.failures,
        })
        .collect();
    Ok(Diagnostics {
        app_version: app.package_info().version.to_string(),
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        data_dir: state.paths.data_dir.to_string_lossy().to_string(),
        db_path: state.paths.db_path.to_string_lossy().to_string(),
        thumbnail_dir: state.paths.thumbnail_dir.to_string_lossy().to_string(),
        log_path: state.paths.log_path().to_string_lossy().to_string(),
        providers,
        counts,
    })
}

/// Small helper for the UI: the providers that currently receive queries.
#[tauri::command]
pub fn enabled_providers(state: State<'_, AppState>) -> Vec<String> {
    state
        .registry
        .enabled(&state.settings_snapshot())
        .iter()
        .map(|provider| provider.id().to_string())
        .collect()
}

#[allow(dead_code)]
fn default_state_label(state: ProviderState) -> &'static str {
    state.as_str()
}

#[allow(dead_code)]
fn app_data_dir(app: &AppHandle) -> AppResult<PathBuf> {
    app.path()
        .app_data_dir()
        .map_err(|error| AppError::Io(format!("could not locate app data directory: {error}")))
}
