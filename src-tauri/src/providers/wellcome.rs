//! Wellcome Collection adapter — a key-free catalogue of digitised artworks, photographs and
//! illustrations served through IIIF.
//!
//! The `images` endpoint already returns one record per picture (rather than per catalogue work), so
//! a page needs a single request. Each record carries its licence, credit line and an average
//! colour, and the IIIF image can be requested at any size for thumbnails, previews and downloads.

use async_trait::async_trait;
use serde_json::Value;

use crate::error::{AppError, AppResult};
use crate::models::{Asset, ProviderInfo, ProviderPage, SearchRequest};
use crate::util::is_allowed_web_url;

use super::{capabilities, info, policy, require_query, text_at, ImageProvider, ProviderContext};

pub struct WellcomeProvider;

const API: &str = "https://api.wellcomecollection.org/catalogue/v2/images";

#[async_trait]
impl ImageProvider for WellcomeProvider {
    fn id(&self) -> &'static str {
        "wellcome"
    }

    fn name(&self) -> &'static str {
        "Wellcome Collection"
    }

    fn info(&self, _has_api_key: bool) -> ProviderInfo {
        info(
            "wellcome",
            "Wellcome Collection",
            "Wellcome Collection catalogue API",
            "https://wellcomecollection.org",
            "Public domain, CC0 or CC BY; in-copyright images are preview-only",
            true,
            false,
            false,
            capabilities(false, false, false, false, false, &["relevance"], &["artwork"]),
            policy(
                "Public REST API (api.wellcomecollection.org), no key required",
                Some("https://developers.wellcomecollection.org/"),
                None,
                Some("https://wellcomecollection.org/pages/YB1ZaREAACQAu5qg"),
                true,
                "polite use requested; Stockcraft sends at most one request per second",
                "Only Public Domain Mark, CC0 and CC BY images are offered for download. In-copyright images link to the catalogue record.",
            ),
            "Digitised artworks, photographs and illustrations from the Wellcome Collection.",
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
            pairs.append_pair("query", query);
            pairs.append_pair("page", &page.to_string());
            pairs.append_pair("pageSize", &page_size.to_string());
        }
        ctx.http.acquire("wellcome").await?;
        let value = ctx.http.get_json(url.as_str(), None).await?;
        parse_page(&value, page, page_size)
    }

    async fn details(&self, ctx: &ProviderContext, provider_asset_id: &str) -> AppResult<Asset> {
        let id = provider_asset_id
            .strip_prefix("wellcome:")
            .unwrap_or(provider_asset_id);
        if id.is_empty()
            || !id
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
        {
            return Err(AppError::validation("invalid Wellcome image id"));
        }
        ctx.http.acquire("wellcome").await?;
        let value = ctx.http.get_json(&format!("{API}/{id}"), None).await?;
        parse_asset(&value)
            .ok_or_else(|| AppError::Malformed("Wellcome returned an incomplete image".into()))
    }
}

pub(crate) fn parse_page(value: &Value, page: u32, page_size: u32) -> AppResult<ProviderPage> {
    let results = value
        .get("results")
        .and_then(Value::as_array)
        .ok_or_else(|| AppError::Malformed("Wellcome response has no results array".into()))?;
    let total = value.get("totalResults").and_then(Value::as_u64);
    let total_pages = value.get("totalPages").and_then(Value::as_u64);
    let assets = results.iter().filter_map(parse_asset).collect::<Vec<_>>();
    let has_more = total_pages
        .map(|pages| u64::from(page) < pages)
        .unwrap_or_else(|| assets.len() >= page_size as usize);
    Ok(ProviderPage {
        assets,
        page,
        has_more,
        total,
    })
}

/// Turn a IIIF `…/info.json` URL into a real image request at `size` (e.g. `400,` or `full`).
pub(crate) fn iiif_url(info_json: &str, size: &str) -> Option<String> {
    let base = info_json.trim().trim_end_matches('/');
    let base = base.strip_suffix("/info.json")?;
    Some(format!("{base}/full/{size}/0/default.jpg"))
}

fn license_allows_download(id: &str) -> bool {
    matches!(
        id,
        "pdm" | "cc0" | "cc-by" | "cc-by-sa" | "cc-by-nc" | "cc-by-nc-sa"
    )
}

fn license_name(id: &str, fallback: Option<String>) -> String {
    if let Some(label) = fallback {
        return label;
    }
    match id {
        "pdm" => "Public Domain Mark".into(),
        "cc0" => "CC0 1.0".into(),
        "cc-by" => "CC BY 4.0".into(),
        "inc" => "In copyright".into(),
        _ => "See the Wellcome catalogue record".into(),
    }
}

