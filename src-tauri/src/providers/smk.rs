//! SMK (Statens Museum for Kunst) adapter — Denmark's national gallery open API.
//!
//! The search API needs no key and labels each record with `public_domain` plus a `rights` URL.
//! Direct downloads are offered only for public-domain works, and the `image_native` endpoint
//! serves an ordinary JPEG rather than the archival TIFF.

use async_trait::async_trait;
use serde_json::Value;

use crate::error::{AppError, AppResult};
use crate::models::{Asset, ProviderInfo, ProviderPage, SearchRequest};
use crate::util::{extension_for, is_allowed_web_url};

use super::{capabilities, info, policy, require_query, text_at, ImageProvider, ProviderContext};

pub struct SmkProvider;

const API: &str = "https://api.smk.dk/api/v1/art";

#[async_trait]
impl ImageProvider for SmkProvider {
    fn id(&self) -> &'static str {
        "smk"
    }

    fn name(&self) -> &'static str {
        "SMK – Statens Museum for Kunst"
    }

    fn info(&self, _has_api_key: bool) -> ProviderInfo {
        info(
            "smk",
            "SMK – Statens Museum for Kunst",
            "SMK Open",
            "https://open.smk.dk",
            "Public domain (CC0) when flagged; otherwise see the SMK record",
            true,
            false,
            false,
            capabilities(false, false, false, false, false, &["relevance"], &["artwork"]),
            policy(
                "Open REST API (api.smk.dk), no key required",
                Some("https://api.smk.dk/api/v1/docs/"),
                None,
                Some("https://www.smk.dk/en/legal/"),
                true,
                "undocumented public limits; Stockcraft sends at most one request per second",
                "Only records with public_domain = true are offered for direct download. Others link to the SMK record.",
            ),
            "Danish national gallery with an open, key-free collection API.",
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
        let mut url = url::Url::parse(&format!("{API}/search/"))
            .map_err(|e| AppError::internal(e.to_string()))?;
        {
            let mut pairs = url.query_pairs_mut();
            pairs.append_pair("keys", query);
            pairs.append_pair("offset", &offset.to_string());
            pairs.append_pair("rows", &page_size.to_string());
        }
        ctx.http.acquire("smk").await?;
        let value = ctx.http.get_json(url.as_str(), None).await?;
        parse_page(&value, page, page_size)
    }

    async fn details(&self, ctx: &ProviderContext, provider_asset_id: &str) -> AppResult<Asset> {
        let object_number = provider_asset_id
            .strip_prefix("smk:")
            .unwrap_or(provider_asset_id)
            .trim();
        if object_number.is_empty()
            || !object_number
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_'))
        {
            return Err(AppError::validation("invalid SMK object number"));
        }
        let mut url = url::Url::parse(API).map_err(|e| AppError::internal(e.to_string()))?;
        url.query_pairs_mut()
            .append_pair("object_number", object_number);
        ctx.http.acquire("smk").await?;
        let value = ctx.http.get_json(url.as_str(), None).await?;
        value
            .get("items")
            .and_then(Value::as_array)
            .and_then(|items| items.first())
            .and_then(parse_asset)
            .ok_or_else(|| AppError::Malformed("SMK returned an incomplete artwork".into()))
    }
}

pub(crate) fn parse_page(value: &Value, page: u32, page_size: u32) -> AppResult<ProviderPage> {
    let items = value
        .get("items")
        .and_then(Value::as_array)
        .ok_or_else(|| AppError::Malformed("SMK response has no items array".into()))?;
    let total = value.get("found").and_then(Value::as_u64);
    let assets = items.iter().filter_map(parse_asset).collect::<Vec<_>>();
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

fn first_title(item: &Value) -> Option<String> {
    item.get("titles")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| text_at(entry, "title"))
        .next()
}

fn first_period(item: &Value) -> Option<String> {
    item.get("production_date")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| text_at(entry, "period"))
        .next()
}

