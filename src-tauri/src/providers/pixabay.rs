//! Optional Pixabay adapter — official API only, requires a user-supplied key.
//!
//! Pixabay's API terms require responses to be cached for 24 hours and forbid permanent
//! hotlinking of Pixabay URLs. The search orchestrator therefore enforces a 24-hour metadata TTL
//! for this provider, and the interface caches thumbnails through the Rust proxy.
//!
//! Public search pages are not scraped: `pixabay.com/robots.txt` disallows the filter query strings
//! a search adapter would need, and the API is the documented access path.

use async_trait::async_trait;
use serde_json::Value;

use crate::error::{AppError, AppResult};
use crate::models::{Asset, ProviderInfo, ProviderPage, SearchRequest};
use crate::util::{extension_for, is_allowed_web_url};

use super::{capabilities, info, policy, require_query, text_at, ImageProvider, ProviderContext};

pub struct PixabayProvider;

const API: &str = "https://pixabay.com/api/";

#[async_trait]
impl ImageProvider for PixabayProvider {
    fn id(&self) -> &'static str {
        "pixabay"
    }

    fn name(&self) -> &'static str {
        "Pixabay"
    }

    fn requires_api_key(&self) -> bool {
        true
    }

    fn api_key(&self, settings: &crate::models::AppSettings) -> Option<String> {
        Some(settings.pixabay_api_key.clone())
    }

    fn info(&self, has_api_key: bool) -> ProviderInfo {
        let mut provider = info(
            "pixabay",
            "Pixabay",
            "Pixabay",
            "https://pixabay.com",
            "Pixabay Content License (royalty-free; attribution not required)",
            false,
            true,
            has_api_key,
            capabilities(
                true,
                true,
                true,
                false,
                true,
                &["relevance", "newest"],
                &["photo", "illustration", "vector"],
            ),
            policy(
                "Official API only (pixabay.com/api, key required)",
                Some("https://pixabay.com/api/docs/"),
                Some("https://pixabay.com/robots.txt"),
                Some("https://pixabay.com/service/terms/"),
                false,
                "100 requests per 60 seconds; responses must be cached for 24 hours",
                "API only. Public search pages are not scraped. Pixabay requires API responses to be cached for 24 hours; Stockcraft enforces that even when the global metadata TTL is lower. Basic keys expose scaled images (up to 1280 px); full-resolution URLs require Pixabay approval.",
            ),
            "Optional. Add a free Pixabay API key in Settings → Sources to search Pixabay through its official API. Results are cached for 24 hours as Pixabay requires.",
        );
        provider.contact_url = Some("https://pixabay.com/service/about/api/".into());
        provider
    }

    async fn search(
        &self,
        ctx: &ProviderContext,
        request: &SearchRequest,
    ) -> AppResult<ProviderPage> {
        let key = ctx.settings.pixabay_api_key.trim().to_string();
        if key.is_empty() {
            return Err(AppError::Policy(
                "Pixabay requires an API key; add one in Settings → Sources".into(),
            ));
        }
        let query = require_query(request)?;
        if query.chars().count() > 100 {
            return Err(AppError::validation(
                "Pixabay queries are limited to 100 characters",
            ));
        }
        let page = request.page.max(1);
        let page_size = ctx.page_size(request.limit, 200).max(3);
        let mut url = url::Url::parse(API).map_err(|e| AppError::internal(e.to_string()))?;
        {
            let mut pairs = url.query_pairs_mut();
            pairs.append_pair("key", &key);
            pairs.append_pair("q", query);
            pairs.append_pair("lang", "en");
            pairs.append_pair("page", &page.to_string());
            pairs.append_pair("per_page", &page_size.to_string());
            pairs.append_pair(
                "safesearch",
                if ctx.settings.safe_search {
                    "true"
                } else {
                    "false"
                },
            );
            match request.filters.media_type.as_str() {
                "photo" => pairs.append_pair("image_type", "photo"),
                "illustration" => pairs.append_pair("image_type", "illustration"),
                "vector" => pairs.append_pair("image_type", "vector"),
                _ => pairs.append_pair("image_type", "all"),
            };
            match request.filters.orientation.as_str() {
                "landscape" => pairs.append_pair("orientation", "horizontal"),
                "portrait" => pairs.append_pair("orientation", "vertical"),
                _ => &mut pairs,
            };
            if request.filters.min_width > 0 {
                pairs.append_pair("min_width", &request.filters.min_width.to_string());
            }
            if request.filters.min_height > 0 {
                pairs.append_pair("min_height", &request.filters.min_height.to_string());
            }
            if let Some(color) = pixabay_color(&request.filters.color) {
                pairs.append_pair("colors", color);
            }
            if request.sort == "newest" {
                pairs.append_pair("order", "latest");
            } else {
                pairs.append_pair("order", "popular");
            }
        }
        ctx.http.acquire("pixabay").await?;
        let value = ctx.http.get_json(url.as_str(), None).await?;
        parse_page(&value, page, page_size)
    }

    async fn details(&self, ctx: &ProviderContext, provider_asset_id: &str) -> AppResult<Asset> {
        let id = provider_asset_id
            .rsplit(':')
            .next()
            .unwrap_or(provider_asset_id);
        if id.is_empty() || !id.chars().all(|ch| ch.is_ascii_digit()) {
            return Err(AppError::validation("invalid Pixabay image id"));
        }
        let key = ctx.settings.pixabay_api_key.trim().to_string();
        if key.is_empty() {
            return Err(AppError::Policy("Pixabay requires an API key".into()));
        }
        let mut url = url::Url::parse(API).map_err(|e| AppError::internal(e.to_string()))?;
        {
            let mut pairs = url.query_pairs_mut();
            pairs.append_pair("key", &key);
            pairs.append_pair("id", id);
        }
        ctx.http.acquire("pixabay").await?;
        let value = ctx.http.get_json(url.as_str(), None).await?;
        let page = parse_page(&value, 1, 1)?;
        page.assets
            .into_iter()
            .next()
            .ok_or_else(|| AppError::NotFound("Pixabay has no image with that id".into()))
    }
}

