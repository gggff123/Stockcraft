//! Optional Pexels adapter — official API only, requires a user-supplied key.
//!
//! The public-page adapter is intentionally absent: `pexels.com/robots.txt` disallows search query
//! URLs, filter parameters and download paths. This adapter never touches those pages.

use async_trait::async_trait;
use serde_json::Value;

use crate::error::{AppError, AppResult};
use crate::models::{Asset, ProviderInfo, ProviderPage, SearchRequest};
use crate::util::{extension_for, nearest_color_name};

use super::{capabilities, info, policy, require_query, text_at, ImageProvider, ProviderContext};

pub struct PexelsProvider;

const API: &str = "https://api.pexels.com/v1";

#[async_trait]
impl ImageProvider for PexelsProvider {
    fn id(&self) -> &'static str {
        "pexels"
    }

    fn name(&self) -> &'static str {
        "Pexels"
    }

    fn requires_api_key(&self) -> bool {
        true
    }

    fn api_key(&self, settings: &crate::models::AppSettings) -> Option<String> {
        Some(settings.pexels_api_key.clone())
    }

    fn info(&self, has_api_key: bool) -> ProviderInfo {
        let mut provider = info(
            "pexels",
            "Pexels",
            "Pexels",
            "https://www.pexels.com",
            "Pexels licence",
            false,
            true,
            has_api_key,
            capabilities(true, true, false, false, true, &["relevance"], &["photo"]),
            policy(
                "Official API only (api.pexels.com/v1, key required)",
                Some("https://www.pexels.com/api/documentation/"),
                Some("https://www.pexels.com/robots.txt"),
                Some("https://www.pexels.com/terms-of-service/"),
                false,
                "documented per plan (default 200 requests/hour)",
                "robots.txt disallows *q=* search URLs, filter query strings and /download/ paths, so no public-page adapter exists. This adapter is disabled until a key is saved.",
            ),
            "Optional. Add a free Pexels API key in Settings → Sources to search Pexels through its official API. Stockcraft never scrapes Pexels.",
        );
        provider.contact_url = Some("https://www.pexels.com/api/".into());
        provider
    }

    async fn search(
        &self,
        ctx: &ProviderContext,
        request: &SearchRequest,
    ) -> AppResult<ProviderPage> {
        let key = ctx.settings.pexels_api_key.trim().to_string();
        if key.is_empty() {
            return Err(AppError::Policy(
                "Pexels requires an API key; add one in Settings → Sources".into(),
            ));
        }
        let query = require_query(request)?;
        let page = request.page.max(1);
        let page_size = ctx.page_size(request.limit, 80);
        let mut url = url::Url::parse(&format!("{API}/search"))
            .map_err(|e| AppError::internal(e.to_string()))?;
        {
            let mut pairs = url.query_pairs_mut();
            pairs.append_pair("query", query);
            pairs.append_pair("per_page", &page_size.to_string());
            pairs.append_pair("page", &page.to_string());
            pairs.append_pair("locale", "en-US");
            match request.filters.orientation.as_str() {
                "landscape" => pairs.append_pair("orientation", "landscape"),
                "portrait" => pairs.append_pair("orientation", "portrait"),
                "square" => pairs.append_pair("orientation", "square"),
                _ => &mut pairs,
            };
            let min_side = request.filters.min_width.max(request.filters.min_height);
            if min_side >= 2_000 {
                pairs.append_pair("size", "large");
            } else if min_side >= 1_200 {
                pairs.append_pair("size", "medium");
            } else if min_side > 0 {
                pairs.append_pair("size", "small");
            }
            if request.filters.color != "any" {
                pairs.append_pair("color", &request.filters.color);
            }
        }
        ctx.http.acquire("pexels").await?;
        let value = ctx.http.get_json(url.as_str(), Some(&key)).await?;
        parse_page(&value, page, page_size)
    }

    async fn details(&self, ctx: &ProviderContext, provider_asset_id: &str) -> AppResult<Asset> {
        let id = provider_asset_id
            .rsplit(':')
            .next()
            .unwrap_or(provider_asset_id);
        if id.is_empty() || !id.chars().all(|ch| ch.is_ascii_digit()) {
            return Err(AppError::validation("invalid Pexels photo id"));
        }
        let key = ctx.settings.pexels_api_key.trim().to_string();
        if key.is_empty() {
            return Err(AppError::Policy("Pexels requires an API key".into()));
        }
        ctx.http.acquire("pexels").await?;
        let value = ctx
            .http
            .get_json(&format!("{API}/photos/{id}"), Some(&key))
            .await?;
        parse_asset(&value)
            .ok_or_else(|| AppError::Malformed("Pexels returned an incomplete photo".into()))
    }
}

pub(crate) fn parse_page(value: &Value, page: u32, page_size: u32) -> AppResult<ProviderPage> {
    let photos = value
        .get("photos")
        .and_then(Value::as_array)
        .ok_or_else(|| AppError::Malformed("Pexels response has no photos array".into()))?;
    let total = value.get("total_results").and_then(Value::as_u64);
    let mut assets = Vec::new();
    for photo in photos {
        if let Some(asset) = parse_asset(photo) {
            assets.push(asset);
        }
    }
    let has_more = value
        .get("next_page")
        .map(|next| !next.is_null())
        .unwrap_or_else(|| {
            total
                .map(|total| u64::from(page) * u64::from(page_size) < total)
                .unwrap_or(false)
        });
    Ok(ProviderPage {
        assets,
        page,
        has_more,
        total,
    })
}

