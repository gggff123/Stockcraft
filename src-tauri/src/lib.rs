//! Stockcraft Tauri entry point: paths, logging, database, services and the IPC surface.

mod cache;
mod commands;
mod database;
mod downloads;
mod error;
mod models;
mod providers;
mod search;
mod state;
mod util;

use std::sync::{Arc, Mutex, RwLock};

use tauri::Manager;
use tracing_subscriber::EnvFilter;

use crate::cache::ThumbnailCache;
use crate::database::Database;
use crate::downloads::DownloadManager;
use crate::providers::health::HealthRegistry;
use crate::providers::http::HttpClient;
use crate::providers::ProviderRegistry;
use crate::search::SearchService;
use crate::state::{AppPaths, AppState};

const LOG_NAME: &str = "stockcraft.log";

fn resolve_paths(app: &tauri::AppHandle) -> Result<AppPaths, Box<dyn std::error::Error>> {
    let data_dir = app.path().app_data_dir()?;
    let thumbnail_dir = data_dir.join("thumbnails");
    let log_dir = data_dir.join("logs");
    std::fs::create_dir_all(&data_dir)?;
    std::fs::create_dir_all(&thumbnail_dir)?;
    std::fs::create_dir_all(&log_dir)?;
    Ok(AppPaths {
        db_path: data_dir.join("stockcraft.db"),
        data_dir,
        thumbnail_dir,
        log_dir,
    })
}

fn init_logging(paths: &AppPaths) {
    let appender = tracing_appender::rolling::daily(&paths.log_dir, LOG_NAME);
    let (writer, guard) = tracing_appender::non_blocking(appender);
    Box::leak(Box::new(guard));
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("stockcraft_lib=info,warn"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(writer)
        .with_ansi(false)
        .try_init();
    tracing::info!(data_dir = %paths.data_dir.display(), "Stockcraft starting");
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let paths = resolve_paths(app.handle())?;
            init_logging(&paths);
            let db = Arc::new(Database::open(&paths.db_path)?);
            let settings = {
                let mut settings = db.load_settings()?;
                if settings.download_dir.trim().is_empty() {
                    settings.download_dir = downloads::default_download_dir()
                        .to_string_lossy()
                        .to_string();
                    let _ = db.save_settings(&settings);
                }
                settings
            };
            let http = Arc::new(HttpClient::new()?);
            let health = Arc::new(HealthRegistry::new());
            let cache = Arc::new(ThumbnailCache::new(&paths.thumbnail_dir)?);
            let registry = Arc::new(ProviderRegistry::new());
            let settings = Arc::new(RwLock::new(settings));
            let statuses = Arc::new(Mutex::new(std::collections::HashMap::new()));
            let search = Arc::new(SearchService::new(
                app.handle().clone(),
                Arc::clone(&db),
                Arc::clone(&registry),
                Arc::clone(&http),
                Arc::clone(&health),
                Arc::clone(&settings),
                Arc::clone(&statuses),
            ));
            let downloads = Arc::new(DownloadManager::new(
                app.handle().clone(),
                Arc::clone(&db),
                Arc::clone(&settings),
            )?);
            app.manage(AppState {
                db,
                settings,
                registry,
                http,
                health,
                cache,
                search,
                downloads,
                statuses,
                paths,
            });
            Ok(())
        })
        .register_asynchronous_uri_scheme_protocol("thumb", |ctx, request, responder| {
            let uri = request.uri().clone();
            let app = ctx.app_handle().clone();
            tauri::async_runtime::spawn(async move {
                let state = app.state::<AppState>();
                let settings = state.settings_snapshot();
                let result = state
                    .cache
                    .fetch_proxied(uri.path(), uri.query(), &settings)
                    .await;
                let response = match result {
                    Ok((bytes, content_type)) => tauri::http::Response::builder()
                        .status(200)
                        .header("Content-Type", content_type)
                        .header("Cache-Control", "public, max-age=86400")
                        .header("X-Content-Type-Options", "nosniff")
                        .body(bytes)
                        .unwrap_or_else(|_| tauri::http::Response::new(Vec::new())),
                    Err(error) => {
                        tracing::warn!(%error, "thumbnail proxy request failed");
                        tauri::http::Response::builder()
                            .status(404)
                            .header("Cache-Control", "no-store")
                            .body(Vec::new())
                            .unwrap_or_else(|_| tauri::http::Response::new(Vec::new()))
                    }
                };
                responder.respond(response);
            });
        })
        .invoke_handler(tauri::generate_handler![
            commands::start_search,
            commands::cancel_search,
            commands::load_more,
            commands::list_providers,
            commands::provider_status,
            commands::reset_provider_health,
            commands::get_asset_details,
            commands::download_asset,
            commands::cancel_download,
            commands::list_downloads,
            commands::clear_download_history,
            commands::open_path,
            commands::open_external_url,
            commands::validate_external_url,
            commands::pick_folder,
            commands::download_path,
            commands::get_settings,
            commands::update_settings,
            commands::toggle_favorite,
            commands::list_favorites,
            commands::favorite_ids,
            commands::list_library,
            commands::list_collections,
            commands::create_collection,
            commands::rename_collection,
            commands::delete_collection,
            commands::add_to_collection,
            commands::remove_from_collection,
            commands::search_history,
            commands::clear_search_history,
            commands::delete_history_item,
            commands::clear_cache,
            commands::cache_stats,
            commands::get_thumbnail_base,
            commands::get_diagnostics,
            commands::enabled_providers,
        ])
        .run(tauri::generate_context!())
        .expect("Stockcraft failed to start");
}
