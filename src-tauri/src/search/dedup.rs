//! Cross-provider duplicate detection.
//!
//! A single image can appear under several provider ids and with tracking parameters in its URLs.
//! A result is a duplicate when its provider/asset-id pair, its canonical source page or its
//! canonical download image URL has already been seen in the same search.

use std::collections::HashSet;

use crate::models::Asset;
use crate::util::canonical_url;

#[derive(Default)]
pub struct Deduper {
    seen: HashSet<String>,
}

impl Deduper {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns `true` when the asset is fresh, `false` when a duplicate.
    pub fn insert(&mut self, asset: &Asset) -> bool {
        let mut keys = vec![format!("id:{}:{}", asset.provider, asset.provider_asset_id)];
        if let Some(page) = asset
            .page_url
            .as_deref()
            .filter(|url| !url.trim().is_empty())
        {
            keys.push(format!("page:{}", canonical_url(page)));
        }
        if let Some(image) = asset
            .download_url
            .as_deref()
            .filter(|url| !url.trim().is_empty())
        {
            keys.push(format!("image:{}", canonical_url(image)));
        }
        if keys.iter().any(|key| self.seen.contains(key)) {
            return false;
        }
        self.seen.extend(keys);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(provider: &str, id: &str, page: &str, image: &str) -> Asset {
        let mut asset = Asset::new(provider, id);
        asset.page_url = Some(page.into());
        asset.download_url = Some(image.into());
        asset
    }

    #[test]
    fn rejects_identical_provider_ids() {
        let mut deduper = Deduper::new();
        assert!(deduper.insert(&asset("a", "1", "https://a/1", "https://a/1.jpg")));
        assert!(!deduper.insert(&asset("a", "1", "https://a/1", "https://a/1.jpg")));
        assert!(deduper.insert(&asset("b", "1", "https://b/1", "https://b/1.jpg")));
    }

    #[test]
    fn rejects_cross_provider_duplicates_by_canonical_page_or_image() {
        let mut deduper = Deduper::new();
        assert!(deduper.insert(&asset(
            "a",
            "1",
            "https://example.org/photo?utm_source=x&id=1",
            "https://cdn.example.org/photo.jpg?utm_campaign=y"
        )));
        assert!(!deduper.insert(&asset(
            "b",
            "2",
            "https://example.org/photo?id=1",
            "https://cdn.example.org/other.jpg"
        )));
        assert!(!deduper.insert(&asset(
            "c",
            "3",
            "https://example.org/other",
            "https://cdn.example.org/photo.jpg"
        )));
    }

    #[test]
    fn accepts_assets_without_urls_when_ids_differ() {
        let mut deduper = Deduper::new();
        let mut first = Asset::new("a", "1");
        first.page_url = None;
        first.download_url = None;
        let mut second = Asset::new("b", "2");
        second.page_url = None;
        second.download_url = None;
        assert!(deduper.insert(&first));
        assert!(deduper.insert(&second));
    }
}
