//! Openverse adapter — anonymous official REST API only (`api.openverse.org/v1`).

use async_trait::async_trait;
use serde_json::Value;

use crate::error::{AppError, AppResult};
use crate::models::{Asset, ProviderInfo, ProviderPage, SearchRequest};
use crate::util::{extension_for, nearest_color_name, strip_html};

use super::{capabilities, info, policy, require_query, text_at, ImageProvider, ProviderContext};

pub struct OpenverseProvider;

const API: &str = "https://api.openverse.org/v1/images/";

#[async_trait]
impl ImageProvider for OpenverseProvider {
    fn id(&self) -> &'static str {
        "openverse"
    }

    fn name(&self) -> &'static str {
        "Openverse"
    }

    fn info(&self, _has_api_key: bool) -> ProviderInfo {
        info(
            "openverse",
            "Openverse",
            "Openverse (WordPress Foundation)",
            "https://openverse.org",
            "CC licences and public domain; attribution may be required",
            true,
            false,
            false,
            capabilities(
                true,
                false,
                true,
                true,
                false,
                &["relevance", "newest"],
                &["photo", "illustration", "artwork"],
            ),
            policy(
                "Official REST API (api.openverse.org/v1)",
                Some("https://api.openverse.org/v1/"),
                None,
                Some("https://openverse.org/terms/"),
                true,
                "anonymous: 20/min burst, 200/day sustained (live x-ratelimit headers)",
                "API only. The Openverse website and dataset are never scraped. Anonymous requests are rate limited; Stockcraft spaces requests, caches responses and opens a circuit on 429.",
            ),
            "No account or key needed. Anonymous daily budget is small, so Stockcraft caches results for a short time.",
        )
    }

    async fn search(
        &self,
        ctx: &ProviderContext,
        request: &SearchRequest,
    ) -> AppResult<ProviderPage> {
        let query = require_query(request)?;
        let page = request.page.max(1);
        let page_size = ctx.page_size(request.limit, 20);
        let mut url = url::Url::parse(API).map_err(|e| AppError::internal(e.to_string()))?;
        {
            let mut pairs = url.query_pairs_mut();
            pairs.append_pair("q", query);
            pairs.append_pair("page", &page.to_string());
            pairs.append_pair("page_size", &page_size.to_string());
            if ctx.settings.safe_search {
                pairs.append_pair("mature", "false");
            }
            match request.filters.orientation.as_str() {
                "landscape" => pairs.append_pair("aspect_ratio", "wide"),
                "portrait" => pairs.append_pair("aspect_ratio", "tall"),
                "square" => pairs.append_pair("aspect_ratio", "square"),
                _ => &mut pairs,
            };
            match request.filters.media_type.as_str() {
                "photo" => pairs.append_pair("category", "photograph"),
                "illustration" => pairs.append_pair("category", "illustration"),
                "artwork" => pairs.append_pair("category", "digitized_artwork"),
                _ => &mut pairs,
            };
            match request.filters.license.as_str() {
                "cc0" => pairs.append_pair("license", "cc0"),
                "by" => pairs.append_pair("license", "by"),
                "by-sa" => pairs.append_pair("license", "by-sa"),
                _ => &mut pairs,
            };
            if request.sort == "newest" {
                pairs.append_pair("ordering", "-created_on");
            }
        }

        ctx.http.acquire("openverse").await?;
        let value = ctx.http.get_json(url.as_str(), None).await?;
        parse_page(&value, page, page_size)
    }

    async fn details(&self, ctx: &ProviderContext, provider_asset_id: &str) -> AppResult<Asset> {
        let id = provider_asset_id
            .rsplit(':')
            .next()
            .unwrap_or(provider_asset_id);
        if id.is_empty()
            || id.len() > 128
            || !id.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
        {
            return Err(AppError::validation("invalid Openverse asset id"));
        }
        ctx.http.acquire("openverse").await?;
        let value = ctx.http.get_json(&format!("{API}{id}/"), None).await?;
        parse_asset(&value)
            .ok_or_else(|| AppError::Malformed("Openverse returned an incomplete asset".into()))
    }
}

