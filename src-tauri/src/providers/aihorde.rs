//! AI Horde adapter — a generation provider rather than a search provider.
//!
//! Every tenth search submits one anonymous generation job and streams the finished images back as
//! inline `data:` URLs. The provider is deliberately conservative: the documented anonymous key,
//! one job per minute, a small job (two 512 px images) and a bounded wait so a queued job can never
//! stall the rest of the search for long. Any failure is best-effort and simply returns no images.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde_json::Value;

use crate::error::{AppError, AppResult};
use crate::models::{Asset, ProviderCapabilities, ProviderInfo, ProviderPage, SearchRequest};
use crate::util::now_iso;

use super::{bool_at, info, policy, require_query, text_at, ImageProvider, ProviderContext};

pub struct AiHordeProvider;

/// Documented anonymous key: no account, low queue priority, no kudos.
const ANONYMOUS_KEY: &str = "0000000000";
const API: &str = "https://stablehorde.net/api/v2";
const GENERATE_EVERY: u64 = 10;
const GENERATION_INTERVAL: Duration = Duration::from_secs(60);
const POLL_INTERVAL: Duration = Duration::from_secs(2);
const MAX_WAIT: Duration = Duration::from_secs(45);
const IMAGE_SIZE: u32 = 512;
const MAX_IMAGES: usize = 2;
const MAX_REMEMBERED: usize = 64;

static SEARCH_COUNTER: AtomicU64 = AtomicU64::new(0);
static LAST_GENERATION: Mutex<Option<Instant>> = Mutex::new(None);

fn generated_assets() -> &'static Mutex<HashMap<String, Asset>> {
    static GENERATED: OnceLock<Mutex<HashMap<String, Asset>>> = OnceLock::new();
    GENERATED.get_or_init(|| Mutex::new(HashMap::new()))
}

/// True when the running search count reaches the generation cadence.
pub(crate) fn should_generate(search_number: u64) -> bool {
    search_number > 0 && search_number.is_multiple_of(GENERATE_EVERY)
}

fn empty_page(page: u32) -> ProviderPage {
    ProviderPage {
        assets: Vec::new(),
        page,
        has_more: false,
        total: None,
    }
}

fn remember(assets: &[Asset]) {
    let mut cache = generated_assets()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if cache.len() + assets.len() > MAX_REMEMBERED {
        cache.clear();
    }
    for asset in assets {
        cache.insert(asset.provider_asset_id.clone(), asset.clone());
    }
}

fn rate_limit_allows() -> bool {
    let mut last = LAST_GENERATION
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    match *last {
        Some(previous) if previous.elapsed() < GENERATION_INTERVAL => false,
        _ => {
            *last = Some(Instant::now());
            true
        }
    }
}

fn capabilities() -> ProviderCapabilities {
    ProviderCapabilities {
        search: true,
        pagination: false,
        download: true,
        orientation_filter: false,
        size_filter: false,
        media_type_filter: false,
        license_filter: false,
        color_filter: false,
        sort_options: vec!["relevance".into()],
        media_types: vec!["illustration".into()],
    }
}

#[async_trait]
impl ImageProvider for AiHordeProvider {
    fn id(&self) -> &'static str {
        "aihorde"
    }

    fn name(&self) -> &'static str {
        "AI Horde"
    }

    fn info(&self, _has_api_key: bool) -> ProviderInfo {
        info(
            "aihorde",
            "AI Horde",
            "AI Horde (stablehorde.net)",
            "https://aihorde.net",
            "AI-generated; check the model licence before reuse",
            true,
            false,
            false,
            capabilities(),
            policy(
                "Public REST API (stablehorde.net) with the documented anonymous key",
                Some("https://aihorde.net/api/"),
                None,
                Some("https://aihorde.net/"),
                true,
                "one anonymous generation per minute; Stockcraft only submits on every tenth search",
                "Generation, not search: images are produced on demand and returned inline. Jobs are small, censored and best-effort; a queue timeout yields no images rather than an error.",
            ),
            "Generates images for every tenth search using the shared AI Horde network.",
        )
    }

    async fn search(
        &self,
        ctx: &ProviderContext,
        request: &SearchRequest,
    ) -> AppResult<ProviderPage> {
        if request.page > 1 {
            return Ok(empty_page(request.page));
        }
        let search_number = SEARCH_COUNTER.fetch_add(1, Ordering::Relaxed) + 1;
        if !should_generate(search_number) {
            return Ok(empty_page(1));
        }
        let query = require_query(request)?;
        if !rate_limit_allows() {
            tracing::debug!("AI Horde skipped: the generation rate limit is still active");
            return Ok(empty_page(1));
        }
        match self.generate(ctx, query).await {
            Ok(page) => Ok(page),
            Err(error) => {
                tracing::warn!(%error, "AI Horde generation failed; returning no images");
                Ok(empty_page(1))
            }
        }
    }

    async fn details(&self, _ctx: &ProviderContext, provider_asset_id: &str) -> AppResult<Asset> {
        let key = provider_asset_id
            .strip_prefix("aihorde:")
            .unwrap_or(provider_asset_id);
        generated_assets()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(key)
            .cloned()
            .ok_or_else(|| {
                AppError::NotFound(
                    "generated images are kept for the current session only; run a new search"
                        .into(),
                )
            })
    }
}

