//! Cleveland Museum of Art adapter — open-access API, CC0 images only for direct download.

use async_trait::async_trait;
use serde_json::Value;

use crate::error::{AppError, AppResult};
use crate::models::{Asset, ProviderInfo, ProviderPage, SearchRequest};
use crate::util::{extension_for, iso_timestamp, strip_html};

use super::{capabilities, info, policy, require_query, text_at, ImageProvider, ProviderContext};

pub struct ClevelandProvider;

const API: &str = "https://openaccess-api.clevelandart.org/api/artworks";

#[async_trait]
impl ImageProvider for ClevelandProvider {
    fn id(&self) -> &'static str {
        "cleveland"
    }

    fn name(&self) -> &'static str {
        "Cleveland Museum of Art"
    }

    fn info(&self, _has_api_key: bool) -> ProviderInfo {
        info(
            "cleveland",
            "Cleveland Museum of Art",
            "Cleveland Museum of Art Open Access",
            "https://www.clevelandart.org",
            "CC0 for open-access works; otherwise copyrighted",
            true,
            false,
            false,
            capabilities(false, false, false, false, false, &["relevance"], &["artwork"]),
            policy(
                "Open Access API (openaccess-api.clevelandart.org)",
                Some("https://openaccess-api.clevelandart.org/"),
                None,
                Some("https://www.clevelandart.org/open-access"),
                true,
                "undocumented public limits; Stockcraft sends at most two requests per second",
                "Only records whose share_license_status is CC0 are offered for direct download. Others link to the museum record.",
            ),
            "Open-access museum collection with a per-object licence flag.",
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
        let skip = (page.saturating_sub(1)) * page_size;
        let mut url =
            url::Url::parse(&format!("{API}/")).map_err(|e| AppError::internal(e.to_string()))?;
        {
            let mut pairs = url.query_pairs_mut();
            pairs.append_pair("q", query);
            pairs.append_pair("has_image", "1");
            pairs.append_pair("skip", &skip.to_string());
            pairs.append_pair("limit", &page_size.to_string());
        }
        ctx.http.acquire("cleveland").await?;
        let value = ctx.http.get_json(url.as_str(), None).await?;
        parse_page(&value, page, page_size)
    }

    async fn details(&self, ctx: &ProviderContext, provider_asset_id: &str) -> AppResult<Asset> {
        let id = provider_asset_id
            .rsplit(':')
            .next()
            .unwrap_or(provider_asset_id);
        if id.is_empty() || !id.chars().all(|ch| ch.is_ascii_digit()) {
            return Err(AppError::validation("invalid Cleveland artwork id"));
        }
        ctx.http.acquire("cleveland").await?;
        let value = ctx.http.get_json(&format!("{API}/{id}"), None).await?;
        value
            .get("data")
            .and_then(parse_asset)
            .ok_or_else(|| AppError::Malformed("Cleveland returned an incomplete artwork".into()))
    }
}