pub(crate) fn parse_asset(item: &Value) -> Option<Asset> {
    let id = item.get("id").and_then(Value::as_u64)?;
    let mut asset = Asset::new("pexels", id.to_string());
    asset.title = text_at(item, "alt");
    asset.description = None;
    asset.page_url = text_at(item, "url");
    asset.thumbnail_url = text_at(item, "src.medium").or_else(|| text_at(item, "src.small"));
    asset.preview_url = text_at(item, "src.large").or_else(|| text_at(item, "src.large2x"));
    asset.download_url = text_at(item, "src.original");
    asset.creator = text_at(item, "photographer");
    asset.creator_url = text_at(item, "photographer_url");
    asset.width = item
        .get("width")
        .and_then(Value::as_u64)
        .map(|value| value.min(u32::MAX as u64) as u32);
    asset.height = item
        .get("height")
        .and_then(Value::as_u64)
        .map(|value| value.min(u32::MAX as u64) as u32);
    asset.mime_type = Some(
        match extension_for(None, asset.download_url.as_deref().unwrap_or("")).as_str() {
            "png" => "image/png",
            "webp" => "image/webp",
            _ => "image/jpeg",
        }
        .to_string(),
    );
    asset.license_name = Some("Pexels License".into());
    asset.license_url = Some("https://www.pexels.com/license/".into());
    asset.attribution = Some(format!(
        "Photo by {} on Pexels",
        asset.creator.as_deref().unwrap_or("a Pexels contributor")
    ));
    asset.source_name = Some("Pexels".into());
    asset.media_type = "photo".into();
    asset.download_allowed = asset
        .download_url
        .as_deref()
        .map(crate::util::is_allowed_web_url)
        .unwrap_or(false);
    asset.dominant_color = text_at(item, "avg_color").and_then(|color| nearest_color_name(&color));
    if let Some(photographer_id) = item.get("photographer_id").and_then(Value::as_u64) {
        asset
            .provider_metadata
            .insert("photographerId".into(), photographer_id.to_string());
    }
    asset
        .provider_metadata
        .insert("license".into(), "Pexels License".into());
    Some(asset)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Value {
        serde_json::json!({
            "page": 1,
            "per_page": 24,
            "total_results": 8000,
            "next_page": "https://api.pexels.com/v1/search?page=2",
            "photos": [{
                "id": 2014422,
                "width": 3024,
                "height": 4032,
                "url": "https://www.pexels.com/photo/brown-rocks-2014422/",
                "photographer": "Joey Farina",
                "photographer_url": "https://www.pexels.com/@joey",
                "photographer_id": 680589,
                "avg_color": "#8C8C8C",
                "src": {
                    "original": "https://images.pexels.com/photos/2014422/pexels-photo-2014422.jpeg",
                    "large2x": "https://images.pexels.com/photos/2014422/pexels-photo-2014422.jpeg?w=1880",
                    "large": "https://images.pexels.com/photos/2014422/pexels-photo-2014422.jpeg?w=940",
                    "medium": "https://images.pexels.com/photos/2014422/pexels-photo-2014422.jpeg?w=350",
                    "small": "https://images.pexels.com/photos/2014422/pexels-photo-2014422.jpeg?w=280",
                    "portrait": "https://images.pexels.com/photos/2014422/pexels-photo-2014422.jpeg?w=800&h=1200",
                    "landscape": "https://images.pexels.com/photos/2014422/pexels-photo-2014422.jpeg?w=1200&h=627"
                },
                "alt": "Brown Rocks"
            }]
        })
    }

    #[test]
    fn parses_documented_photo_shape() {
        let page = parse_page(&fixture(), 1, 24).unwrap();
        assert_eq!(page.total, Some(8000));
        assert!(page.has_more);
        let asset = &page.assets[0];
        assert_eq!(asset.id, "pexels:2014422");
        assert_eq!(asset.title.as_deref(), Some("Brown Rocks"));
        assert_eq!(asset.width, Some(3024));
        assert_eq!(
            asset.download_url.as_deref(),
            Some("https://images.pexels.com/photos/2014422/pexels-photo-2014422.jpeg")
        );
        assert_eq!(asset.dominant_color.as_deref(), Some("grey"));
        assert_eq!(asset.license_name.as_deref(), Some("Pexels License"));
        assert!(asset.download_allowed);
    }

    #[test]
    fn missing_photos_is_malformed() {
        assert_eq!(
            parse_page(&serde_json::json!({}), 1, 24)
                .unwrap_err()
                .code(),
            "malformed"
        );
    }

    #[test]
    fn requires_a_key() {
        let provider = PexelsProvider;
        let info = provider.info(false);
        assert!(info.requires_api_key);
        assert!(!info.has_api_key);
        assert!(!info.enabled_by_default);
        assert!(!info.policy.scraping_permitted);
        assert!(info.info_url_mentions_robots_placeholder());
    }

    trait InfoTestExt {
        fn info_url_mentions_robots_placeholder(&self) -> bool;
    }

    impl InfoTestExt for ProviderInfo {
        fn info_url_mentions_robots_placeholder(&self) -> bool {
            self.policy
                .robots_url
                .as_deref()
                .is_some_and(|url| url.ends_with("robots.txt"))
        }
    }
}
