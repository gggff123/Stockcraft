//! Optional Freepik (now Magnific) stock-search adapter.
//!
//! Freepik's public pages cannot be searched: `freepik.com/robots.txt` disallows `/search?` and
//! `/*/search?`, and download paths. The documented route is the Magnific Resources API, which
//! requires an API key and a paid/enterprise plan.
//!
//! This adapter is deliberately **search/preview-only**. The API exposes licensed preview images,
//! not free downloads, so every result sets `download_allowed = false` and links back to the
//! Freepik page and licence. Stockcraft does not scrape Freepik.

use async_trait::async_trait;
use serde_json::Value;

use crate::error::{AppError, AppResult};
use crate::models::{Asset, ProviderInfo, ProviderPage, SearchRequest};
use crate::util::extension_for;

use super::{capabilities, info, policy, require_query, text_at, ImageProvider, ProviderContext};

pub struct FreepikProvider;

const API: &str = "https://api.magnific.com/v1/resources";
const KEY_HEADER: &str = "x-magnific-api-key";

#[async_trait]
impl ImageProvider for FreepikProvider {
    fn id(&self) -> &'static str {
        "freepik"
    }

    fn name(&self) -> &'static str {
        "Freepik (Magnific)"
    }

    fn requires_api_key(&self) -> bool {
        true
    }

    fn api_key(&self, settings: &crate::models::AppSettings) -> Option<String> {
        Some(settings.freepik_api_key.clone())
    }

    fn info(&self, has_api_key: bool) -> ProviderInfo {
        let mut provider = info(
            "freepik",
            "Freepik (Magnific)",
            "Freepik / Magnific stock library",
            "https://www.freepik.com",
            "Freemium or premium Freepik licence; attribution and a plan may be required",
            false,
            true,
            has_api_key,
            capabilities(
                true,
                false,
                true,
                false,
                false,
                &["relevance", "newest"],
                &["photo", "vector", "illustration"],
            ),
            policy(
                "Official Magnific Resources API (api.magnific.com/v1/resources, key required)",
                Some("https://docs.magnific.com/api-reference/resources/get-all-resources"),
                Some("https://www.freepik.com/robots.txt"),
                Some("https://www.freepik.com/terms-of-use"),
                false,
                "API-plan dependent; Stockcraft sends at most one request per second",
                "robots.txt disallows /search? and download paths, so no public-page adapter exists. The API returns licensed preview images; Stockcraft marks Freepik results as view-only and links to the source page and licence. It never downloads Freepik assets.",
            ),
            "Optional. Add a Freepik / Magnific API key in Settings → Sources to search licensed stock. Results are preview-only; downloading requires a Freepik plan and is not offered in Stockcraft.",
        );
        provider.capabilities.download = false;
        provider.contact_url = Some("https://www.magnific.com/api".into());
        provider
    }

    async fn search(
        &self,
        ctx: &ProviderContext,
        request: &SearchRequest,
    ) -> AppResult<ProviderPage> {
        let key = ctx.settings.freepik_api_key.trim().to_string();
        if key.is_empty() {
            return Err(AppError::Policy(
                "Freepik / Magnific requires an API key; add one in Settings → Sources".into(),
            ));
        }
        let query = require_query(request)?;
        let page = request.page.max(1);
        if page > 100 {
            return Ok(ProviderPage {
                assets: Vec::new(),
                page,
                has_more: false,
                total: None,
            });
        }
        let page_size = ctx.page_size(request.limit, 100).max(1);
        let mut url = url::Url::parse(API).map_err(|e| AppError::internal(e.to_string()))?;
        {
            let mut pairs = url.query_pairs_mut();
            pairs.append_pair("term", query);
            pairs.append_pair("page", &page.to_string());
            pairs.append_pair("limit", &page_size.to_string());
            pairs.append_pair(
                "order",
                if request.sort == "newest" {
                    "recent"
                } else {
                    "relevance"
                },
            );
            match request.filters.orientation.as_str() {
                "landscape" => pairs.append_pair("filters[orientation][landscape]", "1"),
                "portrait" => pairs.append_pair("filters[orientation][portrait]", "1"),
                "square" => pairs.append_pair("filters[orientation][square]", "1"),
                _ => &mut pairs,
            };
            match request.filters.media_type.as_str() {
                "photo" => pairs.append_pair("filters[content_type][photo]", "1"),
                "vector" => pairs.append_pair("filters[content_type][vector]", "1"),
                _ => &mut pairs,
            };
        }
        ctx.http.acquire("freepik").await?;
        let value = ctx
            .http
            .get_json_with_header(url.as_str(), KEY_HEADER, &key)
            .await?;
        parse_page(&value, page, page_size)
    }

    async fn details(&self, ctx: &ProviderContext, provider_asset_id: &str) -> AppResult<Asset> {
        let id = provider_asset_id
            .rsplit(':')
            .next()
            .unwrap_or(provider_asset_id);
        if id.is_empty() || !id.chars().all(|ch| ch.is_ascii_digit()) {
            return Err(AppError::validation("invalid Freepik resource id"));
        }
        let key = ctx.settings.freepik_api_key.trim().to_string();
        if key.is_empty() {
            return Err(AppError::Policy(
                "Freepik / Magnific requires an API key".into(),
            ));
        }
        let mut url = url::Url::parse(API).map_err(|e| AppError::internal(e.to_string()))?;
        {
            let mut pairs = url.query_pairs_mut();
            pairs.append_pair("page", "1");
            pairs.append_pair("limit", "1");
            pairs.append_pair("filters[ids]", id);
        }
        ctx.http.acquire("freepik").await?;
        let value = ctx
            .http
            .get_json_with_header(url.as_str(), KEY_HEADER, &key)
            .await?;
        let page = parse_page(&value, 1, 1)?;
        page.assets
            .into_iter()
            .next()
            .ok_or_else(|| AppError::NotFound("Freepik has no resource with that id".into()))
    }
}

