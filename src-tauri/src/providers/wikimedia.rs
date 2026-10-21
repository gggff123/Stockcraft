//! Wikimedia Commons adapter — MediaWiki Action API, anonymously, with a descriptive User-Agent.

use async_trait::async_trait;
use serde_json::Value;

use crate::error::{AppError, AppResult};
use crate::models::{Asset, ProviderInfo, ProviderPage, SearchRequest};
use crate::util::{iso_timestamp, strip_html};

use super::{capabilities, info, policy, require_query, text_at, ImageProvider, ProviderContext};

pub struct WikimediaProvider;

const API: &str = "https://commons.wikimedia.org/w/api.php";

#[async_trait]
impl ImageProvider for WikimediaProvider {
    fn id(&self) -> &'static str {
        "wikimedia"
    }

    fn name(&self) -> &'static str {
        "Wikimedia Commons"
    }

    fn info(&self, _has_api_key: bool) -> ProviderInfo {
        info(
            "wikimedia",
            "Wikimedia Commons",
            "Wikimedia Commons",
            "https://commons.wikimedia.org",
            "Per-file licence (CC, public domain); check each file page",
            true,
            false,
            false,
            capabilities(false, false, true, false, false, &["relevance"], &["photo", "illustration", "diagram", "map"]),
            policy(
                "MediaWiki Action API (commons.wikimedia.org/w/api.php)",
                Some("https://www.mediawiki.org/wiki/API:Search"),
                Some("https://commons.wikimedia.org/robots.txt"),
                Some("https://foundation.wikimedia.org/wiki/Policy:Terms_of_Use"),
                true,
                "MediaWiki API etiquette; one serial request per second, 50 results per page",
                "API only; descriptive User-Agent required. File licence and artist come from each file's extmetadata.",
            ),
            "Encyclopaedic media: photographs, diagrams, maps and historical artwork. Licence varies per file.",
        )
    }

    async fn search(
        &self,
        ctx: &ProviderContext,
        request: &SearchRequest,
    ) -> AppResult<ProviderPage> {
        let query = require_query(request)?;
        let page = request.page.max(1);
        let page_size = ctx.page_size(request.limit, 50);
        let offset = (page.saturating_sub(1)) * page_size;
        let mut url = url::Url::parse(API).map_err(|e| AppError::internal(e.to_string()))?;
        {
            let mut pairs = url.query_pairs_mut();
            pairs.append_pair("action", "query");
            pairs.append_pair("format", "json");
            pairs.append_pair("formatversion", "2");
            pairs.append_pair("generator", "search");
            pairs.append_pair("gsrsearch", query);
            pairs.append_pair("gsrnamespace", "6");
            pairs.append_pair("gsrlimit", &page_size.to_string());
            pairs.append_pair("gsroffset", &offset.to_string());
            pairs.append_pair("prop", "imageinfo");
            pairs.append_pair("iiprop", "url|size|mime|extmetadata|user");
            pairs.append_pair("iiurlwidth", "800");
            if request.sort == "newest" {
                pairs.append_pair("gsrsort", "create_timestamp_desc");
            }
        }
        ctx.http.acquire("wikimedia").await?;
        let value = ctx.http.get_json(url.as_str(), None).await?;
        parse_page(&value, page)
    }

    async fn details(&self, ctx: &ProviderContext, provider_asset_id: &str) -> AppResult<Asset> {
        let id = provider_asset_id
            .rsplit(':')
            .next()
            .unwrap_or(provider_asset_id);
        if id.is_empty() || !id.chars().all(|ch| ch.is_ascii_digit()) {
            return Err(AppError::validation("invalid Wikimedia page id"));
        }
        let mut url = url::Url::parse(API).map_err(|e| AppError::internal(e.to_string()))?;
        {
            let mut pairs = url.query_pairs_mut();
            pairs.append_pair("action", "query");
            pairs.append_pair("format", "json");
            pairs.append_pair("formatversion", "2");
            pairs.append_pair("pageids", id);
            pairs.append_pair("prop", "imageinfo");
            pairs.append_pair("iiprop", "url|size|mime|extmetadata|user");
            pairs.append_pair("iiurlwidth", "1200");
        }
        ctx.http.acquire("wikimedia").await?;
        let value = ctx.http.get_json(url.as_str(), None).await?;
        let page = value
            .pointer("/query/pages/0")
            .ok_or_else(|| AppError::NotFound("Wikimedia file not found".into()))?;
        parse_item(page)
            .ok_or_else(|| AppError::Malformed("Wikimedia returned an incomplete file".into()))
    }
}

