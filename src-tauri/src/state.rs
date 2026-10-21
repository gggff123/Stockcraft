//! Shared application state managed by Tauri and handed to every command.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};

use crate::cache::ThumbnailCache;
use crate::database::Database;
use crate::downloads::DownloadManager;
use crate::models::{AppSettings, ProviderInfo, ProviderState, ProviderStatusInfo};
use crate::providers::health::HealthRegistry;
use crate::providers::http::HttpClient;
use crate::providers::ProviderRegistry;
use crate::search::SearchService;

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub data_dir: PathBuf,
    pub db_path: PathBuf,
    pub thumbnail_dir: PathBuf,
    pub log_dir: PathBuf,
}

impl AppPaths {
    pub fn log_path(&self) -> PathBuf {
        // The tracing appender rotates daily; diagnostics point at the most recent file.
        let mut newest: Option<(std::time::SystemTime, PathBuf)> = None;
        if let Ok(entries) = std::fs::read_dir(&self.log_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path
                    .file_name()
                    .map(|name| name.to_string_lossy().starts_with("stockcraft.log"))
                    .unwrap_or(false)
                {
                    continue;
                }
                if let Ok(modified) = entry.metadata().and_then(|metadata| metadata.modified()) {
                    if newest
                        .as_ref()
                        .map(|(time, _)| modified > *time)
                        .unwrap_or(true)
                    {
                        newest = Some((modified, path));
                    }
                }
            }
        }
        newest
            .map(|(_, path)| path)
            .unwrap_or_else(|| self.log_dir.join("stockcraft.log"))
    }
}

pub struct AppState {
    pub db: Arc<Database>,
    pub settings: Arc<RwLock<AppSettings>>,
    pub registry: Arc<ProviderRegistry>,
    pub http: Arc<HttpClient>,
    pub health: Arc<HealthRegistry>,
    pub cache: Arc<ThumbnailCache>,
    pub search: Arc<SearchService>,
    pub downloads: Arc<DownloadManager>,
    pub statuses: Arc<Mutex<std::collections::HashMap<String, ProviderStatusInfo>>>,
    pub paths: AppPaths,
}

impl AppState {
    pub fn settings_snapshot(&self) -> AppSettings {
        self.settings
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    pub fn provider_infos(&self) -> Vec<ProviderInfo> {
        self.registry.infos(&self.settings_snapshot())
    }

    pub fn provider_statuses(&self) -> Vec<ProviderStatusInfo> {
        let settings = self.settings_snapshot();
        let stored = self
            .statuses
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        let enabled = self.registry.enabled(&settings);
        let enabled_ids: std::collections::HashSet<&str> =
            enabled.iter().map(|provider| provider.id()).collect();
        self.registry
            .all()
            .iter()
            .map(|provider| {
                let health = self.health.snapshot(provider.id());
                let mut status = stored.get(provider.id()).cloned().unwrap_or_else(|| {
                    let mut status = ProviderStatusInfo::new(
                        provider.id(),
                        provider.name(),
                        ProviderState::Idle,
                    );
                    status.message = "ready".into();
                    status
                });
                if !enabled_ids.contains(provider.id()) {
                    status.state = ProviderState::Disabled;
                    status.message = if provider.requires_api_key()
                        && provider
                            .api_key(&settings)
                            .map(|key| key.trim().is_empty())
                            .unwrap_or(false)
                    {
                        "add an API key in Settings → Sources".into()
                    } else {
                        "disabled in Settings → Sources".into()
                    };
                }
                status.failures = health.failures;
                status.blocked = health.blocked || status.blocked;
                if health.open {
                    status.circuit_open_until_ms = health.circuit_open_until_ms();
                    if status.state == ProviderState::Idle {
                        status.state = ProviderState::CircuitOpen;
                        status.message = health
                            .gate_message()
                            .unwrap_or_else(|| "circuit open".into());
                    }
                } else {
                    status.circuit_open_until_ms = None;
                }
                if status.state == ProviderState::Idle && health.last_success_ms.is_some() {
                    status.last_success_ms = health.last_success_ms;
                }
                status
            })
            .collect()
    }
}
/// Drop derived idle/disabled entries after settings change so the next status query rebuilds them
/// from the current provider list. Real outcomes (errors, rate limits) are preserved.
pub fn invalidate_default_statuses(state: &AppState) {
    let mut stored = state
        .statuses
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    stored
        .retain(|_, status| !matches!(status.state, ProviderState::Idle | ProviderState::Disabled));
}
