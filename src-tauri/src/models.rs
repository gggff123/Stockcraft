//! Serde models shared with the TypeScript frontend (see `src/lib/types.ts`).
//!
//! Everything crossing the IPC boundary uses camelCase keys so the Rust structs map 1:1 onto the
//! interfaces in `types.ts`. Optional values are serialised as `null` rather than omitted, which is
//! what the frontend's optional properties accept.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ProviderState {
    #[default]
    Idle,
    Searching,
    Ok,
    Empty,
    Unavailable,
    Timeout,
    RateLimited,
    CircuitOpen,
    Restricted,
    Disabled,
    Malformed,
}

impl ProviderState {
    pub fn as_str(self) -> &'static str {
        match self {
            ProviderState::Idle => "idle",
            ProviderState::Searching => "searching",
            ProviderState::Ok => "ok",
            ProviderState::Empty => "empty",
            ProviderState::Unavailable => "unavailable",
            ProviderState::Timeout => "timeout",
            ProviderState::RateLimited => "rate_limited",
            ProviderState::CircuitOpen => "circuit_open",
            ProviderState::Restricted => "restricted",
            ProviderState::Disabled => "disabled",
            ProviderState::Malformed => "malformed",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Asset {
    pub id: String,
    pub provider: String,
    pub provider_asset_id: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub thumbnail_url: Option<String>,
    pub preview_url: Option<String>,
    pub page_url: Option<String>,
    pub download_url: Option<String>,
    pub creator: Option<String>,
    pub creator_url: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub mime_type: Option<String>,
    pub license_name: Option<String>,
    pub license_url: Option<String>,
    pub attribution: Option<String>,
    pub source_name: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default = "default_media_type")]
    pub media_type: String,
    #[serde(default)]
    pub download_allowed: bool,
    pub dominant_color: Option<String>,
    pub created_at: Option<String>,
    #[serde(default)]
    pub provider_metadata: BTreeMap<String, String>,
}

fn default_media_type() -> String {
    "photo".to_string()
}

