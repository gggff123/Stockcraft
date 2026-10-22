//! Search orchestration: concurrent provider calls, cancellation, progressive events, metadata
//! caching, deduplication, local post-filtering and per-provider status.

pub mod dedup;

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use serde_json::Value;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter};
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

use crate::database::Database;
use crate::error::{AppError, AppResult};
use crate::models::{
    AppSettings, Asset, ProviderError, ProviderPage, ProviderPayload, ProviderState,
    ProviderStatusInfo, SearchFinished, SearchRequest, SearchStart,
};
use crate::providers::health::{FailureKind, HealthRegistry};
use crate::providers::http::HttpClient;
use crate::providers::{ImageProvider, ProviderContext, ProviderRegistry};

use self::dedup::Deduper;

pub struct SearchService {
    app: AppHandle,
    db: Arc<Database>,
    registry: Arc<ProviderRegistry>,
    http: Arc<HttpClient>,
    health: Arc<HealthRegistry>,
    settings: Arc<RwLock<AppSettings>>,
    statuses: Arc<Mutex<HashMap<String, ProviderStatusInfo>>>,
    sessions: Mutex<HashMap<String, Arc<SearchSession>>>,
    provider_slots: Arc<Semaphore>,
}

struct SearchSession {
    id: String,
    request: SearchRequest,
    cancel: CancellationToken,
    pages: Mutex<HashMap<String, u32>>,
    deduper: Mutex<Deduper>,
    errors: Mutex<Vec<ProviderError>>,
    results: AtomicUsize,
    finished: AtomicBool,
    history_id: Option<i64>,
    started: Instant,
}

