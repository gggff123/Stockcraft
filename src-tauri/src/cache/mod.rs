//! Disk thumbnail cache and the safe image proxy behind the `thumb://` URI scheme.
//!
//! Remote image bytes never reach the webview directly in desktop mode: the frontend rewrites image
//! URLs to `${base}/fetch?u=<remote>` and this module validates the host, fetches with a size cap,
//! verifies the declared content type and the file signature, then caches the bytes with LRU
//! eviction. When thumbnail caching is disabled the proxy still works, but nothing is written to
//! disk (and `get_thumbnail_base` returns an empty string, so the UI uses provider URLs directly).

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use tokio::sync::Semaphore;

use crate::error::{AppError, AppResult};
use crate::models::AppSettings;
use crate::util::validate_remote_url;

const MAX_IMAGE_BYTES: usize = 12 * 1024 * 1024;

pub struct ThumbnailCache {
    dir: PathBuf,
    client: reqwest::Client,
    semaphore: Semaphore,
}

impl ThumbnailCache {
    pub fn new(dir: &Path) -> AppResult<Self> {
        std::fs::create_dir_all(dir)?;
        let client = reqwest::Client::builder()
            .user_agent(crate::providers::http::USER_AGENT)
            .connect_timeout(Duration::from_secs(8))
            .timeout(Duration::from_secs(20))
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                if attempt.previous().len() >= 5 {
                    return attempt.stop();
                }
                match validate_remote_url(attempt.url().as_str()) {
                    Ok(_) => attempt.follow(),
                    Err(_) => attempt.stop(),
                }
            }))
            .build()
            .map_err(|error| {
                AppError::internal(format!("could not build thumbnail client: {error}"))
            })?;
        Ok(Self {
            dir: dir.to_path_buf(),
            client,
            semaphore: Semaphore::new(8),
        })
    }

    pub fn base_url() -> &'static str {
        if cfg!(target_os = "windows") {
            "http://thumb.localhost"
        } else {
            "thumb://localhost"
        }
    }

    /// Build the proxy URL the frontend uses in `tile.ts`.
    pub fn proxy_url(&self, remote: &str) -> String {
        let encoded: String = url::form_urlencoded::byte_serialize(remote.as_bytes()).collect();
        format!("{}/fetch?u={}", Self::base_url(), encoded)
    }

    /// Fetch (or read from disk) image bytes for a `/fetch?u=` request path.
    pub async fn fetch_proxied(
        &self,
        path: &str,
        query: Option<&str>,
        settings: &AppSettings,
    ) -> AppResult<(Vec<u8>, String)> {
        if !path.ends_with("/fetch") && path != "/fetch" {
            return Err(AppError::NotFound("unknown thumbnail route".into()));
        }
        let query = query.ok_or_else(|| AppError::validation("missing image URL"))?;
        let remote = url::form_urlencoded::parse(query.as_bytes())
            .find(|(key, _)| key == "u")
            .map(|(_, value)| value.into_owned())
            .ok_or_else(|| AppError::validation("missing image URL"))?;
        let remote = validate_remote_url(&remote)?.to_string();
        self.fetch(&remote, settings).await
    }

    async fn fetch(&self, remote: &str, settings: &AppSettings) -> AppResult<(Vec<u8>, String)> {
        let _permit = self
            .semaphore
            .acquire()
            .await
            .map_err(|_| AppError::internal("thumbnail worker pool closed"))?;

        if settings.cache_thumbnails {
            if let Some(hit) = self.read_cached(remote) {
                return Ok(hit);
            }
        }

        let response = self.client.get(remote).send().await.map_err(|error| {
            if error.is_timeout() {
                AppError::Timeout("thumbnail request timed out".into())
            } else {
                AppError::Network(format!("thumbnail fetch failed: {error}"))
            }
        })?;
        if !response.status().is_success() {
            return Err(AppError::Unavailable(format!(
                "thumbnail server returned HTTP {}",
                response.status()
            )));
        }
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(|value| {
                value
                    .split(';')
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_ascii_lowercase()
            })
            .unwrap_or_default();
        if !content_type.starts_with("image/") {
            return Err(AppError::Malformed(
                "thumbnail response is not an image".into(),
            ));
        }
        if let Some(length) = response.content_length() {
            if length as usize > MAX_IMAGE_BYTES {
                return Err(AppError::Malformed(
                    "thumbnail is larger than the 12 MB limit".into(),
                ));
            }
        }
        let mut stream = response.bytes_stream();
        let mut bytes = Vec::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk
                .map_err(|error| AppError::Network(format!("thumbnail stream failed: {error}")))?;
            if bytes.len() + chunk.len() > MAX_IMAGE_BYTES {
                return Err(AppError::Malformed(
                    "thumbnail is larger than the 12 MB limit".into(),
                ));
            }
            bytes.extend_from_slice(&chunk);
        }
        if !looks_like_image(&bytes, &content_type) {
            return Err(AppError::Malformed(
                "thumbnail bytes do not match an image format".into(),
            ));
        }

        if settings.cache_thumbnails {
            if let Err(error) =
                self.store(remote, &bytes, &content_type, settings.thumbnail_cache_mb)
            {
                tracing::warn!(%error, "thumbnail cache write failed");
            }
        }
        Ok((bytes, content_type))
    }

    fn paths(&self, remote: &str) -> (PathBuf, PathBuf) {
        let digest = Sha256::digest(remote.as_bytes());
        let key = hex::encode(digest);
        (
            self.dir.join(format!("{key}.img")),
            self.dir.join(format!("{key}.type")),
        )
    }

    fn read_cached(&self, remote: &str) -> Option<(Vec<u8>, String)> {
        let (image, kind) = self.paths(remote);
        let content_type = std::fs::read_to_string(&kind).ok()?.trim().to_string();
        if !content_type.starts_with("image/") {
            return None;
        }
        let bytes = std::fs::read(&image).ok()?;
        if bytes.is_empty() || !looks_like_image(&bytes, &content_type) {
            let _ = std::fs::remove_file(&image);
            let _ = std::fs::remove_file(&kind);
            return None;
        }
        // Touch for LRU. Failure is non-fatal.
        if let Ok(file) = std::fs::OpenOptions::new().write(true).open(&image) {
            let _ = file.set_modified(SystemTime::now());
        }
        Some((bytes, content_type))
    }

    fn store(&self, remote: &str, bytes: &[u8], content_type: &str, max_mb: u32) -> AppResult<()> {
        let (image, kind) = self.paths(remote);
        let temp = self.dir.join(format!(".{}.tmp", uuid::Uuid::new_v4()));
        std::fs::write(&temp, bytes)?;
        std::fs::rename(&temp, &image)?;
        std::fs::write(&kind, content_type)?;
        self.evict(max_mb)?;
        Ok(())
    }

    /// Delete the least-recently-used files until the cache is back under 90% of its limit.
    pub fn evict(&self, max_mb: u32) -> AppResult<u64> {
        let limit = u64::from(max_mb) * 1024 * 1024;
        let mut files: Vec<(SystemTime, u64, PathBuf)> = Vec::new();
        let mut total = 0u64;
        for entry in std::fs::read_dir(&self.dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("img") {
                continue;
            }
            let metadata = entry.metadata()?;
            total += metadata.len();
            files.push((
                metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                metadata.len(),
                path,
            ));
        }
        if total <= limit {
            return Ok(0);
        }
        files.sort_by_key(|(modified, _, _)| *modified);
        let target = limit.saturating_sub(limit / 10);
        let mut freed = 0u64;
        for (_, size, path) in files {
            if total <= target {
                break;
            }
            if std::fs::remove_file(&path).is_ok() {
                total = total.saturating_sub(size);
                freed += size;
                let kind = path.with_extension("type");
                let _ = std::fs::remove_file(kind);
            }
        }
        Ok(freed)
    }

    pub fn stats(&self) -> (u64, u64) {
        let mut bytes = 0u64;
        let mut files = 0u64;
        if let Ok(entries) = std::fs::read_dir(&self.dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|ext| ext.to_str()) != Some("img") {
                    continue;
                }
                if let Ok(metadata) = entry.metadata() {
                    bytes += metadata.len();
                    files += 1;
                }
            }
        }
        (bytes, files)
    }

    pub fn clear(&self) -> u64 {
        let mut freed = 0u64;
        if let Ok(entries) = std::fs::read_dir(&self.dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if matches!(
                    path.extension().and_then(|ext| ext.to_str()),
                    Some("img") | Some("type")
                ) {
                    let size = entry.metadata().map(|metadata| metadata.len()).unwrap_or(0);
                    if std::fs::remove_file(&path).is_ok() {
                        freed += size;
                    }
                }
            }
        }
        freed
    }
}