fn string_list(item: &Value, key: &str, limit: usize) -> Vec<String> {
    item.get(key)
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .take(limit)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn parse_asset(item: &Value) -> Option<Asset> {
    let has_image = item
        .get("has_image")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    if !has_image {
        return None;
    }
    let thumbnail = text_at(item, "image_thumbnail")?;
    let object_number = text_at(item, "object_number").or_else(|| text_at(item, "id"))?;
    let public_domain = item
        .get("public_domain")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut asset = Asset::new("smk", object_number.clone());
    asset.title = first_title(item);
    asset.description = item
        .get("content_description")
        .and_then(Value::as_array)
        .and_then(|values| values.first())
        .and_then(Value::as_str)
        .map(|text| text.chars().take(1200).collect());
    asset.page_url = text_at(item, "frontend_url")
        .or_else(|| Some(format!("https://open.smk.dk/artwork/image/{object_number}")));
    asset.thumbnail_url = Some(thumbnail.clone());
    asset.preview_url = text_at(item, "image_iiif_id")
        .map(|iiif| format!("{iiif}/full/!1686,/0/default.jpg"))
        .or_else(|| Some(thumbnail.clone()));
    asset.download_url = if public_domain {
        text_at(item, "image_native")
    } else {
        None
    };
    asset.width =
        super::number_at(item, "image_width").map(|value| value.min(u32::MAX as u64) as u32);
    asset.height =
        super::number_at(item, "image_height").map(|value| value.min(u32::MAX as u64) as u32);
    asset.creator = item
        .get("artist")
        .and_then(Value::as_array)
        .and_then(|artists| artists.first())
        .and_then(Value::as_str)
        .map(str::to_string);
    let download_url = asset
        .download_url
        .clone()
        .unwrap_or_else(|| asset.preview_url.clone().unwrap_or_default());
    asset.mime_type = Some(
        match extension_for(None, &download_url).as_str() {
            "png" => "image/png",
            "tif" => "image/tiff",
            "webp" => "image/webp",
            _ => "image/jpeg",
        }
        .to_string(),
    );
    let rights = text_at(item, "rights");
    asset.license_name = Some(if public_domain {
        "Public domain (CC0)".into()
    } else {
        "See the SMK record".into()
    });
    asset.license_url = Some(if public_domain {
        rights.unwrap_or_else(|| "https://creativecommons.org/publicdomain/mark/1.0/".into())
    } else {
        "https://www.smk.dk/en/legal/".into()
    });
    asset.attribution = Some(format!(
        "\"{}\" — SMK, Statens Museum for Kunst{}",
        asset.title.as_deref().unwrap_or("Artwork"),
        if public_domain {
            " (public domain)"
        } else {
            ""
        }
    ));
    asset.source_name = Some("SMK – Statens Museum for Kunst".into());
    asset.media_type = "artwork".into();
    asset.download_allowed = public_domain
        && asset
            .download_url
            .as_deref()
            .map(is_allowed_web_url)
            .unwrap_or(false);
    asset.tags = string_list(item, "content_subject", 6);
    asset.tags.extend(string_list(item, "materials", 3));
    if let Some(period) = first_period(item) {
        asset.tags.push(period);
    }
    asset.tags.truncate(10);
    asset
        .provider_metadata
        .insert("publicDomain".into(), public_domain.to_string());
    if let Some(object_number) = text_at(item, "object_number") {
        asset
            .provider_metadata
            .insert("objectNumber".into(), object_number);
    }
    Some(asset)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Value {
        serde_json::json!({
            "offset": 0,
            "rows": 24,
            "found": 2,
            "items": [{
                "id": "1170012745_object",
                "object_number": "KMS445",
                "has_image": true,
                "public_domain": true,
                "rights": "https://creativecommons.org/publicdomain/mark/1.0/",
                "titles": [{"title": "Motiv fra Capri, kort efter solens opgang", "type": "museumstitel"}],
                "artist": ["Christen Købke"],
                "production_date": [{"period": "1841-1843"}],
                "content_subject": ["Capri"],
                "materials": ["Lærred"],
                "image_thumbnail": "https://iip-thumb.smk.dk/iiif/jp2/x.tif.jp2/full/!1024,/0/default.jpg",
                "image_iiif_id": "https://iip.smk.dk/iiif/jp2/x.tif.jp2",
                "image_native": "https://api.smk.dk/api/v1/download/abc/KMS445.jpg",
                "image_width": 6693,
                "image_height": 4451,
                "frontend_url": "https://open.smk.dk/artwork/image/KMS445"
            }, {
                "id": "2",
                "object_number": "KMS999",
                "has_image": true,
                "public_domain": false,
                "image_thumbnail": "https://iip-thumb.smk.dk/iiif/jp2/y.tif.jp2/full/!1024,/0/default.jpg"
            }]
        })
    }

    #[test]
    fn parses_public_domain_artwork() {
        let page = parse_page(&fixture(), 1, 24).unwrap();
        assert_eq!(page.total, Some(2));
        let asset = &page.assets[0];
        assert_eq!(asset.id, "smk:KMS445");
        assert_eq!(asset.width, Some(6693));
        assert!(asset.download_allowed);
        assert!(asset
            .download_url
            .as_deref()
            .unwrap()
            .ends_with("KMS445.jpg"));
        assert!(asset.preview_url.as_deref().unwrap().contains("!1686,"));
        assert!(asset
            .license_name
            .as_deref()
            .unwrap()
            .contains("Public domain"));
        assert!(asset.tags.contains(&"Capri".to_string()));
    }

    #[test]
    fn non_public_domain_is_not_downloadable() {
        let asset = &parse_page(&fixture(), 1, 24).unwrap().assets[1];
        assert!(!asset.download_allowed);
        assert!(asset.download_url.is_none());
    }

    #[test]
    fn record_without_image_is_skipped() {
        let mut value = fixture();
        value["items"][0]["has_image"] = serde_json::json!(false);
        assert_eq!(parse_page(&value, 1, 24).unwrap().assets.len(), 1);
    }
}