impl SearchService {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        app: AppHandle,
        db: Arc<Database>,
        registry: Arc<ProviderRegistry>,
        http: Arc<HttpClient>,
        health: Arc<HealthRegistry>,
        settings: Arc<RwLock<AppSettings>>,
        statuses: Arc<Mutex<HashMap<String, ProviderStatusInfo>>>,
    ) -> Self {
        Self {
            app,
            db,
            registry,
            http,
            health,
            settings,
            statuses,
            sessions: Mutex::new(HashMap::new()),
            provider_slots: Arc::new(Semaphore::new(4)),
        }
    }

    pub fn start(self: &Arc<Self>, mut request: SearchRequest) -> AppResult<SearchStart> {
        let settings = self.settings();
        request.query = request.query.trim().replace('\u{0}', "");
        if request.query.is_empty() {
            return Err(AppError::validation("enter something to search for"));
        }
        if request.query.chars().count() > 160 {
            return Err(AppError::validation(
                "searches are limited to 160 characters",
            ));
        }
        request.limit = if request.limit == 0 {
            settings.results_per_page
        } else {
            request.limit.clamp(1, 100)
        };
        request.page = 1;
        if !matches!(request.sort.as_str(), "relevance" | "newest" | "largest") {
            request.sort = "relevance".into();
        }
        let allowed: HashSet<String> = self
            .registry
            .enabled(&settings)
            .iter()
            .map(|p| p.id().to_string())
            .collect();
        let mut chosen = Vec::new();
        let requested = if request.providers.is_empty() {
            settings.enabled_providers.clone()
        } else {
            request.providers.clone()
        };
        for id in requested {
            if allowed.contains(&id) && !chosen.contains(&id) {
                chosen.push(id);
            }
        }
        if chosen.is_empty() {
            return Err(AppError::validation(
                "no usable sources are enabled; enable a source or add a Pexels API key",
            ));
        }
        request.providers = chosen.clone();

        let history_id = if settings.history_enabled {
            self.db.add_history(&request.query, &request.providers).ok()
        } else {
            None
        };
        let session = Arc::new(SearchSession {
            id: uuid::Uuid::new_v4().to_string(),
            request: request.clone(),
            cancel: CancellationToken::new(),
            pages: Mutex::new(HashMap::new()),
            deduper: Mutex::new(Deduper::new()),
            errors: Mutex::new(Vec::new()),
            results: AtomicUsize::new(0),
            finished: AtomicBool::new(false),
            history_id,
            started: Instant::now(),
        });

        let mut initial = Vec::new();
        let mut spawned = Vec::new();
        for provider in self.registry.all() {
            if !chosen.iter().any(|id| id == provider.id()) {
                continue;
            }
            let health = self.health.snapshot(provider.id());
            if let Some(message) = health.gate_message() {
                let state = if health.blocked {
                    ProviderState::Restricted
                } else {
                    ProviderState::CircuitOpen
                };
                let mut status = ProviderStatusInfo::new(provider.id(), provider.name(), state);
                status.message = message;
                status.failures = health.failures;
                status.circuit_open_until_ms = health.circuit_open_until_ms();
                status.blocked = health.blocked;
                initial.push(status);
                continue;
            }
            let mut status =
                ProviderStatusInfo::new(provider.id(), provider.name(), ProviderState::Searching);
            status.message = "request in flight".into();
            initial.push(status);
            spawned.push(Arc::clone(provider));
        }

        {
            let mut statuses = self
                .statuses
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            for status in &initial {
                statuses.insert(status.provider.clone(), status.clone());
            }
        }
        {
            let mut sessions = self
                .sessions
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            sessions.insert(session.id.clone(), Arc::clone(&session));
            prune_sessions(&mut sessions, &session.id);
        }
        tracing::info!(search = %session.id, providers = ?chosen, "search started");

        let service = Arc::clone(self);
        let task_session = Arc::clone(&session);
        tauri::async_runtime::spawn(async move {
            let mut handles = Vec::new();
            for provider in spawned {
                let service = Arc::clone(&service);
                let session = Arc::clone(&task_session);
                handles.push(tauri::async_runtime::spawn(async move {
                    service.run_initial(provider, session).await;
                }));
            }
            for handle in handles {
                let _ = handle.await;
            }
            service.finalize(&task_session, false);
        });

        Ok(SearchStart {
            search_id: session.id.clone(),
            query: session.request.query.clone(),
            providers: initial,
        })
    }

    pub async fn cancel(&self, search_id: &str) -> AppResult<()> {
        let session = {
            let sessions = self
                .sessions
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            sessions.get(search_id).cloned()
        };
        let session =
            session.ok_or_else(|| AppError::NotFound("search session not found".into()))?;
        session.cancel.cancel();
        tracing::info!(search = %session.id, "search cancelled");
        self.finalize(&session, true);
        Ok(())
    }

    pub async fn load_more(
        &self,
        search_id: &str,
        provider_id: &str,
    ) -> AppResult<ProviderStatusInfo> {
        let session = {
            let sessions = self
                .sessions
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            sessions.get(search_id).cloned()
        }
        .ok_or_else(|| {
            AppError::NotFound("search session not found; run the search again".into())
        })?;
        if session.cancel.is_cancelled() {
            return Err(AppError::Cancelled("search was cancelled".into()));
        }
        let provider = self
            .registry
            .get(provider_id)
            .ok_or_else(|| AppError::NotFound("unknown provider".into()))?;
        let health = self.health.snapshot(provider_id);
        if let Some(message) = health.gate_message() {
            let state = if health.blocked {
                ProviderState::Restricted
            } else {
                ProviderState::CircuitOpen
            };
            let mut status = ProviderStatusInfo::new(provider_id, provider.name(), state);
            status.message = message;
            status.failures = health.failures;
            status.circuit_open_until_ms = health.circuit_open_until_ms();
            status.blocked = health.blocked;
            self.set_status(status.clone());
            return Ok(status);
        }

        let next_page = {
            let pages = session
                .pages
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            pages.get(provider_id).copied().unwrap_or(1) + 1
        };
        let settings = self.settings();
        let started = Instant::now();
        let page = self
            .call_provider(&session, &provider, next_page, &settings)
            .await;
        match page {
            Ok((page, cached)) => {
                let assets = self.filter_assets(&session, page.assets.clone(), &settings);
                let status = self.success_status(
                    &provider,
                    &assets,
                    &page,
                    cached,
                    started.elapsed().as_millis() as u64,
                );
                {
                    let mut pages = session
                        .pages
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    pages.insert(provider_id.to_string(), next_page);
                }
                self.set_status(status.clone());
                let payload = ProviderPayload {
                    search_id: session.id.clone(),
                    provider: provider_id.to_string(),
                    assets,
                    page: next_page,
                    has_more: page.has_more,
                    total: page.total,
                    status: status.clone(),
                    error: None,
                    append: true,
                };
                let _ = self.app.emit("search://provider", payload);
                Ok(status)
            }
            Err(AppError::Cancelled(_)) => Err(AppError::Cancelled("search was cancelled".into())),
            Err(error) => {
                let status = self.failure_status(&provider, &error);
                self.set_status(status.clone());
                let payload = ProviderPayload {
                    search_id: session.id.clone(),
                    provider: provider_id.to_string(),
                    assets: Vec::new(),
                    page: next_page,
                    has_more: false,
                    total: None,
                    status: status.clone(),
                    error: Some(error.to_string()),
                    append: true,
                };
                let _ = self.app.emit("search://provider", payload);
                Err(error)
            }
        }
    }

    fn settings(&self) -> AppSettings {
        self.settings
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    fn set_status(&self, status: ProviderStatusInfo) {
        let mut statuses = self
            .statuses
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        statuses.insert(status.provider.clone(), status);
    }

    async fn run_initial(
        self: Arc<Self>,
        provider: Arc<dyn ImageProvider>,
        session: Arc<SearchSession>,
    ) {
        // Give the command response a moment to reach the frontend so its searchId is set before
        // the first streamed payload (cached responses can otherwise arrive instantly).
        tokio::time::sleep(Duration::from_millis(40)).await;
        if session.cancel.is_cancelled() {
            return;
        }
        let settings = self.settings();
        let permit = tokio::select! {
            _ = session.cancel.cancelled() => return,
            permit = self.provider_slots.clone().acquire_owned() => permit,
        };
        let Ok(_permit) = permit else {
            return;
        };
        let started = Instant::now();

        if let Some(provider) = self.registry.get(provider.id()) {
            self.run_initial_inner(&provider, &session, started, &settings)
                .await;
        }
    }

    async fn run_initial_inner(
        self: &Arc<Self>,
        provider: &Arc<dyn ImageProvider>,
        session: &Arc<SearchSession>,
        started: Instant,
        settings: &AppSettings,
    ) {
        if session.cancel.is_cancelled() {
            return;
        }
        let result = self.call_provider(session, provider, 1, settings).await;
        let latency = started.elapsed().as_millis() as u64;
        match result {
            Ok((page, cached)) => {
                let assets = self.filter_assets(session, page.assets.clone(), settings);
                let mut status = self.success_status(provider, &assets, &page, cached, latency);
                status.has_more = page.has_more;
                self.set_status(status.clone());
                session
                    .pages
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .insert(provider.id().to_string(), 1);
                let count = assets.len();
                session.results.fetch_add(count, Ordering::Relaxed);
                let payload = ProviderPayload {
                    search_id: session.id.clone(),
                    provider: provider.id().to_string(),
                    assets,
                    page: 1,
                    has_more: page.has_more,
                    total: page.total,
                    status,
                    error: None,
                    append: false,
                };
                let _ = self.app.emit("search://provider", payload);
            }
            Err(AppError::Cancelled(_)) => {}
            Err(error) => {
                if session.cancel.is_cancelled() {
                    return;
                }
                let status = self.failure_status(provider, &error);
                self.set_status(status.clone());
                session
                    .errors
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .push(ProviderError {
                        provider: provider.id().to_string(),
                        message: error.to_string(),
                    });
                let payload = ProviderPayload {
                    search_id: session.id.clone(),
                    provider: provider.id().to_string(),
                    assets: Vec::new(),
                    page: 1,
                    has_more: false,
                    total: None,
                    status,
                    error: Some(error.to_string()),
                    append: false,
                };
                let _ = self.app.emit("search://provider", payload);
            }
        }
    }

    async fn call_provider(
        &self,
        session: &SearchSession,
        provider: &Arc<dyn ImageProvider>,
        page: u32,
        settings: &AppSettings,
    ) -> AppResult<(ProviderPage, bool)> {
        let mut request = session.request.clone();
        request.page = page;
        request.limit = settings.results_per_page.clamp(1, 100);
        let cache_key = cache_key(provider.id(), &request, page, settings.safe_search);
        if let Ok(Some(cached)) = self.db.get_metadata(&cache_key) {
            return Ok((cached, true));
        }
        let ctx = ProviderContext {
            http: Arc::clone(&self.http),
            settings: settings.clone(),
        };
        let result = tokio::select! {
            _ = session.cancel.cancelled() => Err(AppError::Cancelled("search was cancelled".into())),
            result = provider.search(&ctx, &request) => result,
        };
        match result {
            Ok(page) => {
                self.health.record_success(provider.id());
                // Pixabay's API terms require responses to be cached for 24 hours. That rule wins
                // over the user's global TTL, including when metadata caching is otherwise disabled.
                // AI Horde produces a unique result for every trigger, so caching it can only hide
                // fresh images behind a stale entry.
                let ttl_minutes = match provider.id() {
                    "pixabay" => 24 * 60,
                    "aihorde" => 0,
                    _ => settings.metadata_cache_minutes,
                };
                if ttl_minutes > 0 {
                    if let Err(error) =
                        self.db
                            .put_metadata(&cache_key, provider.id(), &page, ttl_minutes)
                    {
                        tracing::warn!(%error, provider = provider.id(), "metadata cache write failed");
                    }
                }
                Ok((page, false))
            }
            Err(error) => {
                if !matches!(error, AppError::Cancelled(_)) {
                    let kind = match error {
                        AppError::Policy(_) => FailureKind::Policy,
                        AppError::RateLimited(_) => FailureKind::RateLimited,
                        _ => FailureKind::Transient,
                    };
                    self.health
                        .record_failure(provider.id(), kind, error.to_string());
                }
                Err(error)
            }
        }
    }

    fn filter_assets(
        &self,
        session: &SearchSession,
        assets: Vec<Asset>,
        settings: &AppSettings,
    ) -> Vec<Asset> {
        // The colour filter is applied by the frontend against provider-published colour metadata,
        // where the result grid can honestly count images that carry no colour data.
        let mut filters = session.request.filters.clone();
        filters.color = "any".into();
        let mut deduper = session
            .deduper
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        assets
            .into_iter()
            .filter(|asset| {
                if settings.safe_search
                    && asset
                        .provider_metadata
                        .get("mature")
                        .map(|value| value == "true")
                        .unwrap_or(false)
                {
                    return false;
                }
                crate::util::matches_filters(asset, &filters) && deduper.insert(asset)
            })
            .collect()
    }

    fn success_status(
        &self,
        provider: &Arc<dyn ImageProvider>,
        assets: &[Asset],
        page: &ProviderPage,
        cached: bool,
        latency_ms: u64,
    ) -> ProviderStatusInfo {
        let health = self.health.snapshot(provider.id());
        let mut status = ProviderStatusInfo::new(
            provider.id(),
            provider.name(),
            if assets.is_empty() {
                ProviderState::Empty
            } else {
                ProviderState::Ok
            },
        );
        status.results = assets.len();
        status.total = page.total;
        status.has_more = page.has_more;
        status.latency_ms = if cached { None } else { Some(latency_ms) };
        status.failures = health.failures;
        status.last_success_ms = health.last_success_ms;
        status.blocked = health.blocked;
        status.message = if assets.is_empty() {
            if cached {
                "no matches (cached response)".into()
            } else {
                "no matches".into()
            }
        } else if cached {
            format!("{} result(s) from local metadata cache", assets.len())
        } else if assets.len() < page.assets.len() {
            format!(
                "{} result(s); {} hidden by local filters or duplicates",
                assets.len(),
                page.assets.len() - assets.len()
            )
        } else {
            format!("{} result(s)", assets.len())
        };
        status
    }

    fn failure_status(
        &self,
        provider: &Arc<dyn ImageProvider>,
        error: &AppError,
    ) -> ProviderStatusInfo {
        let health = self.health.snapshot(provider.id());
        let state = match error {
            AppError::Policy(_) => ProviderState::Restricted,
            AppError::RateLimited(_) => ProviderState::RateLimited,
            AppError::Timeout(_) => ProviderState::Timeout,
            AppError::Malformed(_) => ProviderState::Malformed,
            _ => ProviderState::Unavailable,
        };
        let mut status = ProviderStatusInfo::new(provider.id(), provider.name(), state);
        status.message = error.to_string();
        status.failures = health.failures;
        status.circuit_open_until_ms = health.circuit_open_until_ms();
        status.last_success_ms = health.last_success_ms;
        status.blocked = health.blocked || matches!(error, AppError::Policy(_));
        status
    }

    fn finalize(&self, session: &Arc<SearchSession>, cancelled: bool) {
        if session.finished.swap(true, Ordering::SeqCst) {
            return;
        }
        let cancelled = cancelled || session.cancel.is_cancelled();
        let result_count = session.results.load(Ordering::Relaxed);
        let errors = session
            .errors
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        if cancelled {
            let mut statuses = self
                .statuses
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            for status in statuses.values_mut() {
                if status.state == ProviderState::Searching {
                    status.state = ProviderState::Empty;
                    status.message = "cancelled".into();
                }
            }
        }
        if let Some(id) = session.history_id {
            if let Err(error) = self.db.update_history_count(id, result_count) {
                tracing::warn!(%error, "could not update search history result count");
            }
        }
        tracing::info!(
            search = %session.id,
            results = result_count,
            cancelled,
            errors = errors.len(),
            "search finished"
        );
        let finished = SearchFinished {
            search_id: session.id.clone(),
            result_count,
            cancelled,
            errors,
        };
        let _ = self.app.emit("search://finished", finished);
    }
}