fn looks_like_image(bytes: &[u8], content_type: &str) -> bool {
    if bytes.len() < 12 {
        return false;
    }
    let signature_ok = bytes.starts_with(&[0xff, 0xd8, 0xff])
        || bytes.starts_with(&[0x89, b'P', b'N', b'G'])
        || bytes.starts_with(b"GIF87a")
        || bytes.starts_with(b"GIF89a")
        || (bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP")
        || bytes.starts_with(b"BM")
        || bytes.starts_with(&[0x49, 0x49, 0x2a, 0x00])
        || bytes.starts_with(&[0x4d, 0x4d, 0x00, 0x2a]);
    let svg_ok = content_type == "image/svg+xml"
        && String::from_utf8_lossy(&bytes[..bytes.len().min(256)])
            .to_ascii_lowercase()
            .contains("<svg");
    let avif_ok = content_type == "image/avif" && bytes.len() > 12 && &bytes[4..8] == b"ftyp";
    signature_ok || svg_ok || avif_ok
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cache() -> (tempfile::TempDir, ThumbnailCache) {
        let dir = tempfile::tempdir().unwrap();
        let cache = ThumbnailCache::new(dir.path()).unwrap();
        (dir, cache)
    }

    const JPEG: &[u8] = &[0xff, 0xd8, 0xff, 0xe0, 0, 0, 0, 0, 0, 0, 0, 0];

    #[test]
    fn proxy_url_encodes_the_remote_url() {
        assert_eq!(
            ThumbnailCache::base_url().starts_with("http"),
            cfg!(target_os = "windows")
        );
        let (_, cache) = cache();
        let url = cache.proxy_url("https://example.org/a b.jpg?x=1&y=2");
        assert!(url.starts_with(ThumbnailCache::base_url()));
        assert!(url.contains("%3A%2F%2F"));
        assert!(url.contains("%26"));
    }

    #[test]
    fn signature_check_rejects_html_disguised_as_an_image() {
        assert!(looks_like_image(JPEG, "image/jpeg"));
        assert!(!looks_like_image(
            b"<!doctype html><html></html>",
            "image/jpeg"
        ));
        assert!(looks_like_image(
            br#"<?xml version="1.0"?><svg xmlns="http://www.w3.org/2000/svg"></svg>"#,
            "image/svg+xml"
        ));
        assert!(!looks_like_image(
            br#"<?xml version="1.0"?><svg></svg>"#,
            "image/png"
        ));
    }

    #[test]
    fn stats_clear_and_eviction_work_without_the_network() {
        let (dir, cache) = cache();
        let (image, kind) = cache.paths("https://example.org/a.jpg");
        std::fs::write(&image, JPEG).unwrap();
        std::fs::write(&kind, "image/jpeg").unwrap();
        let (bytes, files) = cache.stats();
        assert_eq!(files, 1);
        assert_eq!(bytes, JPEG.len() as u64);
        let hit = cache.read_cached("https://example.org/a.jpg");
        assert!(hit.is_some());
        assert_eq!(hit.unwrap().1, "image/jpeg");

        let (other, other_kind) = cache.paths("https://example.org/b.jpg");
        std::fs::write(&other, JPEG).unwrap();
        std::fs::write(&other_kind, "image/jpeg").unwrap();
        let expected_freed = (JPEG.len() * 2 + "image/jpeg".len() * 2) as u64;
        assert_eq!(cache.clear(), expected_freed);
        assert_eq!(cache.stats(), (0, 0));
        assert!(std::fs::read_dir(dir.path()).unwrap().next().is_none());
    }

    #[test]
    fn eviction_removes_oldest_files_when_over_the_limit() {
        let (dir, cache) = cache();
        // 12 bytes each; force a 0 MB limit so anything must go.
        for name in ["a", "b", "c"] {
            let (image, kind) = cache.paths(name);
            std::fs::write(&image, JPEG).unwrap();
            std::fs::write(&kind, "image/jpeg").unwrap();
        }
        let freed = cache.evict(0).unwrap();
        assert!(freed > 0);
        assert_eq!(cache.stats().0, 0);
        let _ = dir;
    }

    #[test]
    fn cached_reads_reject_a_type_mismatch() {
        let (_dir, cache) = cache();
        let (image, kind) = cache.paths("https://example.org/x.jpg");
        std::fs::write(&image, b"<html>not an image</html>").unwrap();
        std::fs::write(&kind, "image/jpeg").unwrap();
        assert!(cache.read_cached("https://example.org/x.jpg").is_none());
    }
}
