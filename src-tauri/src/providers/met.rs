//! The Metropolitan Museum of Art adapter.
//!
//! The Met publishes a search index and one object record per artwork, so a page is one search
//! request followed by one small request per object. Requests are spaced by a short interval and
//! issued concurrently; a single failing object is skipped rather than failing the whole page.
//!
//! The v1 `/search` endpoint was retired in October 2026; this adapter uses `v1.1`.

use std::sync::Arc;

use async_trait::async_trait;
use futures_util::future::join_all;
use serde_json::Value;

use crate::error::{AppError, AppResult};
use crate::models::{Asset, ProviderInfo, ProviderPage, SearchRequest};
use crate::util::{extension_for, is_allowed_web_url, iso_timestamp};

use super::{
    bool_at, capabilities, info, policy, require_query, text_at, ImageProvider, ProviderContext,
};

pub struct MetProvider;

const SEARCH: &str = "https://collectionapi.metmuseum.org/public/collection/v1.1/search";
const OBJECTS: &str = "https://collectionapi.metmuseum.org/public/collection/v1/objects";

#[async_trait]
impl ImageProvider for MetProvider {
    fn id(&self) -> &'static str {
        "met"
    }

    fn name(&self) -> &'static str {
        "The Metropolitan Museum of Art"
    }

    fn info(&self, _has_api_key: bool) -> ProviderInfo {
        info(
            "met",
            "The Metropolitan Museum of Art",
            "The Met Collection API",
            "https://www.metmuseum.org",
            "CC0 for public-domain works; otherwise see the museum record",
            true,
            false,
            false,
            capabilities(false, false, false, false, false, &["relevance"], &["artwork"]),
            policy(
                "Public Collection API (collectionapi.metmuseum.org), no key required",
                Some("https://metmuseum.github.io/"),
                None,
                Some("https://www.metmuseum.org/about-the-met/policies-and-documents/open-access"),
                true,
                "no published request quota; Stockcraft spaces object requests by 120 ms",
                "Search returns object ids only, so each page also fetches one record per artwork. Only isPublicDomain = true works are offered for download.",
            ),
            "Encyclopaedic museum collection; open-access images carry a per-object public-domain flag.",
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
        let offset = page.saturating_sub(1) * page_size;
        let mut url = url::Url::parse(SEARCH).map_err(|e| AppError::internal(e.to_string()))?;
        {
            let mut pairs = url.query_pairs_mut();
            pairs.append_pair("q", query);
            pairs.append_pair("hasImages", "true");
            pairs.append_pair("offset", &offset.to_string());
            pairs.append_pair("limit", &page_size.to_string());
        }
        ctx.http.acquire("met").await?;
        let value = ctx.http.get_json(url.as_str(), None).await?;
        let total = value.get("total").and_then(Value::as_u64);
        let ids = value
            .get("objectIDs")
            .and_then(Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(Value::as_u64)
                    .take(page_size as usize)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if ids.is_empty() {
            return Ok(ProviderPage {
                assets: Vec::new(),
                page,
                has_more: false,
                total,
            });
        }

        let http = Arc::clone(&ctx.http);
        let fetches = ids.into_iter().map(|id| {
            let http = Arc::clone(&http);
            let url = format!("{OBJECTS}/{id}");
            async move {
                http.acquire("met").await?;
                http.get_json(&url, None).await
            }
        });
        let results = join_all(fetches).await;
        let mut assets = Vec::new();
        let mut first_error = None;
        for result in results {
            match result {
                Ok(value) => {
                    if let Some(asset) = parse_asset(&value) {
                        assets.push(asset);
                    }
                }
                Err(error) => {
                    if first_error.is_none() {
                        first_error = Some(error);
                    }
                }
            }
        }
        if assets.is_empty() {
            if let Some(error) = first_error {
                return Err(error);
            }
        }
        let has_more = total
            .map(|total| u64::from(offset) + u64::from(page_size) < total)
            .unwrap_or(false);
        Ok(ProviderPage {
            assets,
            page,
            has_more,
            total,
        })
    }

    async fn details(&self, ctx: &ProviderContext, provider_asset_id: &str) -> AppResult<Asset> {
        let id = provider_asset_id
            .strip_prefix("met:")
            .unwrap_or(provider_asset_id);
        if id.is_empty() || !id.chars().all(|ch| ch.is_ascii_digit()) {
            return Err(AppError::validation(
                "invalid Metropolitan Museum object id",
            ));
        }
        ctx.http.acquire("met").await?;
        let value = ctx.http.get_json(&format!("{OBJECTS}/{id}"), None).await?;
        parse_asset(&value)
            .ok_or_else(|| AppError::Malformed("The Met returned an incomplete object".into()))
    }
}

pub(crate) fn parse_asset(item: &Value) -> Option<Asset> {
    let id = item.get("objectID").and_then(Value::as_u64)?;
    let small = text_at(item, "primaryImageSmall");
    let large = text_at(item, "primaryImage");
    let preview = small.or_else(|| large.clone())?;
    let public_domain = bool_at(item, "isPublicDomain").unwrap_or(false);
    let mut asset = Asset::new("met", id.to_string());
    asset.title = text_at(item, "title");
    asset.description = text_at(item, "medium");
    asset.page_url = text_at(item, "objectURL").or_else(|| {
        Some(format!(
            "https://www.metmuseum.org/art/collection/search/{id}"
        ))
    });
    asset.thumbnail_url = Some(preview.clone());
    asset.preview_url = Some(preview.clone());
    asset.download_url = if public_domain {
        large.filter(|url| is_allowed_web_url(url))
    } else {
        None
    };
    let download_url = asset
        .download_url
        .clone()
        .unwrap_or_else(|| preview.clone());
    asset.mime_type = Some(
        match extension_for(None, &download_url).as_str() {
            "png" => "image/png",
            "webp" => "image/webp",
            _ => "image/jpeg",
        }
        .to_string(),
    );
    asset.creator = text_at(item, "artistDisplayName").filter(|name| !name.is_empty());
    asset.license_name = Some(if public_domain {
        "CC0 1.0 (public domain dedication)".into()
    } else {
        "See The Met record".into()
    });
    asset.license_url = Some(if public_domain {
        "https://creativecommons.org/publicdomain/zero/1.0/".into()
    } else {
        "https://www.metmuseum.org/about-the-met/policies-and-documents/open-access".into()
    });
    asset.attribution = Some(format!(
        "\"{}\" — The Metropolitan Museum of Art{}",
        asset.title.as_deref().unwrap_or("Artwork"),
        if public_domain { " (CC0)" } else { "" }
    ));
    asset.source_name = Some("The Metropolitan Museum of Art".into());
    asset.media_type = "artwork".into();
    asset.download_allowed = public_domain
        && asset
            .download_url
            .as_deref()
            .map(is_allowed_web_url)
            .unwrap_or(false);
    for key in [
        "department",
        "classification",
        "objectName",
        "artistNationality",
        "objectDate",
    ] {
        if let Some(value) = text_at(item, key) {
            asset.tags.push(value);
        }
    }
    asset.tags.truncate(10);
    asset.created_at = text_at(item, "metadataDate").and_then(|date| iso_timestamp(&date));
    asset
        .provider_metadata
        .insert("publicDomain".into(), public_domain.to_string());
    for (key, field) in [
        ("department", "department"),
        ("classification", "classification"),
        ("objectName", "objectName"),
        ("creditLine", "creditLine"),
    ] {
        if let Some(value) = text_at(item, field) {
            asset.provider_metadata.insert(key.into(), value);
        }
    }
    Some(asset)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Value {
        serde_json::json!({
            "objectID": 436532,
            "isPublicDomain": true,
            "primaryImage": "https://images.metmuseum.org/CRDImages/ep/original/DT1502.jpg",
            "primaryImageSmall": "https://images.metmuseum.org/CRDImages/ep/web-large/DT1502.jpg",
            "title": "Self-Portrait with a Straw Hat",
            "artistDisplayName": "Vincent van Gogh",
            "objectURL": "https://www.metmuseum.org/art/collection/search/436532",
            "objectDate": "1887",
            "medium": "Oil on canvas",
            "department": "European Paintings",
            "classification": "Paintings",
            "artistNationality": "Dutch",
            "objectName": "Painting",
            "creditLine": "Bequest of Miss Adelaide Milton de Groot",
            "metadataDate": "2024-01-30T00:00:00Z"
        })
    }

    #[test]
    fn parses_public_domain_object() {
        let asset = parse_asset(&fixture()).unwrap();
        assert_eq!(asset.id, "met:436532");
        assert_eq!(asset.creator.as_deref(), Some("Vincent van Gogh"));
        assert!(asset.download_allowed);
        assert!(asset
            .download_url
            .as_deref()
            .unwrap()
            .contains("/original/"));
        assert_eq!(
            asset.thumbnail_url.as_deref().unwrap(),
            "https://images.metmuseum.org/CRDImages/ep/web-large/DT1502.jpg"
        );
        assert!(asset.license_name.as_deref().unwrap().contains("CC0"));
        assert!(asset.tags.contains(&"Paintings".to_string()));
        assert!(asset.created_at.is_some());
    }

    #[test]
    fn restricted_object_is_preview_only() {
        let mut value = fixture();
        value["isPublicDomain"] = serde_json::json!(false);
        let asset = parse_asset(&value).unwrap();
        assert!(!asset.download_allowed);
        assert!(asset.download_url.is_none());
        assert_eq!(asset.preview_url, asset.thumbnail_url);
    }

    #[test]
    fn object_without_image_is_skipped() {
        let mut value = fixture();
        value.as_object_mut().unwrap().remove("primaryImageSmall");
        value.as_object_mut().unwrap().remove("primaryImage");
        assert!(parse_asset(&value).is_none());
    }
}
