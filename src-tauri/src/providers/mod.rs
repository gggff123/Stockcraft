//! Provider adapters.
//!
//! Every adapter talks to a documented public API only. The trait keeps provider-specific JSON out
//! of the search orchestrator; the registry owns one instance of each adapter.

pub mod aihorde;
pub mod cleveland;
pub mod freepik;
pub mod health;
pub mod http;
pub mod met;
pub mod nasa;
pub mod openverse;
pub mod pexels;
pub mod pixabay;
pub mod rate_limit;
pub mod smk;
pub mod wellcome;
pub mod wikimedia;

use std::sync::Arc;

use async_trait::async_trait;

use crate::error::{AppError, AppResult};
use crate::models::{
    AppSettings, Asset, ProviderCapabilities, ProviderInfo, ProviderPage, ProviderPolicy,
    SearchRequest,
};

use self::http::HttpClient;

pub struct ProviderContext {
    pub http: Arc<HttpClient>,
    pub settings: AppSettings,
}

impl ProviderContext {
    pub fn page_size(&self, requested: u32, max: u32) -> u32 {
        requested.clamp(1, max.max(1))
    }
}

#[async_trait]
pub trait ImageProvider: Send + Sync {
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn info(&self, has_api_key: bool) -> ProviderInfo;
    fn requires_api_key(&self) -> bool {
        false
    }
    async fn search(
        &self,
        ctx: &ProviderContext,
        request: &SearchRequest,
    ) -> AppResult<ProviderPage>;
    async fn details(&self, ctx: &ProviderContext, provider_asset_id: &str) -> AppResult<Asset>;

    /// Adapters that need an API key override this.
    fn api_key(&self, _settings: &AppSettings) -> Option<String> {
        None
    }
}

pub struct ProviderRegistry {
    providers: Vec<Arc<dyn ImageProvider>>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self {
            providers: vec![
                Arc::new(openverse::OpenverseProvider),
                Arc::new(wikimedia::WikimediaProvider),
                Arc::new(nasa::NasaProvider),
                Arc::new(cleveland::ClevelandProvider),
                Arc::new(pexels::PexelsProvider),
                Arc::new(pixabay::PixabayProvider),
                Arc::new(freepik::FreepikProvider),
                Arc::new(wellcome::WellcomeProvider),
                Arc::new(smk::SmkProvider),
                Arc::new(met::MetProvider),
                Arc::new(aihorde::AiHordeProvider),
            ],
        }
    }

    pub fn all(&self) -> &[Arc<dyn ImageProvider>] {
        &self.providers
    }

    pub fn get(&self, id: &str) -> Option<Arc<dyn ImageProvider>> {
        self.providers
            .iter()
            .find(|provider| provider.id() == id)
            .cloned()
    }

    pub fn ids(&self) -> Vec<String> {
        self.providers
            .iter()
            .map(|provider| provider.id().to_string())
            .collect()
    }

    pub fn infos(&self, settings: &AppSettings) -> Vec<ProviderInfo> {
        self.providers
            .iter()
            .map(|provider| {
                let has_key = provider
                    .api_key(settings)
                    .map(|key| !key.trim().is_empty())
                    .unwrap_or(false);
                provider.info(has_key)
            })
            .collect()
    }

    /// Providers the user enabled *and* that are usable (keyed providers need a key).
    pub fn enabled(&self, settings: &AppSettings) -> Vec<Arc<dyn ImageProvider>> {
        self.providers
            .iter()
            .filter(|provider| {
                settings
                    .enabled_providers
                    .iter()
                    .any(|id| id == provider.id())
            })
            .filter(|provider| {
                !provider.requires_api_key()
                    || provider
                        .api_key(settings)
                        .map(|key| !key.trim().is_empty())
                        .unwrap_or(false)
            })
            .cloned()
            .collect()
    }
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/* --------------------------------------------------------------- shared helpers */

pub fn capabilities(
    orientation: bool,
    size: bool,
    media_type: bool,
    license: bool,
    color: bool,
    sorts: &[&str],
    media_types: &[&str],
) -> ProviderCapabilities {
    ProviderCapabilities {
        search: true,
        pagination: true,
        download: true,
        orientation_filter: orientation,
        size_filter: size,
        media_type_filter: media_type,
        license_filter: license,
        color_filter: color,
        sort_options: sorts.iter().map(|s| (*s).to_string()).collect(),
        media_types: media_types.iter().map(|s| (*s).to_string()).collect(),
    }
}