pub(crate) fn parse_asset(item: &Value) -> Option<Asset> {
    let id = text_at(item, "id")?;
    let source_title = text_at(item, "source.title")
        .or_else(|| text_at(item, "source.id").map(|_| "Untitled image".to_string()))?;
    let thumbnail = item.get("thumbnail")?.clone();
    let info_json = text_at(&thumbnail, "url").or_else(|| {
        item.pointer("/locations/0/url")
            .and_then(Value::as_str)
            .map(str::to_string)
    })?;
    let thumbnail_url = iiif_url(&info_json, "400,")?;
    let preview_url = iiif_url(&info_json, "1600,")?;
    let download_url = iiif_url(&info_json, "full")?;

    let license = thumbnail.get("license");
    let license_id = license
        .and_then(|license| text_at(license, "id"))
        .unwrap_or_else(|| "unknown".into());
    let label = license.and_then(|license| text_at(license, "label"));
    let license_url = license
        .and_then(|license| text_at(license, "url"))
        .unwrap_or_else(|| "https://wellcomecollection.org/pages/YB1ZaREAACQAu5qg".into());
    let credit = text_at(&thumbnail, "credit").or_else(|| {
        item.pointer("/locations/0/credit")
            .and_then(Value::as_str)
            .map(str::to_string)
    });
    let downloadable = license_allows_download(&license_id);

    let mut asset = Asset::new("wellcome", id);
    asset.title = Some(source_title.chars().take(300).collect());
    asset.thumbnail_url = Some(thumbnail_url);
    asset.preview_url = Some(preview_url);
    asset.download_url = Some(download_url.clone());
    asset.creator = credit.clone();
    asset.mime_type = Some("image/jpeg".into());
    asset.license_name = Some(license_name(&license_id, label));
    asset.license_url = Some(license_url);
    asset.attribution = Some(format!(
        "\"{}\" — Wellcome Collection{} ({})",
        asset.title.as_deref().unwrap_or("Image"),
        credit
            .as_deref()
            .filter(|credit| !credit.eq_ignore_ascii_case("Wellcome Collection"))
            .map(|credit| format!(", {credit}"))
            .unwrap_or_default(),
        license_id.to_ascii_uppercase()
    ));
    asset.source_name = Some("Wellcome Collection".into());
    asset.media_type = "artwork".into();
    asset.download_allowed = downloadable && is_allowed_web_url(&download_url);
    asset.dominant_color = text_at(item, "averageColor").filter(|color| {
        color.starts_with('#')
            && color.len() == 7
            && color[1..].chars().all(|ch| ch.is_ascii_hexdigit())
    });
    asset.tags = vec!["Wellcome Collection".into(), license_id.clone()];
    asset.tags.retain(|tag| !tag.trim().is_empty());
    if let Some(work) = text_at(item, "source.id") {
        asset.page_url = Some(format!("https://wellcomecollection.org/works/{work}"));
        asset.provider_metadata.insert("workId".into(), work);
    }
    if let Some(credit) = credit {
        asset.provider_metadata.insert("credit".into(), credit);
    }
    asset
        .provider_metadata
        .insert("licenseId".into(), license_id);
    Some(asset)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Value {
        serde_json::json!({
            "totalResults": 2,
            "totalPages": 1,
            "results": [{
                "id": "vxs9cw4g",
                "source": {"id": "kjmxnynt", "title": "Anemone flowers. Watercolour.", "type": "Work"},
                "thumbnail": {
                    "url": "https://iiif.wellcomecollection.org/image/V0043453ER/info.json",
                    "credit": "Wellcome Collection",
                    "license": {"id": "pdm", "label": "Public Domain Mark", "url": "https://creativecommons.org/share-your-work/public-domain/pdm/"}
                },
                "averageColor": "#c2ac9c",
                "aspectRatio": 0.7975
            }, {
                "id": "mw4ah4kq",
                "source": {"id": "abc123", "title": "Portrait of a patient", "type": "Work"},
                "thumbnail": {
                    "url": "https://iiif.wellcomecollection.org/image/L0030730/info.json",
                    "credit": "Wellcome Collection",
                    "license": {"id": "inc", "label": "In copyright", "url": "http://rightsstatements.org/vocab/InC/1.0/"}
                }
            }]
        })
    }

    #[test]
    fn builds_iiif_urls_from_info_json() {
        assert_eq!(
            iiif_url(
                "https://iiif.wellcomecollection.org/image/V0043453ER/info.json",
                "400,"
            )
            .as_deref(),
            Some("https://iiif.wellcomecollection.org/image/V0043453ER/full/400,/0/default.jpg")
        );
        assert!(iiif_url("https://example.org/not-iiif", "400,").is_none());
    }

    #[test]
    fn parses_public_domain_image() {
        let page = parse_page(&fixture(), 1, 24).unwrap();
        assert_eq!(page.total, Some(2));
        let asset = &page.assets[0];
        assert_eq!(asset.id, "wellcome:vxs9cw4g");
        assert!(asset.download_allowed);
        assert!(asset
            .attribution
            .as_deref()
            .unwrap()
            .contains("Wellcome Collection"));
        assert_eq!(asset.dominant_color.as_deref(), Some("#c2ac9c"));
        assert_eq!(
            asset.page_url.as_deref(),
            Some("https://wellcomecollection.org/works/kjmxnynt")
        );
    }

    #[test]
    fn in_copyright_image_is_preview_only() {
        let asset = &parse_page(&fixture(), 1, 24).unwrap().assets[1];
        assert!(!asset.download_allowed);
        assert!(asset
            .license_name
            .as_deref()
            .unwrap()
            .contains("In copyright"));
    }

    #[test]
    fn record_without_thumbnail_is_skipped() {
        let mut value = fixture();
        value["results"][0]
            .as_object_mut()
            .unwrap()
            .remove("thumbnail");
        assert_eq!(parse_page(&value, 1, 24).unwrap().assets.len(), 1);
    }
}