impl Asset {
    pub fn new(provider: &str, provider_asset_id: impl Into<String>) -> Self {
        let provider_asset_id = provider_asset_id.into();
        Self {
            id: format!("{provider}:{provider_asset_id}"),
            provider: provider.to_string(),
            provider_asset_id,
            media_type: default_media_type(),
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetFilters {
    #[serde(default = "default_orientation")]
    pub orientation: String,
    #[serde(default)]
    pub min_width: u32,
    #[serde(default)]
    pub min_height: u32,
    #[serde(default = "default_any")]
    pub media_type: String,
    #[serde(default = "default_any")]
    pub license: String,
    #[serde(default = "default_any")]
    pub color: String,
    #[serde(default = "default_all")]
    pub category: String,
}

fn default_orientation() -> String {
    "any".into()
}
fn default_any() -> String {
    "any".into()
}
fn default_all() -> String {
    "all".into()
}

impl Default for AssetFilters {
    fn default() -> Self {
        Self {
            orientation: default_orientation(),
            min_width: 0,
            min_height: 0,
            media_type: default_any(),
            license: default_any(),
            color: default_any(),
            category: default_all(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchRequest {
    pub query: String,
    #[serde(default)]
    pub providers: Vec<String>,
    #[serde(default = "default_limit")]
    pub limit: u32,
    #[serde(default = "default_page")]
    pub page: u32,
    #[serde(default)]
    pub filters: AssetFilters,
    #[serde(default = "default_sort")]
    pub sort: String,
}

fn default_limit() -> u32 {
    24
}
fn default_page() -> u32 {
    1
}
fn default_sort() -> String {
    "relevance".into()
}

impl Default for SearchRequest {
    fn default() -> Self {
        Self {
            query: String::new(),
            providers: Vec::new(),
            limit: default_limit(),
            page: 1,
            filters: AssetFilters::default(),
            sort: default_sort(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProviderPolicy {
    pub access_method: String,
    pub docs_url: Option<String>,
    pub robots_url: Option<String>,
    pub terms_url: Option<String>,
    pub scraping_permitted: bool,
    pub anonymous_access: bool,
    pub rate_limit: String,
    pub notes: String,
    pub checked_on: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProviderCapabilities {
    pub search: bool,
    pub pagination: bool,
    pub download: bool,
    pub orientation_filter: bool,
    pub size_filter: bool,
    pub media_type_filter: bool,
    pub license_filter: bool,
    pub color_filter: bool,
    pub sort_options: Vec<String>,
    pub media_types: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProviderInfo {
    pub id: String,
    pub name: String,
    pub source_name: String,
    pub website_url: String,
    pub license_hint: String,
    pub enabled_by_default: bool,
    pub requires_api_key: bool,
    pub has_api_key: bool,
    pub capabilities: ProviderCapabilities,
    pub policy: ProviderPolicy,
    pub notes: String,
    pub contact_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProviderStatusInfo {
    pub provider: String,
    pub name: String,
    pub state: ProviderState,
    pub message: String,
    pub results: usize,
    pub total: Option<u64>,
    pub latency_ms: Option<u64>,
    pub has_more: bool,
    pub failures: u32,
    pub circuit_open_until_ms: Option<u64>,
    pub last_success_ms: Option<u64>,
    pub blocked: bool,
}

impl ProviderStatusInfo {
    pub fn new(id: &str, name: &str, state: ProviderState) -> Self {
        Self {
            provider: id.to_string(),
            name: name.to_string(),
            state,
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchStart {
    pub search_id: String,
    pub query: String,
    pub providers: Vec<ProviderStatusInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderError {
    pub provider: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderPayload {
    pub search_id: String,
    pub provider: String,
    pub assets: Vec<Asset>,
    pub page: u32,
    pub has_more: bool,
    pub total: Option<u64>,
    pub status: ProviderStatusInfo,
    pub error: Option<String>,
    pub append: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchFinished {
    pub search_id: String,
    pub result_count: usize,
    pub cancelled: bool,
    pub errors: Vec<ProviderError>,
}

/// One provider page, used both on the wire (inside `ProviderPayload`) and in the metadata cache.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderPage {
    pub assets: Vec<Asset>,
    pub page: u32,
    pub has_more: bool,
    pub total: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadRecord {
    pub id: i64,
    pub asset_id: String,
    pub provider: String,
    pub title: String,
    pub file_path: String,
    pub source_url: Option<String>,
    pub license_name: Option<String>,
    pub license_url: Option<String>,
    pub bytes: u64,
    pub status: String,
    pub error: Option<String>,
    pub created_at: String,
    pub thumbnail_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgress {
    pub download_id: i64,
    pub asset_id: String,
    pub received: u64,
    pub total: u64,
    pub status: String,
    pub file_path: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Collection {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    pub item_count: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryItem {
    pub id: i64,
    pub query: String,
    pub providers: String,
    pub result_count: i64,
    pub searched_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub theme: String,
    pub density: String,
    pub download_dir: String,
    pub results_per_page: u32,
    pub enabled_providers: Vec<String>,
    pub cache_thumbnails: bool,
    pub thumbnail_cache_mb: u32,
    pub metadata_cache_minutes: u32,
    pub confirm_overwrite: bool,
    pub write_attribution_sidecar: bool,
    pub open_folder_after_download: bool,
    pub safe_search: bool,
    pub reduced_motion: bool,
    pub pexels_api_key: String,
    pub pixabay_api_key: String,
    pub freepik_api_key: String,
    pub history_enabled: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: "system".into(),
            density: "standard".into(),
            download_dir: String::new(),
            results_per_page: 24,
            enabled_providers: vec![
                "openverse".into(),
                "wikimedia".into(),
                "nasa".into(),
                "cleveland".into(),
            ],
            cache_thumbnails: true,
            thumbnail_cache_mb: 256,
            metadata_cache_minutes: 30,
            confirm_overwrite: true,
            write_attribution_sidecar: true,
            open_folder_after_download: false,
            safe_search: true,
            reduced_motion: false,
            pexels_api_key: String::new(),
            pixabay_api_key: String::new(),
            freepik_api_key: String::new(),
            history_enabled: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CacheStats {
    pub thumbnail_bytes: u64,
    pub thumbnail_files: u64,
    pub metadata_rows: i64,
    pub library_items: i64,
    pub favorites: i64,
    pub collections: i64,
    pub downloads: i64,
    pub db_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderDiagnostic {
    pub id: String,
    pub name: String,
    pub state: ProviderState,
    pub message: String,
    pub failures: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostics {
    pub app_version: String,
    pub os: String,
    pub arch: String,
    pub data_dir: String,
    pub db_path: String,
    pub thumbnail_dir: String,
    pub log_path: String,
    pub providers: Vec<ProviderDiagnostic>,
    pub counts: CacheStats,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asset_ids_are_stable_and_camel_case_on_the_wire() {
        let mut asset = Asset::new("openverse", "abc-123");
        asset.title = Some("A title".into());
        asset.provider_metadata.insert("k".into(), "v".into());
        let json = serde_json::to_value(&asset).unwrap();
        assert_eq!(json["id"], "openverse:abc-123");
        assert_eq!(json["providerAssetId"], "abc-123");
        assert_eq!(json["mediaType"], "photo");
        assert_eq!(json["providerMetadata"]["k"], "v");
        assert!(json.get("downloadAllowed").is_some());
    }

    #[test]
    fn default_settings_are_sane() {
        let s = AppSettings::default();
        assert_eq!(s.results_per_page, 24);
        assert!(s.enabled_providers.contains(&"openverse".to_string()));
        assert!(!s.enabled_providers.contains(&"pexels".to_string()));
        assert!(s.thumbnail_cache_mb >= 64);
    }

    #[test]
    fn provider_states_serialise_as_frontend_strings() {
        assert_eq!(
            serde_json::to_value(ProviderState::RateLimited).unwrap(),
            "rate_limited"
        );
        assert_eq!(
            serde_json::to_value(ProviderState::CircuitOpen).unwrap(),
            "circuit_open"
        );
        assert_eq!(
            serde_json::to_value(ProviderState::Malformed).unwrap(),
            "malformed"
        );
    }
}