pub(crate) fn parse_page(value: &Value, page: u32, page_size: u32) -> AppResult<ProviderPage> {
    let results = value
        .get("results")
        .and_then(Value::as_array)
        .ok_or_else(|| AppError::Malformed("Openverse response has no results array".into()))?;
    let total = value
        .get("result_count")
        .and_then(Value::as_u64)
        .or_else(|| {
            value
                .get("page_count")
                .and_then(Value::as_u64)
                .map(|pages| pages * u64::from(page_size))
        });
    let mut assets = Vec::new();
    for item in results {
        if let Some(asset) = parse_asset(item) {
            assets.push(asset);
        }
    }
    let has_more = total
        .map(|total| u64::from(page) * u64::from(page_size) < total)
        .unwrap_or_else(|| assets.len() >= page_size as usize);
    Ok(ProviderPage {
        assets,
        page,
        has_more,
        total,
    })
}

pub(crate) fn parse_asset(item: &Value) -> Option<Asset> {
    let id = text_at(item, "id")?;
    let mut asset = Asset::new("openverse", id);
    asset.title = text_at(item, "title");
    asset.page_url = text_at(item, "foreign_landing_url");
    asset.thumbnail_url = text_at(item, "thumbnail");
    asset.preview_url = asset.thumbnail_url.clone();
    asset.download_url = text_at(item, "url");
    asset.page_url = asset.page_url.or_else(|| text_at(item, "detail_url"));
    asset.creator = text_at(item, "creator").map(|creator| strip_html(&creator));
    asset.creator_url = text_at(item, "creator_url");
    asset.width = super::number_at(item, "width").map(|w| w.min(u32::MAX as u64) as u32);
    asset.height = super::number_at(item, "height").map(|h| h.min(u32::MAX as u64) as u32);
    let filetype = text_at(item, "filetype");
    asset.mime_type = mime_from_filetype(filetype.as_deref(), asset.download_url.as_deref());
    let license = text_at(item, "license");
    let version = text_at(item, "license_version");
    asset.license_name = license_name(license.as_deref(), version.as_deref());
    asset.license_url = text_at(item, "license_url");
    asset.attribution = text_at(item, "attribution").map(|text| strip_html(&text));
    let source = text_at(item, "source");
    asset.source_name = Some(match source.as_deref() {
        Some(source) => format!("{} (via Openverse)", title_case(source)),
        None => "Openverse".into(),
    });
    asset.tags = item
        .get("tags")
        .and_then(Value::as_array)
        .map(|tags| {
            tags.iter()
                .filter_map(|tag| text_at(tag, "name"))
                .take(12)
                .collect()
        })
        .unwrap_or_default();
    asset.media_type = match text_at(item, "category").as_deref() {
        Some("photograph") => "photo",
        Some("illustration") => "illustration",
        Some("digitized_artwork") => "artwork",
        Some("map") => "map",
        _ => "photo",
    }
    .to_string();
    asset.download_allowed = asset
        .download_url
        .as_deref()
        .map(crate::util::is_allowed_web_url)
        .unwrap_or(false);
    asset.dominant_color = text_at(item, "color").and_then(|color| nearest_color_name(&color));
    asset.created_at = text_at(item, "created_on")
        .and_then(|date| crate::util::iso_timestamp(&date))
        .or_else(|| text_at(item, "indexed_on").and_then(|date| crate::util::iso_timestamp(&date)));
    if let Some(provider) = text_at(item, "provider") {
        asset
            .provider_metadata
            .insert("openverseProvider".into(), provider);
    }
    if let Some(source) = text_at(item, "source") {
        asset
            .provider_metadata
            .insert("openverseSource".into(), source);
    }
    if let Some(license) = license {
        asset.provider_metadata.insert("license".into(), license);
    }
    if let Some(mature) = super::bool_at(item, "mature") {
        asset
            .provider_metadata
            .insert("mature".into(), mature.to_string());
    }
    Some(asset)
}

