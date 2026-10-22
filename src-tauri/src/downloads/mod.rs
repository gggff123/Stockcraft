//! Streaming download manager: progress events, cancellation, safe filenames, atomic rename and
//! optional attribution sidecars.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use base64::Engine as _;
use futures_util::StreamExt;
use tauri::{AppHandle, Emitter};
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;

use crate::database::Database;
use crate::error::{AppError, AppResult};
use crate::models::{AppSettings, Asset, DownloadProgress};
use crate::util::{extension_for, safe_filename_stem, safe_stem, unique_path, validate_remote_url};

const MAX_DOWNLOAD_BYTES: u64 = 200 * 1024 * 1024;
const FREE_SPACE_RESERVE: u64 = 32 * 1024 * 1024;

pub struct DownloadManager {
    app: AppHandle,
    db: Arc<Database>,
    settings: Arc<RwLock<AppSettings>>,
    client: reqwest::Client,
    tokens: Arc<Mutex<HashMap<i64, CancellationToken>>>,
}

impl DownloadManager {
    pub fn new(
        app: AppHandle,
        db: Arc<Database>,
        settings: Arc<RwLock<AppSettings>>,
    ) -> AppResult<Self> {
        Ok(Self {
            app,
            db,
            settings,
            client: crate::providers::http::HttpClient::download_client()?,
            tokens: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    pub fn start(&self, asset: &Asset, target_dir: Option<String>) -> AppResult<i64> {
        let settings = self.settings();
        let url = asset.download_url.clone().ok_or_else(|| {
            AppError::Policy("this source does not publish a permitted download URL".into())
        })?;
        // Generated images (AI Horde) arrive as inline base64 data URLs rather than a remote host.
        if !url.starts_with("data:") {
            validate_remote_url(&url)?;
        }
        if !asset.download_allowed {
            return Err(AppError::Policy(
                "this image is not marked as downloadable by its source".into(),
            ));
        }

        let dir = resolve_download_dir(target_dir.as_deref(), &settings)?;
        std::fs::create_dir_all(&dir)?;
        if !dir.is_dir() {
            return Err(AppError::Validation(format!(
                "download destination is not a folder: {}",
                dir.display()
            )));
        }

        let stem = safe_filename_stem(asset.title.as_deref().unwrap_or(&asset.provider_asset_id));
        let extension = extension_for(asset.mime_type.as_deref(), &url);
        let planned = dir.join(format!("{stem}.{extension}"));
        let planned = if settings.confirm_overwrite {
            unique_path(&planned)
        } else {
            planned
        };
        let attribution = asset.attribution.clone();
        // Keep inline base64 out of the downloads table; it is only useful for the transient asset.
        let mut record = asset.clone();
        if record
            .thumbnail_url
            .as_deref()
            .is_some_and(|url| url.starts_with("data:"))
        {
            record.thumbnail_url = None;
        }
        let id =
            self.db
                .insert_download(&record, &planned.to_string_lossy(), attribution.as_deref())?;

        let token = CancellationToken::new();
        {
            let mut tokens = self
                .tokens
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            tokens.insert(id, token.clone());
        }

        let app = self.app.clone();
        let db = Arc::clone(&self.db);
        let client = self.client.clone();
        let asset = asset.clone();
        let path = planned.clone();
        let settings = settings.clone();
        let tokens = Arc::clone(&self.tokens);
        tauri::async_runtime::spawn(async move {
            let manager = DownloadTask {
                app,
                db,
                client,
                asset,
                id,
                token,
                planned: path,
                settings,
            };
            manager.run().await;
            let mut tokens = tokens
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            tokens.remove(&id);
        });
        Ok(id)
    }

    pub fn cancel(&self, id: i64) -> AppResult<()> {
        let tokens = self
            .tokens
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let token = tokens
            .get(&id)
            .ok_or_else(|| AppError::NotFound("download is not running".into()))?;
        token.cancel();
        Ok(())
    }

    fn settings(&self) -> AppSettings {
        self.settings
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }
}

pub fn default_download_dir() -> PathBuf {
    dirs::picture_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")))
        .join("Stockcraft")
}

pub fn resolve_download_dir(target: Option<&str>, settings: &AppSettings) -> AppResult<PathBuf> {
    let raw = target
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| {
            if settings.download_dir.trim().is_empty() {
                ""
            } else {
                settings.download_dir.as_str()
            }
        });
    let path = if raw.is_empty() {
        default_download_dir()
    } else {
        if raw.contains('\0') {
            return Err(AppError::validation(
                "download folder contains an invalid character",
            ));
        }
        let path = PathBuf::from(raw);
        if !path.is_absolute() {
            return Err(AppError::validation(
                "download folder must be an absolute path",
            ));
        }
        path
    };
    Ok(path)
}

struct DownloadTask {
    app: AppHandle,
    db: Arc<Database>,
    client: reqwest::Client,
    asset: Asset,
    id: i64,
    token: CancellationToken,
    planned: PathBuf,
    settings: AppSettings,
}

impl DownloadTask {
    async fn run(self) {
        let result = self.transfer().await;
        match result {
            Ok((path, bytes)) => {
                if let Err(error) = self.db.update_download(
                    self.id,
                    "done",
                    bytes,
                    None,
                    Some(&path.to_string_lossy()),
                ) {
                    tracing::error!(%error, download = self.id, "could not finalise download record");
                }
                self.emit(bytes, bytes, "done", Some(&path), None);
                tracing::info!(download = self.id, bytes, path = %path.display(), "download complete");
                if self.settings.write_attribution_sidecar {
                    if let Err(error) = write_sidecars(&self.asset, &path) {
                        tracing::warn!(%error, download = self.id, "could not write attribution sidecar");
                    }
                }
                if self.settings.open_folder_after_download {
                    if let Some(parent) = path.parent() {
                        let _ = opener::open(parent);
                    }
                }
            }
            Err(DownloadError::Cancelled { bytes }) => {
                let _ = self
                    .db
                    .update_download(self.id, "cancelled", bytes, None, None);
                self.emit(bytes, 0, "cancelled", None, None);
                tracing::info!(download = self.id, "download cancelled");
            }
            Err(DownloadError::Failed(error)) => {
                let _ =
                    self.db
                        .update_download(self.id, "error", 0, Some(&error.to_string()), None);
                self.emit(0, 0, "error", None, Some(&error.to_string()));
                tracing::warn!(download = self.id, %error, "download failed");
            }
        }
    }

    async fn transfer(&self) -> Result<(PathBuf, u64), DownloadError> {
        let url = self.asset.download_url.clone().ok_or_else(|| {
            DownloadError::Failed(AppError::Policy("download URL disappeared".into()))
        })?;
        if self.token.is_cancelled() {
            return Err(DownloadError::Cancelled { bytes: 0 });
        }
        if let Some(inline) = url.strip_prefix("data:") {
            return self.transfer_inline(inline).await;
        }
        let response = self.client.get(&url).send().await.map_err(|error| {
            DownloadError::Failed(AppError::Network(format!(
                "download request failed: {error}"
            )))
        })?;
        if !response.status().is_success() {
            return Err(DownloadError::Failed(AppError::Unavailable(format!(
                "download server returned HTTP {}",
                response.status()
            ))));
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
            return Err(DownloadError::Failed(AppError::Malformed(format!(
                "server returned {content_type:?} instead of an image"
            ))));
        }
        let declared = response.content_length().unwrap_or(0);
        if declared > MAX_DOWNLOAD_BYTES {
            return Err(DownloadError::Failed(AppError::Malformed(
                "file is larger than the 200 MB download limit".into(),
            )));
        }
        let parent = self.planned.parent().unwrap_or_else(|| Path::new("."));
        if let Ok(available) = fs2::available_space(parent) {
            if declared > 0 && declared.saturating_add(FREE_SPACE_RESERVE) > available {
                return Err(DownloadError::Failed(AppError::Io(
                    "not enough free disk space for this download".into(),
                )));
            }
        }

        // If the server's content type implies a different extension, adjust the final name now.
        let extension = extension_for(Some(&content_type), &url);
        let mut final_path = self.planned.clone();
        if final_path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.eq_ignore_ascii_case(&extension))
            != Some(true)
        {
            let stem = final_path
                .file_stem()
                .map(|stem| stem.to_string_lossy().to_string())
                .unwrap_or_else(|| safe_stem(&self.asset.provider_asset_id));
            let candidate = parent.join(format!("{stem}.{extension}"));
            final_path = if self.settings.confirm_overwrite {
                unique_path(&candidate)
            } else {
                candidate
            };
        }

        let temp = parent.join(format!(
            ".stockcraft-{}-{}.part",
            self.id,
            uuid::Uuid::new_v4()
        ));
        let mut file = tokio::fs::File::create(&temp).await.map_err(|error| {
            DownloadError::Failed(AppError::Io(format!(
                "could not create temporary file: {error}"
            )))
        })?;
        let mut stream = response.bytes_stream();
        let mut received = 0u64;
        let mut last_emit = Instant::now() - Duration::from_secs(5);
        loop {
            if self.token.is_cancelled() {
                drop(file);
                let _ = tokio::fs::remove_file(&temp).await;
                return Err(DownloadError::Cancelled { bytes: received });
            }
            let chunk = tokio::select! {
                _ = self.token.cancelled() => {
                    drop(file);
                    let _ = tokio::fs::remove_file(&temp).await;
                    return Err(DownloadError::Cancelled { bytes: received });
                }
                chunk = stream.next() => chunk,
            };
            let Some(chunk) = chunk else {
                break;
            };
            let chunk = chunk.map_err(|error| {
                DownloadError::Failed(AppError::Network(format!(
                    "download stream failed: {error}"
                )))
            })?;
            received = received.saturating_add(chunk.len() as u64);
            if received > MAX_DOWNLOAD_BYTES {
                drop(file);
                let _ = tokio::fs::remove_file(&temp).await;
                return Err(DownloadError::Failed(AppError::Malformed(
                    "file is larger than the 200 MB download limit".into(),
                )));
            }
            file.write_all(&chunk).await.map_err(|error| {
                DownloadError::Failed(AppError::Io(format!("could not write file: {error}")))
            })?;
            if last_emit.elapsed() >= Duration::from_millis(220) {
                last_emit = Instant::now();
                self.emit(received, declared, "running", Some(&final_path), None);
            }
        }
        file.flush().await.map_err(|error| {
            DownloadError::Failed(AppError::Io(format!("could not flush file: {error}")))
        })?;
        drop(file);
        if received == 0 {
            let _ = tokio::fs::remove_file(&temp).await;
            return Err(DownloadError::Failed(AppError::Malformed(
                "server returned an empty file".into(),
            )));
        }
        if final_path.exists() && !self.settings.confirm_overwrite {
            let _ = std::fs::remove_file(&final_path);
        }
        if let Err(error) = tokio::fs::rename(&temp, &final_path).await {
            let _ = tokio::fs::remove_file(&temp).await;
            return Err(DownloadError::Failed(AppError::Io(format!(
                "could not finalise the download: {error}"
            ))));
        }
        Ok((final_path, received))
    }

    /// Write an inline `data:` image (AI Horde) without touching the network.
    async fn transfer_inline(&self, inline: &str) -> Result<(PathBuf, u64), DownloadError> {
        let (mime, bytes) = decode_inline_image(inline).map_err(DownloadError::Failed)?;
        if bytes.len() as u64 > MAX_DOWNLOAD_BYTES {
            return Err(DownloadError::Failed(AppError::Malformed(
                "file is larger than the 200 MB download limit".into(),
            )));
        }
        let parent = self.planned.parent().unwrap_or_else(|| Path::new("."));
        let extension = extension_for(Some(&mime), "");
        let mut final_path = self.planned.clone();
        if final_path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.eq_ignore_ascii_case(&extension))
            != Some(true)
        {
            let stem = final_path
                .file_stem()
                .map(|stem| stem.to_string_lossy().to_string())
                .unwrap_or_else(|| safe_stem(&self.asset.provider_asset_id));
            let candidate = parent.join(format!("{stem}.{extension}"));
            final_path = if self.settings.confirm_overwrite {
                unique_path(&candidate)
            } else {
                candidate
            };
        }
        if self.token.is_cancelled() {
            return Err(DownloadError::Cancelled { bytes: 0 });
        }
        if final_path.exists() && !self.settings.confirm_overwrite {
            let _ = std::fs::remove_file(&final_path);
        }
        tokio::fs::write(&final_path, &bytes)
            .await
            .map_err(|error| {
                DownloadError::Failed(AppError::Io(format!("could not write file: {error}")))
            })?;
        Ok((final_path, bytes.len() as u64))
    }