impl AiHordeProvider {
    async fn generate(&self, ctx: &ProviderContext, query: &str) -> AppResult<ProviderPage> {
        let payload = serde_json::json!({
            "prompt": format!("{query}, high quality, detailed"),
            "params": {
                "width": IMAGE_SIZE,
                "height": IMAGE_SIZE,
                "steps": 14,
                "n": MAX_IMAGES,
                "sampler_name": "k_euler",
            },
            "nsfw": false,
            "censor_nsfw": true,
            "models": [],
            "r2": false,
        });
        ctx.http.acquire("aihorde").await?;
        let submitted = ctx
            .http
            .post_json_with_header(
                &format!("{API}/generate/async"),
                &payload,
                "apikey",
                ANONYMOUS_KEY,
            )
            .await?;
        let generation_id = text_at(&submitted, "id")
            .ok_or_else(|| AppError::Malformed("AI Horde did not return a job id".into()))?;

        let deadline = Instant::now() + MAX_WAIT;
        loop {
            if Instant::now() >= deadline {
                tracing::debug!(generation = %generation_id, "AI Horde generation timed out");
                return Ok(empty_page(1));
            }
            tokio::time::sleep(POLL_INTERVAL).await;
            let check = ctx
                .http
                .get_json_with_header(
                    &format!("{API}/generate/check/{generation_id}"),
                    "apikey",
                    ANONYMOUS_KEY,
                )
                .await?;
            if bool_at(&check, "faulted").unwrap_or(false) {
                tracing::debug!(generation = %generation_id, "AI Horde job faulted");
                return Ok(empty_page(1));
            }
            if bool_at(&check, "done").unwrap_or(false)
                || bool_at(&check, "finished").unwrap_or(false)
            {
                break;
            }
        }

        let status = ctx
            .http
            .get_json_with_header(
                &format!("{API}/generate/status/{generation_id}"),
                "apikey",
                ANONYMOUS_KEY,
            )
            .await?;
        let assets = parse_generation(&status, query, &generation_id);
        if assets.is_empty() {
            return Ok(empty_page(1));
        }
        remember(&assets);
        Ok(ProviderPage {
            total: Some(assets.len() as u64),
            assets,
            page: 1,
            has_more: false,
        })
    }
}

/// AI Horde returns raw base64; wrap it in a `data:` URL the webview can render directly.
pub(crate) fn inline_data_url(image: &str) -> Option<String> {
    let image = image.trim();
    if image.is_empty() {
        return None;
    }
    if image.starts_with("data:") {
        return Some(image.to_string());
    }
    let mime = if image.starts_with("UklGR") {
        "image/webp"
    } else if image.starts_with("iVBOR") {
        "image/png"
    } else if image.starts_with("/9j/") {
        "image/jpeg"
    } else {
        return None;
    };
    Some(format!("data:{mime};base64,{image}"))
}