pub(crate) fn parse_page(value: &Value, page: u32, page_size: u32) -> AppResult<ProviderPage> {
    let data = value
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| AppError::Malformed("Cleveland response has no data array".into()))?;
    let total = value.pointer("/info/total").and_then(Value::as_u64);
    let mut assets = Vec::new();
    for item in data {
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
    let id = item.get("id").and_then(Value::as_u64)?;
    let web = text_at(item, "images.web.url")?;
    let print = text_at(item, "images.print.url");
    let license_status =
        text_at(item, "share_license_status").unwrap_or_else(|| "Copyrighted".into());
    let shared = license_status.eq_ignore_ascii_case("cc0");
    let mut asset = Asset::new("cleveland", id.to_string());
    asset.title = text_at(item, "title");
    asset.description = text_at(item, "digital_description")
        .or_else(|| text_at(item, "description"))
        .map(|text| {
            let text = strip_html(&text);
            text.chars().take(1200).collect()
        });
    asset.page_url = text_at(item, "url").or_else(|| {
        text_at(item, "accession_number")
            .map(|accession| format!("https://www.clevelandart.org/art/{accession}"))
    });
    asset.thumbnail_url = Some(web.clone());
    asset.preview_url = Some(web.clone());
    asset.download_url = if shared {
        print.or_else(|| text_at(item, "images.full.url"))
    } else {
        None
    };
    asset.width = super::number_at(item, "images.print.width")
        .or_else(|| super::number_at(item, "images.web.width"))
        .map(|value| value.min(u32::MAX as u64) as u32);
    asset.height = super::number_at(item, "images.print.height")
        .or_else(|| super::number_at(item, "images.web.height"))
        .map(|value| value.min(u32::MAX as u64) as u32);
    let download_url = asset.download_url.clone().unwrap_or_else(|| web.clone());
    asset.mime_type = Some(
        match extension_for(None, &download_url).as_str() {
            "png" => "image/png",
            "tif" => "image/tiff",
            _ => "image/jpeg",
        }
        .to_string(),
    );
    asset.creator = item
        .pointer("/creators/0/description")
        .and_then(Value::as_str)
        .map(strip_html)
        .filter(|text| !text.is_empty());
    asset.license_name = Some(if shared {
        "CC0 1.0 (public domain dedication)".into()
    } else {
        format!("{license_status} — see museum record")
    });
    asset.license_url = Some(if shared {
        "https://creativecommons.org/publicdomain/zero/1.0/".into()
    } else {
        "https://www.clevelandart.org/open-access".into()
    });
    asset.attribution = Some(format!(
        "\"{}\" — Cleveland Museum of Art{}",
        asset.title.as_deref().unwrap_or("Artwork"),
        if shared { " (CC0)" } else { "" }
    ));
    asset.source_name = Some("Cleveland Museum of Art".into());
    asset.tags = {
        let mut tags = Vec::new();
        for key in ["type", "technique", "department"] {
            if let Some(tag) = text_at(item, key) {
                tags.push(tag);
            }
        }
        if let Some(culture) = item.get("culture").and_then(Value::as_array) {
            tags.extend(culture.iter().filter_map(Value::as_str).map(str::to_string));
        }
        tags.retain(|tag| !tag.trim().is_empty());
        tags.truncate(12);
        tags
    };
    asset.media_type = "artwork".into();
    asset.download_allowed = shared
        && asset
            .download_url
            .as_deref()
            .map(crate::util::is_allowed_web_url)
            .unwrap_or(false);
    asset.created_at = text_at(item, "updated_at").and_then(|date| iso_timestamp(&date));
    if let Some(accession) = text_at(item, "accession_number") {
        asset
            .provider_metadata
            .insert("accessionNumber".into(), accession);
    }
    if let Some(art_type) = text_at(item, "type") {
        asset.provider_metadata.insert("type".into(), art_type);
    }
    asset
        .provider_metadata
        .insert("licenseStatus".into(), license_status);
    if let Some(credit) = text_at(item, "creditline") {
        asset
            .provider_metadata
            .insert("creditline".into(), strip_html(&credit));
    }
    Some(asset)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Value {
        serde_json::json!({
            "info": {"total": 18},
            "data": [{
                "id": 135382,
                "accession_number": "1958.39",
                "share_license_status": "CC0",
                "title": "The Red Kerchief",
                "creation_date": "c. 1868–73",
                "technique": "oil on fabric",
                "department": "Modern European Painting and Sculpture",
                "type": "Painting",
                "culture": ["France, 19th century"],
                "url": "https://clevelandart.org/art/1958.39",
                "creators": [{"description": "Claude Monet (French, 1840–1926)"}],
                "creditline": "Bequest of Leonard C. Hanna Jr.",
                "updated_at": "2023-01-05T10:00:00",
                "images": {
                    "web": {"url": "https://openaccess-cdn.clevelandart.org/1958.39/1958.39_web.jpg", "width": "723", "height": "900"},
                    "print": {"url": "https://openaccess-cdn.clevelandart.org/1958.39/1958.39_print.jpg", "width": "2732", "height": "3400"},
                    "full": {"url": "https://openaccess-cdn.clevelandart.org/1958.39/1958.39_full.tif", "width": "11652", "height": "14500"}
                }
            }]
        })
    }

    #[test]
    fn parses_cc0_artwork() {
        let page = parse_page(&fixture(), 1, 24).unwrap();
        assert_eq!(page.total, Some(18));
        let asset = &page.assets[0];
        assert_eq!(asset.id, "cleveland:135382");
        assert_eq!(asset.title.as_deref(), Some("The Red Kerchief"));
        assert_eq!(asset.width, Some(2732));
        assert_eq!(
            asset.creator.as_deref(),
            Some("Claude Monet (French, 1840–1926)")
        );
        assert!(asset.download_allowed);
        assert!(asset
            .download_url
            .as_deref()
            .unwrap()
            .contains("_print.jpg"));
        assert!(asset.license_name.as_deref().unwrap().contains("CC0"));
        assert_eq!(
            asset
                .provider_metadata
                .get("licenseStatus")
                .map(String::as_str),
            Some("CC0")
        );
        assert!(asset.tags.contains(&"Painting".to_string()));
    }

    #[test]
    fn copyrighted_record_is_not_downloadable() {
        let mut value = fixture();
        value["data"][0]["share_license_status"] = serde_json::json!("Copyrighted");
        let asset = parse_page(&value, 1, 24).unwrap().assets.remove(0);
        assert!(!asset.download_allowed);
        assert!(asset.download_url.is_none());
        assert!(asset.license_name.unwrap().contains("Copyrighted"));
    }

    #[test]
    fn record_without_image_is_skipped() {
        let mut value = fixture();
        value["data"][0]["images"]
            .as_object_mut()
            .unwrap()
            .remove("web");
        assert!(parse_page(&value, 1, 24).unwrap().assets.is_empty());
    }
}
