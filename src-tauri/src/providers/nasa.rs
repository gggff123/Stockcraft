//! NASA Image and Video Library adapter — public JSON API, images only.

use async_trait::async_trait;
use serde_json::Value;

use crate::error::{AppError, AppResult};
use crate::models::{Asset, ProviderInfo, ProviderPage, SearchRequest};
use crate::util::{extension_for, iso_timestamp, strip_html};

use super::{capabilities, info, policy, require_query, text_at, ImageProvider, ProviderContext};

pub struct NasaProvider;

const API: &str = "https://images-api.nasa.gov/search";

#[async_trait]
impl ImageProvider for NasaProvider {
    fn id(&self) -> &'static str {
        "nasa"
    }

    fn name(&self) -> &'static str {
        "NASA Image Library"
    }

    fn info(&self, _has_api_key: bool) -> ProviderInfo {
        info(
            "nasa",
            "NASA Image Library",
            "NASA Image and Video Library",
            "https://images.nasa.gov",
            "NASA media usage guidelines (mostly public domain, not all)",
            true,
            false,
            false,
            capabilities(false, false, false, false, false, &["relevance"], &["photo"]),
            policy(
                "Public JSON API (images-api.nasa.gov)",
                Some("https://images.nasa.gov/docs/images.nasa.gov_api_docs.pdf"),
                None,
                Some("https://www.nasa.gov/nasa-brand-center/images-and-media/"),
                true,
                "undocumented public limits; Stockcraft sends at most one request per second",
                "Not all NASA media is public domain. The per-item metadata and NASA usage guidelines are shown; Stockcraft never claims a NASA image is free of third-party rights.",
            ),
            "Space and Earth imagery, mission photography and illustrations from NASA's archive.",
        )
    }

    async fn search(
        &self,
        ctx: &ProviderContext,
        request: &SearchRequest,
    ) -> AppResult<ProviderPage> {
        let query = require_query(request)?;
        let page = request.page.max(1);
        let page_size = ctx.page_size(request.limit, 100);
        let mut url = url::Url::parse(API).map_err(|e| AppError::internal(e.to_string()))?;
        {
            let mut pairs = url.query_pairs_mut();
            pairs.append_pair("q", query);
            pairs.append_pair("media_type", "image");
            pairs.append_pair("page", &page.to_string());
            pairs.append_pair("page_size", &page_size.to_string());
        }
        ctx.http.acquire("nasa").await?;
        let value = ctx.http.get_json(url.as_str(), None).await?;
        parse_page(&value, page, page_size)
    }

    async fn details(&self, ctx: &ProviderContext, provider_asset_id: &str) -> AppResult<Asset> {
        let id = provider_asset_id
            .rsplit(':')
            .next()
            .unwrap_or(provider_asset_id);
        if id.is_empty()
            || id.len() > 64
            || !id
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
        {
            return Err(AppError::validation("invalid NASA asset id"));
        }
        let mut url = url::Url::parse(API).map_err(|e| AppError::internal(e.to_string()))?;
        url.query_pairs_mut().append_pair("nasa_id", id);
        ctx.http.acquire("nasa").await?;
        let value = ctx.http.get_json(url.as_str(), None).await?;
        let items = value
            .pointer("/collection/items")
            .and_then(Value::as_array)
            .ok_or_else(|| AppError::NotFound("NASA asset not found".into()))?;
        items
            .iter()
            .find_map(parse_item)
            .ok_or_else(|| AppError::Malformed("NASA returned an incomplete asset".into()))
    }
}