pub(crate) fn parse_page(value: &Value, page: u32) -> AppResult<ProviderPage> {
    let pages = value.pointer("/query/pages").and_then(Value::as_array);
    let total = value
        .pointer("/query/searchinfo/totalhits")
        .and_then(Value::as_u64);
    let has_continue = value.get("continue").is_some();
    let mut assets = Vec::new();
    for item in pages.into_iter().flatten() {
        if let Some(asset) = parse_item(item) {
            assets.push(asset);
        }
    }
    Ok(ProviderPage {
        has_more: has_continue,
        assets,
        page,
        total,
    })
}

pub(crate) fn parse_item(item: &Value) -> Option<Asset> {
    let page_id = item.get("pageid").and_then(Value::as_u64)?;
    let info = item.get("imageinfo").and_then(Value::as_array)?.first()?;
    let mime = text_at(info, "mime")?;
    if !mime.starts_with("image/") {
        return None;
    }
    let mut asset = Asset::new("wikimedia", page_id.to_string());
    let raw_title = text_at(item, "title").unwrap_or_default();
    asset.title = Some(
        raw_title
            .strip_prefix("File:")
            .unwrap_or(&raw_title)
            .replace('_', " "),
    );
    asset.description = meta(info, "ImageDescription")
        .map(|text| strip_html(&text))
        .map(|text| text.chars().take(1200).collect());
    asset.page_url = text_at(info, "descriptionurl");
    asset.thumbnail_url = text_at(info, "thumburl");
    asset.preview_url = text_at(info, "thumburl");
    asset.download_url = text_at(info, "url");
    asset.width = super::number_at(info, "width").map(|w| w.min(u32::MAX as u64) as u32);
    asset.height = super::number_at(info, "height").map(|h| h.min(u32::MAX as u64) as u32);
    asset.mime_type = Some(mime.clone());
    asset.creator = meta(info, "Artist")
        .map(|text| strip_html(&text))
        .filter(|text| !text.is_empty());
    asset.license_name = meta(info, "LicenseShortName").map(|text| strip_html(&text));
    asset.license_url = meta(info, "LicenseUrl");
    let attribution = meta(info, "Attribution")
        .or_else(|| meta(info, "Credit"))
        .map(|text| strip_html(&text))
        .filter(|text| !text.is_empty());
    asset.attribution = attribution;
    asset.source_name = Some("Wikimedia Commons".into());
    asset.tags = meta(info, "Categories")
        .map(|text| {
            text.split('|')
                .map(|tag| tag.trim().to_string())
                .filter(|tag| !tag.is_empty())
                .take(12)
                .collect()
        })
        .unwrap_or_default();
    asset.media_type = if mime == "image/svg+xml" {
        "illustration"
    } else if asset
        .tags
        .iter()
        .any(|tag| tag.to_ascii_lowercase().contains("map"))
    {
        "map"
    } else if asset
        .tags
        .iter()
        .any(|tag| tag.to_ascii_lowercase().contains("diagram"))
    {
        "diagram"
    } else {
        "photo"
    }
    .to_string();
    asset.download_allowed = asset
        .download_url
        .as_deref()
        .map(crate::util::is_allowed_web_url)
        .unwrap_or(false);
    asset.created_at = meta(info, "DateTimeOriginal")
        .or_else(|| meta(info, "DateTime"))
        .and_then(|date| iso_timestamp(&date));
    if let Some(user) = text_at(info, "user") {
        asset.provider_metadata.insert("uploader".into(), user);
    }
    if let Some(license) = meta(info, "License") {
        asset.provider_metadata.insert("license".into(), license);
    }
    if let Some(required) = meta(info, "AttributionRequired") {
        asset
            .provider_metadata
            .insert("attributionRequired".into(), required);
    }
    if let Some(usage) = meta(info, "UsageTerms") {
        asset
            .provider_metadata
            .insert("usageTerms".into(), strip_html(&usage));
    }
    Some(asset)
}