/// Map the UI palette onto Pixabay's colour names.
fn pixabay_color(value: &str) -> Option<&'static str> {
    match value {
        "red" => Some("red"),
        "orange" => Some("orange"),
        "yellow" => Some("yellow"),
        "green" => Some("green"),
        "teal" => Some("turquoise"),
        "blue" => Some("blue"),
        "purple" => Some("lilac"),
        "pink" => Some("pink"),
        "brown" => Some("brown"),
        "grey" => Some("gray"),
        "black" => Some("black"),
        "white" => Some("white"),
        _ => None,
    }
}

pub(crate) fn parse_page(value: &Value, page: u32, page_size: u32) -> AppResult<ProviderPage> {
    let hits = value
        .get("hits")
        .and_then(Value::as_array)
        .ok_or_else(|| AppError::Malformed("Pixabay response has no hits array".into()))?;
    let total = value
        .get("totalHits")
        .and_then(Value::as_u64)
        .or_else(|| value.get("total").and_then(Value::as_u64));
    let assets = hits.iter().filter_map(parse_asset).collect::<Vec<_>>();
    let has_more = total
        .map(|total| u64::from(page) * u64::from(page_size) < total)
        .unwrap_or(!assets.is_empty());
    Ok(ProviderPage {
        assets,
        page,
        has_more,
        total,
    })
}