pub(crate) fn parse_page(value: &Value, page: u32, page_size: u32) -> AppResult<ProviderPage> {
    let data = value
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| AppError::Malformed("Freepik response has no data array".into()))?;
    let meta = value.get("meta");
    let total = meta
        .and_then(|meta| meta.get("total"))
        .and_then(Value::as_u64);
    let last_page = meta
        .and_then(|meta| meta.get("last_page"))
        .and_then(Value::as_u64);
    let current_page = meta
        .and_then(|meta| meta.get("current_page"))
        .and_then(Value::as_u64)
        .unwrap_or(u64::from(page));
    let has_more = last_page
        .map(|last| current_page < last)
        .unwrap_or_else(|| data.len() as u32 >= page_size);
    let assets = data.iter().filter_map(parse_asset).collect::<Vec<_>>();
    Ok(ProviderPage {
        assets,
        page,
        has_more,
        total,
    })
}

pub(crate) fn parse_asset(item: &Value) -> Option<Asset> {
    let id = item.get("id").and_then(Value::as_u64)?;
    let mut asset = Asset::new("freepik", id.to_string());
    asset.title = text_at(item, "title");
    asset.description = None;
    asset.page_url = text_at(item, "url");
    let thumbnail = text_at(item, "image.source.url")?;
    asset.thumbnail_url = Some(thumbnail.clone());
    asset.preview_url = Some(thumbnail);
    asset.download_url = None;
    asset.creator = text_at(item, "author.name");
    if let Some(slug) = text_at(item, "author.slug") {
        asset.creator_url = Some(format!("https://www.freepik.com/author/{slug}"));
    }
    if let Some(size) = text_at(item, "image.source.size") {
        if let Some((width, height)) = parse_size(&size) {
            asset.width = Some(width);
            asset.height = Some(height);
        }
    }
    asset.mime_type = Some(
        match extension_for(None, asset.thumbnail_url.as_deref().unwrap_or("")).as_str() {
            "png" => "image/png",
            "webp" => "image/webp",
            "svg" => "image/svg+xml",
            _ => "image/jpeg",
        }
        .to_string(),
    );
    let license = item
        .get("licenses")
        .and_then(Value::as_array)
        .and_then(|licenses| licenses.first());
    let license_type = license
        .and_then(|license| license.get("type"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let license_url = license
        .and_then(|license| license.get("url"))
        .and_then(Value::as_str)
        .map(str::to_string);
    asset.license_name = Some(match license_type.as_deref() {
        Some(kind) => format!("Freepik {kind} licence"),
        None => "Freepik licence".into(),
    });
    asset.license_url = license_url.or_else(|| Some("https://www.freepik.com/terms-of-use".into()));
    asset.attribution = Some(format!(
        "Freepik resource by {}. A Freepik licence is required for reuse.",
        asset.creator.as_deref().unwrap_or("a Freepik contributor")
    ));
    asset.source_name = Some("Freepik (Magnific)".into());
    asset.media_type = match text_at(item, "image.type").as_deref() {
        Some("vector") => "vector",
        Some("psd") => "illustration",
        _ => "photo",
    }
    .into();
    // The API exposes licensed previews, not free downloads. The UI shows Open original page.
    asset.download_allowed = false;
    asset.created_at = text_at(item, "meta.published_at");
    if let Some(keywords) = text_at(item, "related.keywords.name") {
        asset.tags = keywords
            .split(',')
            .map(str::trim)
            .filter(|tag| !tag.is_empty())
            .map(str::to_string)
            .collect();
    }
    if let Some(kind) = license_type {
        asset.provider_metadata.insert("licenseType".into(), kind);
    }
    asset
        .provider_metadata
        .insert("api".into(), "magnific-resources-v1".into());
    Some(asset)
}

fn parse_size(value: &str) -> Option<(u32, u32)> {
    let (width, height) = value.split_once('x')?;
    Some((
        width.trim().parse::<u32>().ok()?,
        height.trim().parse::<u32>().ok()?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Value {
        serde_json::json!({
            "data": [{
                "id": 15667327,
                "title": "White t-shirt with copy space on gray background",
                "url": "https://www.freepik.com/free-photo/white-t-shirts-with-copy-space-gray-background_15667327.htm",
                "image": {
                    "orientation": "horizontal",
                    "type": "photo",
                    "source": {
                        "size": "740x640",
                        "key": "large",
                        "url": "https://img.freepik.com/free-photo/tshirt_53876-104920.jpg"
                    }
                },
                "licenses": [{
                    "type": "freemium",
                    "url": "https://www.freepik.com/profile/license/pdf/4350594"
                }],
                "author": {
                    "name": "John Doe",
                    "id": 2147483647,
                    "avatar": "https://avatar.cdnpk.net/61668527-220726032514.jpg",
                    "slug": "john-doe"
                },
                "meta": {"published_at": "2020-12-01T23:50:50.000Z"},
                "related": {"keywords": {"name": "tshirt, copy space"}}
            }],
            "meta": {
                "per_page": 1,
                "total": 1,
                "last_page": 1,
                "current_page": 1,
                "clean_search": true
            }
        })
    }

    #[test]
    fn parses_documented_resource_and_marks_it_view_only() {
        let page = parse_page(&fixture(), 1, 24).unwrap();
        assert_eq!(page.total, Some(1));
        assert!(!page.has_more);
        let asset = &page.assets[0];
        assert_eq!(asset.id, "freepik:15667327");
        assert_eq!(asset.width, Some(740));
        assert_eq!(asset.height, Some(640));
        assert_eq!(
            asset.thumbnail_url.as_deref(),
            Some("https://img.freepik.com/free-photo/tshirt_53876-104920.jpg")
        );
        assert!(!asset.download_allowed);
        assert_eq!(asset.download_url, None);
        assert_eq!(
            asset.license_name.as_deref(),
            Some("Freepik freemium licence")
        );
        assert_eq!(
            asset.license_url.as_deref(),
            Some("https://www.freepik.com/profile/license/pdf/4350594")
        );
        assert_eq!(asset.source_name.as_deref(), Some("Freepik (Magnific)"));
        assert_eq!(
            asset.tags,
            vec!["tshirt".to_string(), "copy space".to_string()]
        );
    }

    #[test]
    fn missing_data_is_malformed() {
        assert_eq!(
            parse_page(&serde_json::json!({}), 1, 24)
                .unwrap_err()
                .code(),
            "malformed"
        );
    }

    #[test]
    fn requires_a_key_and_never_scrapes_or_downloads() {
        let provider = FreepikProvider;
        let info = provider.info(false);
        assert!(info.requires_api_key);
        assert!(!info.has_api_key);
        assert!(!info.enabled_by_default);
        assert!(!info.policy.scraping_permitted);
        assert!(info
            .policy
            .docs_url
            .as_deref()
            .is_some_and(|url| url.contains("docs.magnific.com")));
        let page = parse_page(&fixture(), 1, 24).unwrap();
        assert!(page.assets.iter().all(|asset| !asset.download_allowed));
    }

    #[test]
    fn parses_source_size() {
        assert_eq!(parse_size("740x640"), Some((740, 640)));
        assert_eq!(parse_size("bad"), None);
    }
}