fn meta(info: &Value, key: &str) -> Option<String> {
    info.pointer(&format!("/extmetadata/{key}/value"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Value {
        serde_json::json!({
            "batchcomplete": true,
            "continue": {"gsroffset": 1, "continue": "gsroffset||"},
            "query": {
                "pages": [{
                    "pageid": 8719606,
                    "ns": 6,
                    "title": "File:The hidden fires of the Flame Nebula.jpg",
                    "imageinfo": [{
                        "size": 38536857,
                        "width": 12564,
                        "height": 15283,
                        "thumburl": "https://thumb.wikimedia.org/thumb.jpg",
                        "url": "https://upload.wikimedia.org/original.jpg",
                        "descriptionurl": "https://commons.wikimedia.org/wiki/File:The_hidden_fires_of_the_Flame_Nebula.jpg",
                        "mime": "image/jpeg",
                        "user": "Originalwana",
                        "extmetadata": {
                            "LicenseShortName": {"value": "CC BY 4.0"},
                            "LicenseUrl": {"value": "https://creativecommons.org/licenses/by/4.0"},
                            "License": {"value": "cc-by-4.0"},
                            "Artist": {"value": "<a href=\"x\">ESO/J. Emerson</a>"},
                            "AttributionRequired": {"value": "true"},
                            "ImageDescription": {"value": "A <b>spectacular</b> star-forming region."},
                            "Categories": {"value": "Flame Nebula|Astronomy|Featured pictures"},
                            "DateTimeOriginal": {"value": "2009-12-11"}
                        }
                    }]
                }]
            }
        })
    }

    #[test]
    fn parses_imageinfo_and_extmetadata() {
        let page = parse_page(&fixture(), 1).unwrap();
        assert!(page.has_more);
        let asset = &page.assets[0];
        assert_eq!(asset.id, "wikimedia:8719606");
        assert_eq!(
            asset.title.as_deref(),
            Some("The hidden fires of the Flame Nebula.jpg")
        );
        assert_eq!(asset.creator.as_deref(), Some("ESO/J. Emerson"));
        assert_eq!(asset.license_name.as_deref(), Some("CC BY 4.0"));
        assert_eq!(asset.width, Some(12564));
        assert_eq!(asset.tags.len(), 3);
        assert!(asset.download_allowed);
        assert_eq!(
            asset.attribution.as_deref(),
            None,
            "empty attribution is omitted"
        );
        assert_eq!(
            asset
                .provider_metadata
                .get("attributionRequired")
                .map(String::as_str),
            Some("true")
        );
        assert!(asset.description.as_ref().unwrap().contains("spectacular"));
    }

    #[test]
    fn non_image_files_are_skipped() {
        let mut value = fixture();
        value["query"]["pages"][0]["imageinfo"][0]["mime"] = serde_json::json!("application/pdf");
        let page = parse_page(&value, 1).unwrap();
        assert!(page.assets.is_empty());
    }

    #[test]
    fn missing_query_is_an_empty_page_not_a_fake_result() {
        let page = parse_page(&serde_json::json!({"batchcomplete": true}), 1).unwrap();
        assert!(page.assets.is_empty());
        assert!(!page.has_more);
    }
}