pub(crate) fn parse_page(value: &Value, page: u32, page_size: u32) -> AppResult<ProviderPage> {
    let items = value
        .pointer("/collection/items")
        .and_then(Value::as_array)
        .ok_or_else(|| AppError::Malformed("NASA response has no collection.items array".into()))?;
    let total = value
        .pointer("/collection/metadata/total_hits")
        .and_then(Value::as_u64);
    let mut assets = Vec::new();
    for item in items {
        if let Some(asset) = parse_item(item) {
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

pub(crate) fn parse_item(item: &Value) -> Option<Asset> {
    let data = item.get("data").and_then(Value::as_array)?.first()?;
    let nasa_id = text_at(data, "nasa_id")?;
    if text_at(data, "media_type").as_deref() != Some("image") {
        return None;
    }
    let links = item.get("links").and_then(Value::as_array);
    let find = |rel: &str| {
        links.into_iter().flatten().find(|link| {
            text_at(link, "rel").as_deref() == Some(rel)
                && text_at(link, "render").as_deref() == Some("image")
        })
    };
    let canonical = find("canonical");
    let preview = find("preview").or(canonical);
    let alternate = links.into_iter().flatten().find(|link| {
        text_at(link, "rel").as_deref() == Some("alternate")
            && text_at(link, "render").as_deref() == Some("image")
            && super::number_at(link, "width")
                .map(|width| width <= 1600)
                .unwrap_or(true)
    });
    let download_url = canonical
        .or(alternate)
        .and_then(|link| text_at(link, "href"))
        .or_else(|| preview.and_then(|link| text_at(link, "href")))?;

    let mut asset = Asset::new("nasa", nasa_id.clone());
    asset.title = text_at(data, "title");
    asset.description = text_at(data, "description").map(|text| {
        let text = strip_html(&text);
        text.chars().take(1500).collect()
    });
    asset.page_url = Some(format!("https://images.nasa.gov/details/{nasa_id}"));
    asset.thumbnail_url = preview.and_then(|link| text_at(link, "href"));
    asset.preview_url = alternate
        .and_then(|link| text_at(link, "href"))
        .or_else(|| canonical.and_then(|link| text_at(link, "href")))
        .or_else(|| asset.thumbnail_url.clone());
    asset.download_url = Some(download_url.clone());
    asset.creator = text_at(data, "photographer")
        .or_else(|| text_at(data, "secondary_creator"))
        .or_else(|| text_at(data, "creator"))
        .or_else(|| text_at(data, "center"))
        .map(|text| strip_html(&text));
    asset.width = canonical
        .and_then(|link| super::number_at(link, "width"))
        .or_else(|| alternate.and_then(|link| super::number_at(link, "width")))
        .map(|value| value.min(u32::MAX as u64) as u32);
    asset.height = canonical
        .and_then(|link| super::number_at(link, "height"))
        .or_else(|| alternate.and_then(|link| super::number_at(link, "height")))
        .map(|value| value.min(u32::MAX as u64) as u32);
    asset.mime_type = Some(
        match extension_for(None, &download_url).as_str() {
            "png" => "image/png",
            "gif" => "image/gif",
            "tif" => "image/tiff",
            _ => "image/jpeg",
        }
        .to_string(),
    );
    asset.license_name = Some("NASA media usage guidelines".into());
    asset.license_url = Some("https://www.nasa.gov/nasa-brand-center/images-and-media/".into());
    asset.attribution = Some(format!(
        "\"{}\" — NASA ({})",
        asset.title.as_deref().unwrap_or("NASA image"),
        text_at(data, "center").unwrap_or_else(|| "NASA".into())
    ));
    asset.source_name = Some("NASA Image and Video Library".into());
    asset.tags = data
        .get("keywords")
        .and_then(Value::as_array)
        .map(|tags| {
            tags.iter()
                .filter_map(|tag| tag.as_str())
                .map(|tag| tag.trim().to_string())
                .filter(|tag| !tag.is_empty())
                .take(12)
                .collect()
        })
        .unwrap_or_default();
    asset.media_type = "photo".into();
    asset.download_allowed = crate::util::is_allowed_web_url(&download_url);
    asset.created_at = text_at(data, "date_created").and_then(|date| iso_timestamp(&date));
    if let Some(center) = text_at(data, "center") {
        asset.provider_metadata.insert("center".into(), center);
    }
    asset.provider_metadata.insert(
        "usageGuidelines".into(),
        "Not all NASA media is public domain".into(),
    );
    Some(asset)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Value {
        serde_json::json!({
            "collection": {
                "items": [{
                    "href": "https://images-assets.nasa.gov/image/PIA14417/collection.json",
                    "data": [{
                        "center": "JPL",
                        "date_created": "2011-08-10T21:00:09Z",
                        "description": "The <b>Dumbbell nebula</b>.",
                        "keywords": ["Helix Nebula", "Spitzer Space Telescope"],
                        "media_type": "image",
                        "nasa_id": "PIA14417",
                        "secondary_creator": "NASA/JPL-Caltech",
                        "title": "Weighing in on the Dumbbell Nebula"
                    }],
                    "links": [
                        {"href": "https://images-assets.nasa.gov/image/PIA14417/PIA14417~medium.jpg", "rel": "alternate", "render": "image", "width": 1280, "height": 640},
                        {"href": "https://images-assets.nasa.gov/image/PIA14417/PIA14417~thumb.jpg", "rel": "preview", "render": "image", "width": 640, "height": 320},
                        {"href": "https://images-assets.nasa.gov/image/PIA14417/PIA14417~orig.jpg", "rel": "canonical", "render": "image", "width": 2800, "height": 1400}
                    ]
                }],
                "metadata": {"total_hits": 120}
            }
        })
    }

    #[test]
    fn parses_nasa_links_and_picks_canonical_download() {
        let page = parse_page(&fixture(), 1, 24).unwrap();
        assert_eq!(page.total, Some(120));
        assert!(page.has_more);
        let asset = &page.assets[0];
        assert_eq!(asset.id, "nasa:PIA14417");
        assert_eq!(asset.width, Some(2800));
        assert_eq!(asset.height, Some(1400));
        assert_eq!(
            asset.download_url.as_deref(),
            Some("https://images-assets.nasa.gov/image/PIA14417/PIA14417~orig.jpg")
        );
        assert!(asset
            .thumbnail_url
            .as_deref()
            .unwrap()
            .contains("~thumb.jpg"));
        assert_eq!(asset.creator.as_deref(), Some("NASA/JPL-Caltech"));
        assert_eq!(asset.tags.len(), 2);
        assert!(asset.download_allowed);
        assert!(asset
            .description
            .as_ref()
            .unwrap()
            .contains("Dumbbell nebula"));
    }

    #[test]
    fn replaces_the_license_claim_with_honest_guidelines() {
        let asset = parse_asset_from_fixture();
        assert_eq!(
            asset.license_name.as_deref(),
            Some("NASA media usage guidelines")
        );
        assert!(asset.provider_metadata.contains_key("usageGuidelines"));
    }

    fn parse_asset_from_fixture() -> Asset {
        parse_page(&fixture(), 1, 24).unwrap().assets.remove(0)
    }

    #[test]
    fn video_items_are_skipped() {
        let mut value = fixture();
        value["collection"]["items"][0]["data"][0]["media_type"] = serde_json::json!("video");
        assert!(parse_page(&value, 1, 24).unwrap().assets.is_empty());
    }

    #[test]
    fn missing_collection_is_malformed() {
        assert_eq!(
            parse_page(&serde_json::json!({}), 1, 24)
                .unwrap_err()
                .code(),
            "malformed"
        );
    }
}