pub(crate) fn parse_asset(item: &Value) -> Option<Asset> {
    let id = item.get("id").and_then(Value::as_u64)?;
    let mut asset = Asset::new("pixabay", id.to_string());
    let tags = text_at(item, "tags");
    asset.title = tags.clone();
    asset.description = None;
    asset.page_url = text_at(item, "pageURL");
    asset.thumbnail_url = text_at(item, "webformatURL").or_else(|| text_at(item, "previewURL"));
    asset.preview_url = text_at(item, "largeImageURL").or_else(|| text_at(item, "webformatURL"));
    let original = text_at(item, "imageURL");
    let full_hd = text_at(item, "fullHDURL");
    let large = text_at(item, "largeImageURL");
    let web = text_at(item, "webformatURL");
    asset.download_url = original
        .clone()
        .or(full_hd.clone())
        .or(large.clone())
        .or(web.clone());
    asset.creator = text_at(item, "user");
    if let (Some(user), Some(user_id)) = (
        text_at(item, "user"),
        item.get("user_id").and_then(Value::as_u64),
    ) {
        asset.creator_url = Some(format!("https://pixabay.com/users/{user}-{user_id}/"));
    }
    asset.width = item
        .get("imageWidth")
        .and_then(Value::as_u64)
        .or_else(|| item.get("webformatWidth").and_then(Value::as_u64))
        .map(|value| value.min(u32::MAX as u64) as u32);
    asset.height = item
        .get("imageHeight")
        .and_then(Value::as_u64)
        .or_else(|| item.get("webformatHeight").and_then(Value::as_u64))
        .map(|value| value.min(u32::MAX as u64) as u32);
    asset.mime_type = Some(
        match extension_for(None, asset.download_url.as_deref().unwrap_or("")).as_str() {
            "png" => "image/png",
            "webp" => "image/webp",
            "svg" => "image/svg+xml",
            _ => "image/jpeg",
        }
        .to_string(),
    );
    asset.license_name = Some("Pixabay Content License".into());
    asset.license_url = Some("https://pixabay.com/service/license-summary/".into());
    asset.attribution = Some(format!(
        "Image by {} on Pixabay",
        asset.creator.as_deref().unwrap_or("a Pixabay contributor")
    ));
    asset.source_name = Some("Pixabay".into());
    asset.media_type = match text_at(item, "type").as_deref() {
        Some("vector") => "vector",
        Some("illustration") => "illustration",
        _ => "photo",
    }
    .into();
    asset.download_allowed = asset
        .download_url
        .as_deref()
        .map(is_allowed_web_url)
        .unwrap_or(false);
    asset.tags = tags
        .as_deref()
        .map(|tags| {
            tags.split(',')
                .map(str::trim)
                .filter(|tag| !tag.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    if original.is_none() && full_hd.is_none() {
        asset.provider_metadata.insert(
            "downloadNote".into(),
            "scaled preview from a basic Pixabay key (maximum 1280 px)".into(),
        );
    }
    asset
        .provider_metadata
        .insert("license".into(), "Pixabay Content License".into());
    Some(asset)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Value {
        serde_json::json!({
            "total": 4692,
            "totalHits": 500,
            "hits": [{
                "id": 195893,
                "pageURL": "https://pixabay.com/photos/blossom-bloom-flower-195893/",
                "type": "photo",
                "tags": "blossom, bloom, flower",
                "previewURL": "https://cdn.pixabay.com/photo/2013/10/15/09/12/flower-195893_150.jpg",
                "previewWidth": 150,
                "previewHeight": 84,
                "webformatURL": "https://pixabay.com/get/35bbf209e13e39d2_640.jpg",
                "webformatWidth": 640,
                "webformatHeight": 360,
                "largeImageURL": "https://pixabay.com/get/ed6a99fd0a76647_1280.jpg",
                "imageWidth": 4000,
                "imageHeight": 2250,
                "imageSize": 4731420,
                "views": 7671,
                "downloads": 6439,
                "likes": 5,
                "user_id": 48777,
                "user": "Josch13",
                "userImageURL": "https://cdn.pixabay.com/user/2013/11/05/02-10-23-764_250x250.jpg"
            }]
        })
    }

    #[test]
    fn parses_documented_hit_shape_and_falls_back_to_scaled_download() {
        let page = parse_page(&fixture(), 1, 24).unwrap();
        assert_eq!(page.total, Some(500));
        assert!(page.has_more);
        let asset = &page.assets[0];
        assert_eq!(asset.id, "pixabay:195893");
        assert_eq!(asset.title.as_deref(), Some("blossom, bloom, flower"));
        assert_eq!(asset.width, Some(4000));
        assert_eq!(asset.height, Some(2250));
        assert_eq!(
            asset.download_url.as_deref(),
            Some("https://pixabay.com/get/ed6a99fd0a76647_1280.jpg")
        );
        assert_eq!(
            asset.license_name.as_deref(),
            Some("Pixabay Content License")
        );
        assert!(asset.download_allowed);
        assert_eq!(
            asset
                .provider_metadata
                .get("downloadNote")
                .map(String::as_str),
            Some("scaled preview from a basic Pixabay key (maximum 1280 px)")
        );
        assert_eq!(asset.tags.len(), 3);
    }

    #[test]
    fn prefers_full_resolution_when_the_key_has_it() {
        let mut value = fixture();
        value["hits"][0]["imageURL"] =
            serde_json::json!("https://cdn.pixabay.com/photo/2013/10/15/09/12/flower-195893.jpg");
        let page = parse_page(&value, 1, 24).unwrap();
        assert_eq!(
            page.assets[0].download_url.as_deref(),
            Some("https://cdn.pixabay.com/photo/2013/10/15/09/12/flower-195893.jpg")
        );
        assert!(!page.assets[0]
            .provider_metadata
            .contains_key("downloadNote"));
    }

    #[test]
    fn missing_hits_is_malformed() {
        assert_eq!(
            parse_page(&serde_json::json!({}), 1, 24)
                .unwrap_err()
                .code(),
            "malformed"
        );
    }

    #[test]
    fn requires_a_key_and_never_scrapes() {
        let provider = PixabayProvider;
        let info = provider.info(false);
        assert!(info.requires_api_key);
        assert!(!info.has_api_key);
        assert!(!info.enabled_by_default);
        assert!(!info.policy.scraping_permitted);
        assert!(info
            .policy
            .docs_url
            .as_deref()
            .is_some_and(|url| url.contains("/api/docs/")));
    }

    #[test]
    fn colour_names_map_to_pixabay_palette() {
        assert_eq!(pixabay_color("teal"), Some("turquoise"));
        assert_eq!(pixabay_color("purple"), Some("lilac"));
        assert_eq!(pixabay_color("grey"), Some("gray"));
        assert_eq!(pixabay_color("any"), None);
    }
}