fn license_name(license: Option<&str>, version: Option<&str>) -> Option<String> {
    let license = license?;
    let base = match license {
        "cc0" => "CC0".to_string(),
        "pdm" => "Public domain mark".to_string(),
        other => format!("CC {}", other.to_ascii_uppercase()),
    };
    Some(match version {
        Some(version) if license != "pdm" => format!("{base} {version}"),
        _ => base,
    })
}

fn mime_from_filetype(filetype: Option<&str>, url: Option<&str>) -> Option<String> {
    if let Some(ext) = filetype {
        let ext = ext.to_ascii_lowercase();
        let mime = match ext.as_str() {
            "jpg" | "jpeg" => "image/jpeg",
            "png" => "image/png",
            "webp" => "image/webp",
            "gif" => "image/gif",
            "svg" => "image/svg+xml",
            "tif" | "tiff" => "image/tiff",
            _ => return None,
        };
        return Some(mime.into());
    }
    url.filter(|url| !url.is_empty())
        .map(|url| match extension_for(None, url).as_str() {
            "png" => "image/png",
            "webp" => "image/webp",
            "gif" => "image/gif",
            "svg" => "image/svg+xml",
            "tif" => "image/tiff",
            _ => "image/jpeg",
        })
        .map(str::to_string)
}

fn title_case(text: &str) -> String {
    text.split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Value {
        serde_json::json!({
            "result_count": 240,
            "page_count": 12,
            "page_size": 20,
            "page": 1,
            "results": [{
                "id": "f561777d-24ea-483f-9cf8-f17aa1fd6aa3",
                "title": "Mountains",
                "indexed_on": "2020-03-27T18:10:35.161824Z",
                "foreign_landing_url": "https://www.flickr.com/photos/22178197@N00/16129656628",
                "url": "https://live.staticflickr.com/7539/16129656628_ddd1db38c2_b.jpg",
                "creator": "Kamil Porembiński",
                "creator_url": "https://www.flickr.com/photos/22178197@N00",
                "license": "by-sa",
                "license_version": "2.0",
                "license_url": "https://creativecommons.org/licenses/by-sa/2.0/",
                "provider": "flickr",
                "source": "flickr",
                "category": "photograph",
                "tags": [{"name": "mountain"}, {"name": "landscape"}],
                "attribution": "\"Mountains\" by Kamil Porembiński is licensed under CC BY-SA 2.0.",
                "mature": false,
                "height": 680,
                "width": 1024,
                "thumbnail": "https://api.openverse.org/v1/images/f561777d-24ea-483f-9cf8-f17aa1fd6aa3/thumb/",
                "detail_url": "https://api.openverse.org/v1/images/f561777d-24ea-483f-9cf8-f17aa1fd6aa3/"
            }]
        })
    }

    #[test]
    fn parses_the_documented_response_shape() {
        let page = parse_page(&fixture(), 1, 20).unwrap();
        assert_eq!(page.total, Some(240));
        assert!(page.has_more);
        let asset = &page.assets[0];
        assert_eq!(asset.id, "openverse:f561777d-24ea-483f-9cf8-f17aa1fd6aa3");
        assert_eq!(asset.title.as_deref(), Some("Mountains"));
        assert_eq!(asset.license_name.as_deref(), Some("CC BY-SA 2.0"));
        assert_eq!(asset.media_type, "photo");
        assert!(asset.download_allowed);
        assert_eq!(
            asset.tags,
            vec!["mountain".to_string(), "landscape".to_string()]
        );
        assert_eq!(asset.source_name.as_deref(), Some("Flickr (via Openverse)"));
        assert_eq!(asset.mime_type.as_deref(), Some("image/jpeg"));
    }

    #[test]
    fn missing_results_is_an_error_not_an_empty_success() {
        let error = parse_page(&serde_json::json!({"oops": true}), 1, 20).unwrap_err();
        assert_eq!(error.code(), "malformed");
    }

    #[test]
    fn pagination_stops_at_the_total() {
        let mut value = fixture();
        value["result_count"] = serde_json::json!(20);
        let page = parse_page(&value, 1, 20).unwrap();
        assert!(!page.has_more);
    }
}