pub fn policy(
    access_method: &str,
    docs_url: Option<&str>,
    robots_url: Option<&str>,
    terms_url: Option<&str>,
    anonymous: bool,
    rate_limit: &str,
    notes: &str,
) -> ProviderPolicy {
    ProviderPolicy {
        access_method: access_method.into(),
        docs_url: docs_url.map(str::to_string),
        robots_url: robots_url.map(str::to_string),
        terms_url: terms_url.map(str::to_string),
        scraping_permitted: false,
        anonymous_access: anonymous,
        rate_limit: rate_limit.into(),
        notes: notes.into(),
        checked_on: "2026-10-21".into(),
    }
}

#[allow(clippy::too_many_arguments)] // declarative constructor used by every adapter
pub fn info(
    id: &str,
    name: &str,
    source_name: &str,
    website: &str,
    license_hint: &str,
    enabled_by_default: bool,
    requires_api_key: bool,
    has_api_key: bool,
    capabilities: ProviderCapabilities,
    policy: ProviderPolicy,
    notes: &str,
) -> ProviderInfo {
    ProviderInfo {
        id: id.into(),
        name: name.into(),
        source_name: source_name.into(),
        website_url: website.into(),
        license_hint: license_hint.into(),
        enabled_by_default,
        requires_api_key,
        has_api_key,
        capabilities,
        policy,
        notes: notes.into(),
        contact_url: None,
    }
}

pub fn require_query(request: &SearchRequest) -> AppResult<&str> {
    let query = request.query.trim();
    if query.is_empty() {
        return Err(AppError::validation("search query is empty"));
    }
    if query.chars().count() > 160 {
        return Err(AppError::validation(
            "search query is limited to 160 characters",
        ));
    }
    Ok(query)
}

/// Extract a string at `path` (dot notation) from JSON, trimming it.
pub fn text_at(value: &serde_json::Value, path: &str) -> Option<String> {
    let mut current = value;
    for part in path.split('.') {
        current = current.get(part)?;
    }
    match current {
        serde_json::Value::String(text) => {
            let text = text.trim();
            if text.is_empty() {
                None
            } else {
                Some(text.to_string())
            }
        }
        serde_json::Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

pub fn number_at(value: &serde_json::Value, path: &str) -> Option<u64> {
    let mut current = value;
    for part in path.split('.') {
        current = current.get(part)?;
    }
    current
        .as_u64()
        .or_else(|| current.as_str().and_then(|text| text.parse::<u64>().ok()))
        .or_else(|| current.as_f64().map(|number| number.max(0.0) as u64))
}

pub fn bool_at(value: &serde_json::Value, path: &str) -> Option<bool> {
    let mut current = value;
    for part in path.split('.') {
        current = current.get(part)?;
    }
    current.as_bool()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_unique_ids_and_honest_policy_records() {
        let registry = ProviderRegistry::new();
        let mut ids = registry.ids();
        ids.sort();
        let mut deduped = ids.clone();
        deduped.dedup();
        assert_eq!(ids, deduped, "provider ids must be unique");
        for provider in registry.all() {
            let info = provider.info(false);
            assert_eq!(info.id, provider.id());
            assert!(!info.policy.access_method.is_empty());
            assert_eq!(info.policy.checked_on, "2026-10-21");
            assert!(!info.policy.scraping_permitted, "Stockcraft never scrapes");
        }
    }

    #[test]
    fn disabled_providers_are_not_returned_by_enabled() {
        let registry = ProviderRegistry::new();
        let mut settings = AppSettings {
            enabled_providers: vec!["openverse".into()],
            ..AppSettings::default()
        };
        let enabled = registry.enabled(&settings);
        assert_eq!(enabled.len(), 1);
        assert_eq!(enabled[0].id(), "openverse");
        settings.enabled_providers = vec!["pexels".into()];
        assert!(registry.enabled(&settings).is_empty(), "pexels needs a key");
        settings.pexels_api_key = "abc".into();
        assert_eq!(registry.enabled(&settings).len(), 1);
    }

    #[test]
    fn json_helpers_read_nested_values() {
        let value = serde_json::json!({"a": {"b": " x "}, "n": "42", "t": true});
        assert_eq!(text_at(&value, "a.b").as_deref(), Some("x"));
        assert_eq!(number_at(&value, "n"), Some(42));
        assert_eq!(bool_at(&value, "t"), Some(true));
        assert_eq!(text_at(&value, "a.c"), None);
    }
}