    fn emit(
        &self,
        received: u64,
        total: u64,
        status: &str,
        file_path: Option<&Path>,
        error: Option<&str>,
    ) {
        let payload = DownloadProgress {
            download_id: self.id,
            asset_id: self.asset.id.clone(),
            received,
            total,
            status: status.to_string(),
            file_path: file_path
                .map(|path| path.to_string_lossy().to_string())
                .unwrap_or_default(),
            error: error.map(str::to_string),
        };
        let _ = self.app.emit("download://progress", payload);
    }
}

enum DownloadError {
    Cancelled { bytes: u64 },
    Failed(AppError),
}

/// Decode the `data:` payload of a generated image, returning its MIME type and bytes.
fn decode_inline_image(inline: &str) -> AppResult<(String, Vec<u8>)> {
    let (meta, payload) = inline
        .split_once(',')
        .ok_or_else(|| AppError::Malformed("inline image data has no comma separator".into()))?;
    if !meta.to_ascii_lowercase().contains(";base64") {
        return Err(AppError::Malformed(
            "inline image data is not base64 encoded".into(),
        ));
    }
    let mime = meta
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    if !mime.starts_with("image/") {
        return Err(AppError::Malformed(format!(
            "inline data is {mime:?}, not an image"
        )));
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(payload.trim())
        .map_err(|_| AppError::Malformed("inline image data could not be decoded".into()))?;
    if bytes.is_empty() {
        return Err(AppError::Malformed("inline image is empty".into()));
    }
    Ok((mime, bytes))
}

fn write_sidecars(asset: &Asset, path: &Path) -> AppResult<()> {
    let stem = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().to_string())
        .unwrap_or_else(|| "image".into());
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let attribution = parent.join(format!("{stem}-attribution.txt"));
    let lines = [
        format!(
            "Title: {}",
            asset.title.as_deref().unwrap_or("Untitled image")
        ),
        format!("Creator: {}", asset.creator.as_deref().unwrap_or("unknown")),
        format!(
            "Source: {}",
            asset.source_name.as_deref().unwrap_or(&asset.provider)
        ),
        format!(
            "Source page: {}",
            asset.page_url.as_deref().unwrap_or("unknown")
        ),
        format!(
            "Download URL: {}",
            asset.download_url.as_deref().unwrap_or("unknown")
        ),
        format!(
            "Licence: {}",
            asset.license_name.as_deref().unwrap_or("unknown")
        ),
        format!(
            "Licence URL: {}",
            asset.license_url.as_deref().unwrap_or("unknown")
        ),
        format!(
            "Attribution: {}",
            asset.attribution.as_deref().unwrap_or("unknown")
        ),
        format!("Asset id: {}", asset.id),
        format!("Downloaded: {}", crate::util::now_iso()),
        String::new(),
        "This file records the source and licence metadata Stockcraft received.".into(),
        "Check the source page and licence before reuse.".into(),
    ];
    std::fs::write(attribution, lines.join("\n"))?;
    let json = parent.join(format!("{stem}.json"));
    std::fs::write(json, serde_json::to_string_pretty(asset)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset() -> Asset {
        let mut asset = Asset::new("test", "42");
        asset.title = Some("A/B: C*? D".into());
        asset.mime_type = Some("image/png".into());
        asset.download_url = Some("https://example.org/a.png".into());
        asset.download_allowed = true;
        asset
    }

    #[test]
    fn resolves_absolute_directories_only() {
        let mut settings = AppSettings {
            download_dir: "C:\\Temp\\Stockcraft".into(),
            ..AppSettings::default()
        };
        assert!(resolve_download_dir(None, &settings).unwrap().is_absolute());
        assert!(resolve_download_dir(Some("relative"), &settings).is_err());
        assert!(resolve_download_dir(Some("C:\\Other"), &settings)
            .unwrap()
            .ends_with("Other"));
        settings.download_dir = String::new();
        assert!(resolve_download_dir(None, &settings).unwrap().is_absolute());
    }

    #[test]
    fn unique_path_never_overwrites() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("image.png");
        std::fs::write(&path, b"x").unwrap();
        let unique = unique_path(&path);
        assert_eq!(
            unique.file_name().unwrap().to_string_lossy(),
            "image (1).png"
        );
        std::fs::write(&unique, b"x").unwrap();
        assert_eq!(
            unique_path(&path).file_name().unwrap().to_string_lossy(),
            "image (2).png"
        );
    }

    #[test]
    fn sidecars_are_honest_and_named_after_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("image.png");
        std::fs::write(&path, b"x").unwrap();
        write_sidecars(&asset(), &path).unwrap();
        let attribution =
            std::fs::read_to_string(dir.path().join("image-attribution.txt")).unwrap();
        assert!(attribution.contains("Source page:"));
        assert!(attribution.contains("Licence:"));
        assert!(attribution.contains("Check the source page and licence before reuse."));
        let json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.path().join("image.json")).unwrap())
                .unwrap();
        assert_eq!(json["id"], "test:42");
    }

    #[test]
    fn reserved_filenames_are_made_safe() {
        let stem = safe_stem("CON");
        assert_eq!(stem, "CON-image");
        assert!(!stem.contains('/'));
    }

    #[test]
    fn decodes_inline_generated_images() {
        let (mime, bytes) = decode_inline_image("image/webp;base64,UklGRg==").unwrap();
        assert_eq!(mime, "image/webp");
        assert_eq!(&bytes[..4], b"RIFF");
        assert!(decode_inline_image("image/png;base64,not base64!").is_err());
        assert!(decode_inline_image("text/plain;base64,aGk=").is_err());
        assert!(decode_inline_image("image/png;base64,").is_err());
        assert!(decode_inline_image("image/png;base64").is_err());
    }
}