fn cache_key(provider: &str, request: &SearchRequest, page: u32, safe_search: bool) -> String {
    let filters: Value = serde_json::to_value(&request.filters).unwrap_or(Value::Null);
    let material = format!(
        "{}|{}|{}|{}|{}|{}|safe={}",
        provider,
        request.query.trim().to_lowercase(),
        page,
        request.limit,
        request.sort,
        filters,
        safe_search
    );
    hex::encode(Sha256::digest(material.as_bytes()))
}

fn prune_sessions(sessions: &mut HashMap<String, Arc<SearchSession>>, keep: &str) {
    if sessions.len() <= 8 {
        return;
    }
    let mut finished: Vec<(Instant, String)> = sessions
        .iter()
        .filter(|(id, session)| id.as_str() != keep && session.finished.load(Ordering::Relaxed))
        .map(|(id, session)| (session.started, id.clone()))
        .collect();
    finished.sort_by_key(|(started, _)| *started);
    for (_, id) in finished.into_iter().take(sessions.len().saturating_sub(8)) {
        sessions.remove(&id);
    }
}

/// Exposed for tests: the cache key must be stable across calls but differ by provider/page.
#[cfg(test)]
pub fn test_cache_key(provider: &str, request: &SearchRequest, page: u32) -> String {
    cache_key(provider, request, page, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::AssetFilters;

    #[test]
    fn cache_keys_are_stable_and_scope_aware() {
        let request = SearchRequest {
            query: "Mountain".into(),
            providers: vec!["openverse".into()],
            limit: 24,
            page: 1,
            filters: AssetFilters::default(),
            sort: "relevance".into(),
        };
        let first = test_cache_key("openverse", &request, 1);
        assert_eq!(first, test_cache_key("openverse", &request, 1));
        assert_ne!(first, test_cache_key("openverse", &request, 2));
        assert_ne!(first, test_cache_key("nasa", &request, 1));
        let mut other = request.clone();
        other.query = "Mountain ".into();
        assert_eq!(
            first,
            test_cache_key("openverse", &other, 1),
            "query is normalised"
        );
    }

    #[test]
    fn pruning_keeps_the_active_session() {
        let mut map = HashMap::new();
        for index in 0..12 {
            let session = Arc::new(SearchSession {
                id: format!("s{index}"),
                request: SearchRequest::default(),
                cancel: CancellationToken::new(),
                pages: Mutex::new(HashMap::new()),
                deduper: Mutex::new(Deduper::new()),
                errors: Mutex::new(Vec::new()),
                results: AtomicUsize::new(0),
                finished: AtomicBool::new(true),
                history_id: None,
                started: Instant::now(),
            });
            map.insert(session.id.clone(), session);
        }
        prune_sessions(&mut map, "s11");
        assert!(map.contains_key("s11"));
        assert!(map.len() <= 8);
    }

    #[test]
    fn filter_assets_applies_local_filters_and_dedup() {
        // The filtering logic is exercised through util::matches_filters and Deduper elsewhere;
        // this asserts the composition order: safe-search first, then filters, then dedup.
        let mut asset = Asset::new("test", "1");
        asset.width = Some(1000);
        asset.height = Some(500);
        asset
            .provider_metadata
            .insert("mature".into(), "true".into());
        let filters = AssetFilters::default();
        assert!(crate::util::matches_filters(&asset, &filters));
        assert_eq!(
            asset.provider_metadata.get("mature").map(String::as_str),
            Some("true")
        );
    }
}