pub(crate) fn parse_generation(value: &Value, query: &str, generation_id: &str) -> Vec<Asset> {
    let Some(generations) = value.get("generations").and_then(Value::as_array) else {
        return Vec::new();
    };
    let title: String = format!("AI Horde · {query}").chars().take(120).collect();
    let mut assets = Vec::new();
    for (index, generation) in generations.iter().take(MAX_IMAGES).enumerate() {
        if bool_at(generation, "censored").unwrap_or(false) {
            continue;
        }
        let Some(data_url) = text_at(generation, "img").and_then(|img| inline_data_url(&img))
        else {
            continue;
        };
        let model = text_at(generation, "model").unwrap_or_else(|| "unknown model".into());
        let worker = text_at(generation, "worker_name").unwrap_or_else(|| "unknown worker".into());
        let seed = text_at(generation, "seed");
        let mut asset = Asset::new("aihorde", format!("{generation_id}:{index}"));
        asset.title = Some(title.clone());
        asset.description = Some(format!(
            "Generated by {model} on {worker}{}.",
            seed.as_deref()
                .map(|seed| format!(" with seed {seed}"))
                .unwrap_or_default()
        ));
        let mime = data_url
            .split(';')
            .next()
            .and_then(|head| head.strip_prefix("data:"))
            .unwrap_or("image/png")
            .to_string();
        asset.thumbnail_url = Some(data_url.clone());
        asset.preview_url = Some(data_url.clone());
        asset.download_url = Some(data_url);
        asset.width = Some(IMAGE_SIZE);
        asset.height = Some(IMAGE_SIZE);
        asset.mime_type = Some(mime);
        asset.creator = Some("AI Horde community".into());
        asset.license_name = Some("AI-generated — check the model licence".into());
        asset.license_url = Some("https://aihorde.net/".into());
        asset.attribution = Some(format!(
            "AI-generated image via AI Horde (model: {model}; worker: {worker}). Verify the model licence before reuse."
        ));
        asset.source_name = Some("AI Horde".into());
        asset.media_type = "illustration".into();
        asset.download_allowed = true;
        asset.created_at = Some(now_iso());
        asset.tags = vec!["ai-generated".into(), model.clone(), "anonymous".into()];
        asset.tags.retain(|tag| !tag.trim().is_empty());
        asset.provider_metadata.insert("model".into(), model);
        asset.provider_metadata.insert("worker".into(), worker);
        asset
            .provider_metadata
            .insert("generationId".into(), generation_id.to_string());
        if let Some(seed) = seed {
            asset.provider_metadata.insert("seed".into(), seed);
        }
        assets.push(asset);
    }
    assets
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 1×1 transparent WebP, base64 encoded.
    const WEBP: &str = "UklGRiIAAABXRUJQVlA4IBYAAAAwAQCdASoBAAEADsD+JaQAA3AAAAAA";

    fn fixture() -> Value {
        serde_json::json!({
            "generations": [
                {
                    "img": WEBP,
                    "seed": "123456",
                    "model": "stable_diffusion",
                    "worker_name": "worker-one",
                    "censored": false
                },
                {
                    "img": "not-an-image",
                    "model": "stable_diffusion",
                    "worker_name": "worker-two",
                    "censored": false
                },
                {
                    "img": WEBP,
                    "model": "stable_diffusion",
                    "worker_name": "worker-three",
                    "censored": true
                }
            ]
        })
    }

    #[test]
    fn generates_on_every_tenth_search() {
        assert!(!should_generate(0));
        assert!(!should_generate(1));
        assert!(!should_generate(9));
        assert!(should_generate(10));
        assert!(!should_generate(19));
        assert!(should_generate(20));
    }

    #[test]
    fn parses_inline_webp_and_skips_invalid_or_censored() {
        let assets = parse_generation(&fixture(), "sunrise", "abc");
        assert_eq!(assets.len(), 1);
        let asset = &assets[0];
        assert_eq!(asset.id, "aihorde:abc:0");
        assert_eq!(asset.provider_asset_id, "abc:0");
        assert_eq!(asset.mime_type.as_deref(), Some("image/webp"));
        assert!(asset
            .download_url
            .as_deref()
            .unwrap()
            .starts_with("data:image/webp;base64,"));
        assert_eq!(asset.media_type, "illustration");
        assert!(asset.download_allowed);
        assert!(asset.title.as_deref().unwrap().contains("sunrise"));
        assert_eq!(
            asset.provider_metadata.get("model").map(String::as_str),
            Some("stable_diffusion")
        );
        assert_eq!(
            asset.provider_metadata.get("seed").map(String::as_str),
            Some("123456")
        );
    }

    #[test]
    fn detects_common_image_formats() {
        assert!(inline_data_url("iVBORw0KGgo=")
            .unwrap()
            .starts_with("data:image/png;"));
        assert!(inline_data_url("/9j/4AAQ")
            .unwrap()
            .starts_with("data:image/jpeg;"));
        assert!(inline_data_url(WEBP)
            .unwrap()
            .starts_with("data:image/webp;"));
        assert!(inline_data_url("  ").is_none());
        assert!(inline_data_url("bm90IGFuIGltYWdl").is_none());
    }

    #[test]
    fn generation_response_without_generations_is_empty() {
        assert!(parse_generation(&serde_json::json!({"message": "no"}), "x", "id").is_empty());
    }
}
