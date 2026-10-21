//! SQLite persistence: schema migrations, library/favourites, collections, history, settings,
//! metadata cache and download records. All access is serialised through a single connection.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;

use crate::error::{AppError, AppResult};
use crate::models::{
    AppSettings, Asset, CacheStats, Collection, DownloadRecord, HistoryItem, ProviderPage,
};
use crate::util::now_iso;

const SETTINGS_KEY: &str = "app_settings";

pub struct Database {
    conn: Mutex<Connection>,
    path: PathBuf,
}

impl Database {
    pub fn open(path: &Path) -> AppResult<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path).map_err(db_error)?;
        Self::from_connection(conn, path.to_path_buf())
    }

    #[cfg(test)]
    pub fn open_in_memory() -> AppResult<Self> {
        let conn = Connection::open_in_memory().map_err(db_error)?;
        Self::from_connection(conn, PathBuf::from(":memory:"))
    }

    fn from_connection(conn: Connection, path: PathBuf) -> AppResult<Self> {
        conn.pragma_update(None, "journal_mode", "WAL").ok();
        conn.pragma_update(None, "synchronous", "NORMAL").ok();
        conn.pragma_update(None, "foreign_keys", "ON").ok();
        conn.busy_timeout(std::time::Duration::from_secs(5)).ok();
        let db = Self {
            conn: Mutex::new(conn),
            path,
        };
        db.migrate()?;
        Ok(db)
    }

    fn lock(&self) -> AppResult<std::sync::MutexGuard<'_, Connection>> {
        self.conn
            .lock()
            .map_err(|_| AppError::Db("database lock poisoned".into()))
    }

    fn migrate(&self) -> AppResult<()> {
        let conn = self.lock()?;
        let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if version < 1 {
            conn.execute_batch(
                r#"
                CREATE TABLE IF NOT EXISTS assets_library(
                    asset_id TEXT PRIMARY KEY,
                    provider TEXT NOT NULL,
                    provider_asset_id TEXT NOT NULL,
                    title TEXT,
                    json TEXT NOT NULL,
                    is_favorite INTEGER NOT NULL DEFAULT 0,
                    saved_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                );
                CREATE INDEX IF NOT EXISTS idx_library_favorite ON assets_library(is_favorite, saved_at DESC);
                CREATE TABLE IF NOT EXISTS collections(
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    name TEXT NOT NULL COLLATE NOCASE UNIQUE,
                    description TEXT,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                );
                CREATE TABLE IF NOT EXISTS collection_items(
                    collection_id INTEGER NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
                    asset_id TEXT NOT NULL REFERENCES assets_library(asset_id) ON DELETE CASCADE,
                    added_at TEXT NOT NULL,
                    PRIMARY KEY(collection_id, asset_id)
                );
                CREATE TABLE IF NOT EXISTS search_history(
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    query TEXT NOT NULL,
                    providers TEXT NOT NULL,
                    result_count INTEGER NOT NULL DEFAULT 0,
                    searched_at TEXT NOT NULL
                );
                CREATE TABLE IF NOT EXISTS settings(
                    key TEXT PRIMARY KEY,
                    value TEXT NOT NULL
                );
                CREATE TABLE IF NOT EXISTS downloads(
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    asset_id TEXT NOT NULL,
                    provider TEXT NOT NULL,
                    title TEXT NOT NULL,
                    file_path TEXT NOT NULL,
                    source_url TEXT,
                    license_name TEXT,
                    license_url TEXT,
                    attribution TEXT,
                    bytes INTEGER NOT NULL DEFAULT 0,
                    status TEXT NOT NULL,
                    error TEXT,
                    thumbnail_url TEXT,
                    created_at TEXT NOT NULL
                );
                CREATE TABLE IF NOT EXISTS metadata_cache(
                    cache_key TEXT PRIMARY KEY,
                    provider TEXT NOT NULL,
                    payload TEXT NOT NULL,
                    fetched_at INTEGER NOT NULL,
                    expires_at INTEGER NOT NULL
                );
                CREATE INDEX IF NOT EXISTS idx_metadata_expiry ON metadata_cache(expires_at);
                PRAGMA user_version = 1;
                "#,
            )?;
        }
        Ok(())
    }

    /* ----------------------------------------------------------------- settings */

    pub fn load_settings(&self) -> AppResult<AppSettings> {
        let conn = self.lock()?;
        let raw: Option<String> = conn
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                params![SETTINGS_KEY],
                |row| row.get(0),
            )
            .optional()?;
        drop(conn);
        match raw {
            Some(raw) => match serde_json::from_str::<AppSettings>(&raw) {
                Ok(settings) => Ok(normalize_settings(settings)),
                Err(_) => Ok(AppSettings::default()),
            },
            None => Ok(AppSettings::default()),
        }
    }

    pub fn save_settings(&self, settings: &AppSettings) -> AppResult<()> {
        let value = serde_json::to_string(settings)?;
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO settings(key, value) VALUES(?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![SETTINGS_KEY, value],
        )?;
        Ok(())
    }

    /* ------------------------------------------------------------------ library */

    pub fn save_asset(&self, asset: &Asset, favorite: Option<bool>) -> AppResult<()> {
        let now = now_iso();
        let json = serde_json::to_string(asset)?;
        let conn = self.lock()?;
        match favorite {
            Some(favorite) => conn.execute(
                "INSERT INTO assets_library(asset_id, provider, provider_asset_id, title, json, is_favorite, saved_at, updated_at)
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)
                 ON CONFLICT(asset_id) DO UPDATE SET
                    title = excluded.title,
                    json = excluded.json,
                    is_favorite = excluded.is_favorite,
                    updated_at = excluded.updated_at",
                params![
                    asset.id,
                    asset.provider,
                    asset.provider_asset_id,
                    asset.title,
                    json,
                    i64::from(favorite),
                    now
                ],
            )?,
            None => conn.execute(
                "INSERT INTO assets_library(asset_id, provider, provider_asset_id, title, json, is_favorite, saved_at, updated_at)
                 VALUES(?1, ?2, ?3, ?4, ?5, 0, ?6, ?6)
                 ON CONFLICT(asset_id) DO UPDATE SET
                    title = excluded.title,
                    json = excluded.json,
                    updated_at = excluded.updated_at",
                params![asset.id, asset.provider, asset.provider_asset_id, asset.title, json, now],
            )?,
        };
        Ok(())
    }

    pub fn toggle_favorite(&self, asset: &Asset) -> AppResult<bool> {
        let conn = self.lock()?;
        let existing: Option<i64> = conn
            .query_row(
                "SELECT is_favorite FROM assets_library WHERE asset_id = ?1",
                params![asset.id],
                |row| row.get(0),
            )
            .optional()?;
        match existing {
            Some(1) => {
                conn.execute(
                    "UPDATE assets_library SET is_favorite = 0, updated_at = ?2 WHERE asset_id = ?1",
                    params![asset.id, now_iso()],
                )?;
                Ok(false)
            }
            Some(_) => {
                conn.execute(
                    "UPDATE assets_library SET is_favorite = 1, updated_at = ?2 WHERE asset_id = ?1",
                    params![asset.id, now_iso()],
                )?;
                Ok(true)
            }
            None => {
                drop(conn);
                self.save_asset(asset, Some(true))?;
                Ok(true)
            }
        }
    }

    pub fn favorite_ids(&self) -> AppResult<Vec<String>> {
        let conn = self.lock()?;
        let mut stmt = conn.prepare("SELECT asset_id FROM assets_library WHERE is_favorite = 1")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn list_favorites(&self, query: &str, limit: i64, offset: i64) -> AppResult<Vec<Asset>> {
        let conn = self.lock()?;
        let mut stmt = conn.prepare(
            "SELECT json FROM assets_library WHERE is_favorite = 1 ORDER BY saved_at DESC LIMIT ?1 OFFSET ?2",
        )?;
        let rows = stmt.query_map(params![limit.clamp(1, 500), offset.max(0)], |row| {
            row.get::<_, String>(0)
        })?;
        let mut assets = Vec::new();
        for row in rows {
            if let Ok(asset) = serde_json::from_str::<Asset>(&row?) {
                if matches_library_query(&asset, query) {
                    assets.push(asset);
                }
            }
        }
        Ok(assets)
    }

    pub fn list_library(
        &self,
        query: &str,
        collection_id: Option<i64>,
        limit: i64,
        offset: i64,
    ) -> AppResult<Vec<Asset>> {
        let conn = self.lock()?;
        let mut assets = Vec::new();
        if let Some(collection_id) = collection_id {
            let mut stmt = conn.prepare(
                "SELECT a.json FROM assets_library a
                 JOIN collection_items ci ON ci.asset_id = a.asset_id
                 WHERE ci.collection_id = ?1
                 ORDER BY ci.added_at DESC LIMIT ?2 OFFSET ?3",
            )?;
            let rows = stmt.query_map(
                params![collection_id, limit.clamp(1, 500), offset.max(0)],
                |row| row.get::<_, String>(0),
            )?;
            for row in rows {
                if let Ok(asset) = serde_json::from_str::<Asset>(&row?) {
                    assets.push(asset);
                }
            }
        } else {
            let mut stmt = conn.prepare(
                "SELECT json FROM assets_library ORDER BY saved_at DESC LIMIT ?1 OFFSET ?2",
            )?;
            let rows = stmt.query_map(params![limit.clamp(1, 500), offset.max(0)], |row| {
                row.get::<_, String>(0)
            })?;
            for row in rows {
                if let Ok(asset) = serde_json::from_str::<Asset>(&row?) {
                    assets.push(asset);
                }
            }
        }
        Ok(assets
            .into_iter()
            .filter(|asset| matches_library_query(asset, query))
            .collect())
    }

    pub fn get_saved_asset(&self, asset_id: &str) -> AppResult<Option<Asset>> {
        let conn = self.lock()?;
        let raw: Option<String> = conn
            .query_row(
                "SELECT json FROM assets_library WHERE asset_id = ?1",
                params![asset_id],
                |row| row.get(0),
            )
            .optional()?;
        Ok(raw.and_then(|raw| serde_json::from_str(&raw).ok()))
    }

    /* -------------------------------------------------------------- collections */

    pub fn list_collections(&self) -> AppResult<Vec<Collection>> {
        let conn = self.lock()?;
        let mut stmt = conn.prepare(
            "SELECT c.id, c.name, c.description, c.created_at, c.updated_at,
                    (SELECT COUNT(*) FROM collection_items ci WHERE ci.collection_id = c.id)
             FROM collections c ORDER BY c.name COLLATE NOCASE ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(Collection {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                created_at: row.get(3)?,
                updated_at: row.get(4)?,
                item_count: row.get(5)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn get_collection(&self, id: i64) -> AppResult<Collection> {
        let conn = self.lock()?;
        conn.query_row(
            "SELECT c.id, c.name, c.description, c.created_at, c.updated_at,
                    (SELECT COUNT(*) FROM collection_items ci WHERE ci.collection_id = c.id)
             FROM collections c WHERE c.id = ?1",
            params![id],
            |row| {
                Ok(Collection {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    description: row.get(2)?,
                    created_at: row.get(3)?,
                    updated_at: row.get(4)?,
                    item_count: row.get(5)?,
                })
            },
        )
        .optional()?
        .ok_or_else(|| AppError::NotFound("collection not found".into()))
    }

    pub fn create_collection(&self, name: &str, description: &str) -> AppResult<Collection> {
        let name = validate_collection_name(name)?;
        let now = now_iso();
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO collections(name, description, created_at, updated_at) VALUES(?1, ?2, ?3, ?3)",
            params![name, description.trim(), now],
        )
        .map_err(|error| {
            if is_unique_violation(&error) {
                AppError::Conflict("a collection with that name already exists".into())
            } else {
                error.into()
            }
        })?;
        let id = conn.last_insert_rowid();
        drop(conn);
        self.get_collection(id)
    }

    pub fn rename_collection(
        &self,
        id: i64,
        name: &str,
        description: &str,
    ) -> AppResult<Collection> {
        let name = validate_collection_name(name)?;
        {
            let conn = self.lock()?;
            let changed = conn
                .execute(
                    "UPDATE collections SET name = ?1, description = ?2, updated_at = ?3 WHERE id = ?4",
                    params![name, description.trim(), now_iso(), id],
                )
                .map_err(|error| {
                    if is_unique_violation(&error) {
                        AppError::Conflict("a collection with that name already exists".into())
                    } else {
                        error.into()
                    }
                })?;
            if changed == 0 {
                return Err(AppError::NotFound("collection not found".into()));
            }
        }
        self.get_collection(id)
    }

    pub fn delete_collection(&self, id: i64) -> AppResult<()> {
        let conn = self.lock()?;
        let changed = conn.execute("DELETE FROM collections WHERE id = ?1", params![id])?;
        if changed == 0 {
            return Err(AppError::NotFound("collection not found".into()));
        }
        Ok(())
    }

    pub fn add_to_collection(&self, collection_id: i64, asset: &Asset) -> AppResult<Collection> {
        self.save_asset(asset, None)?;
        {
            let conn = self.lock()?;
            let exists: i64 = conn.query_row(
                "SELECT COUNT(*) FROM collections WHERE id = ?1",
                params![collection_id],
                |row| row.get(0),
            )?;
            if exists == 0 {
                return Err(AppError::NotFound("collection not found".into()));
            }
            conn.execute(
                "INSERT OR IGNORE INTO collection_items(collection_id, asset_id, added_at) VALUES(?1, ?2, ?3)",
                params![collection_id, asset.id, now_iso()],
            )?;
            conn.execute(
                "UPDATE collections SET updated_at = ?2 WHERE id = ?1",
                params![collection_id, now_iso()],
            )?;
        }
        self.get_collection(collection_id)
    }

    pub fn remove_from_collection(
        &self,
        collection_id: i64,
        asset_id: &str,
    ) -> AppResult<Collection> {
        {
            let conn = self.lock()?;
            conn.execute(
                "DELETE FROM collection_items WHERE collection_id = ?1 AND asset_id = ?2",
                params![collection_id, asset_id],
            )?;
            conn.execute(
                "UPDATE collections SET updated_at = ?2 WHERE id = ?1",
                params![collection_id, now_iso()],
            )?;
        }
        self.get_collection(collection_id)
    }

    /* ------------------------------------------------------------------ history */

    pub fn add_history(&self, query: &str, providers: &[String]) -> AppResult<i64> {
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO search_history(query, providers, result_count, searched_at) VALUES(?1, ?2, 0, ?3)",
            params![query, providers.join(","), now_iso()],
        )?;
        let id = conn.last_insert_rowid();
        // Keep the table small; the UI only needs recent queries.
        conn.execute(
            "DELETE FROM search_history WHERE id NOT IN (SELECT id FROM search_history ORDER BY id DESC LIMIT 200)",
            [],
        )?;
        Ok(id)
    }

    pub fn update_history_count(&self, id: i64, count: usize) -> AppResult<()> {
        let conn = self.lock()?;
        conn.execute(
            "UPDATE search_history SET result_count = ?2 WHERE id = ?1",
            params![id, count as i64],
        )?;
        Ok(())
    }

    pub fn list_history(&self, limit: i64) -> AppResult<Vec<HistoryItem>> {
        let conn = self.lock()?;
        let mut stmt = conn.prepare(
            "SELECT id, query, providers, result_count, searched_at
             FROM search_history ORDER BY id DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit.clamp(1, 100)], |row| {
            Ok(HistoryItem {
                id: row.get(0)?,
                query: row.get(1)?,
                providers: row.get(2)?,
                result_count: row.get(3)?,
                searched_at: row.get(4)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn delete_history_item(&self, id: i64) -> AppResult<()> {
        let conn = self.lock()?;
        conn.execute("DELETE FROM search_history WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn clear_history(&self) -> AppResult<()> {
        let conn = self.lock()?;
        conn.execute("DELETE FROM search_history", [])?;
        Ok(())
    }

    /* ----------------------------------------------------------- metadata cache */

    pub fn get_metadata(&self, key: &str) -> AppResult<Option<ProviderPage>> {
        let now = chrono::Utc::now().timestamp();
        let conn = self.lock()?;
        let raw: Option<String> = conn
            .query_row(
                "SELECT payload FROM metadata_cache WHERE cache_key = ?1 AND expires_at > ?2",
                params![key, now],
                |row| row.get(0),
            )
            .optional()?;
        Ok(raw.and_then(|raw| serde_json::from_str(&raw).ok()))
    }

    pub fn put_metadata(
        &self,
        key: &str,
        provider: &str,
        page: &ProviderPage,
        ttl_minutes: u32,
    ) -> AppResult<()> {
        if ttl_minutes == 0 {
            return Ok(());
        }
        let now = chrono::Utc::now().timestamp();
        let expires = now + i64::from(ttl_minutes) * 60;
        let payload = serde_json::to_string(page)?;
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO metadata_cache(cache_key, provider, payload, fetched_at, expires_at)
             VALUES(?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(cache_key) DO UPDATE SET
                payload = excluded.payload,
                fetched_at = excluded.fetched_at,
                expires_at = excluded.expires_at",
            params![key, provider, payload, now, expires],
        )?;
        conn.execute(
            "DELETE FROM metadata_cache WHERE expires_at <= ?1",
            params![now],
        )?;
        Ok(())
    }

    pub fn clear_metadata(&self) -> AppResult<u64> {
        let conn = self.lock()?;
        let bytes: i64 = conn.query_row(
            "SELECT COALESCE(SUM(LENGTH(payload)), 0) FROM metadata_cache",
            [],
            |row| row.get(0),
        )?;
        conn.execute("DELETE FROM metadata_cache", [])?;
        Ok(bytes.max(0) as u64)
    }

    /* ------------------------------------------------------------------ downloads */

    #[allow(clippy::too_many_arguments)]
    pub fn insert_download(
        &self,
        asset: &Asset,
        file_path: &str,
        attribution: Option<&str>,
    ) -> AppResult<i64> {
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO downloads(asset_id, provider, title, file_path, source_url, license_name,
                                   license_url, attribution, bytes, status, error, thumbnail_url, created_at)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0, 'queued', NULL, ?9, ?10)",
            params![
                asset.id,
                asset.provider,
                asset.title.clone().unwrap_or_else(|| "Untitled image".into()),
                file_path,
                asset.page_url,
                asset.license_name,
                asset.license_url,
                attribution,
                asset.thumbnail_url,
                now_iso()
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn update_download(
        &self,
        id: i64,
        status: &str,
        bytes: u64,
        error: Option<&str>,
        file_path: Option<&str>,
    ) -> AppResult<()> {
        let conn = self.lock()?;
        conn.execute(
            "UPDATE downloads SET status = ?2, bytes = ?3, error = ?4, file_path = COALESCE(?5, file_path)
             WHERE id = ?1",
            params![id, status, bytes as i64, error, file_path],
        )?;
        Ok(())
    }

    pub fn get_download(&self, id: i64) -> AppResult<DownloadRecord> {
        let conn = self.lock()?;
        conn.query_row(
            "SELECT id, asset_id, provider, title, file_path, source_url, license_name, license_url,
                    bytes, status, error, created_at, thumbnail_url
             FROM downloads WHERE id = ?1",
            params![id],
            map_download,
        )
        .optional()?
        .ok_or_else(|| AppError::NotFound("download not found".into()))
    }

    pub fn list_downloads(&self, limit: i64) -> AppResult<Vec<DownloadRecord>> {
        let conn = self.lock()?;
        let mut stmt = conn.prepare(
            "SELECT id, asset_id, provider, title, file_path, source_url, license_name, license_url,
                    bytes, status, error, created_at, thumbnail_url
             FROM downloads ORDER BY id DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit.clamp(1, 500)], map_download)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn clear_downloads(&self) -> AppResult<usize> {
        let conn = self.lock()?;
        let changed = conn.execute("DELETE FROM downloads", [])?;
        Ok(changed)
    }

    /* --------------------------------------------------------------------- stats */

    pub fn cache_stats(&self) -> AppResult<CacheStats> {
        let conn = self.lock()?;
        let metadata_rows = count(&conn, "SELECT COUNT(*) FROM metadata_cache")?;
        let library_items = count(&conn, "SELECT COUNT(*) FROM assets_library")?;
        let favorites = count(
            &conn,
            "SELECT COUNT(*) FROM assets_library WHERE is_favorite = 1",
        )?;
        let collections = count(&conn, "SELECT COUNT(*) FROM collections")?;
        let downloads = count(&conn, "SELECT COUNT(*) FROM downloads")?;
        let db_bytes = std::fs::metadata(&self.path).map(|m| m.len()).unwrap_or(0);
        Ok(CacheStats {
            thumbnail_bytes: 0,
            thumbnail_files: 0,
            metadata_rows,
            library_items,
            favorites,
            collections,
            downloads,
            db_bytes,
        })
    }
}

fn matches_library_query(asset: &Asset, query: &str) -> bool {
    let needle = query.trim().to_ascii_lowercase();
    if needle.is_empty() {
        return true;
    }
    asset
        .title
        .as_deref()
        .unwrap_or("")
        .to_ascii_lowercase()
        .contains(&needle)
        || asset
            .creator
            .as_deref()
            .unwrap_or("")
            .to_ascii_lowercase()
            .contains(&needle)
        || asset
            .tags
            .iter()
            .any(|tag| tag.to_ascii_lowercase().contains(&needle))
}

fn count(conn: &Connection, sql: &str) -> AppResult<i64> {
    Ok(conn.query_row(sql, [], |row| row.get(0))?)
}

fn map_download(row: &rusqlite::Row<'_>) -> rusqlite::Result<DownloadRecord> {
    Ok(DownloadRecord {
        id: row.get(0)?,
        asset_id: row.get(1)?,
        provider: row.get(2)?,
        title: row.get(3)?,
        file_path: row.get(4)?,
        source_url: row.get(5)?,
        license_name: row.get(6)?,
        license_url: row.get(7)?,
        bytes: row.get::<_, i64>(8)?.max(0) as u64,
        status: row.get(9)?,
        error: row.get(10)?,
        created_at: row.get(11)?,
        thumbnail_url: row.get(12)?,
    })
}

fn validate_collection_name(name: &str) -> AppResult<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::validation("collection name is required"));
    }
    if name.chars().count() > 80 {
        return Err(AppError::validation(
            "collection name is limited to 80 characters",
        ));
    }
    if name.chars().any(|ch| ch.is_control()) {
        return Err(AppError::validation(
            "collection name contains control characters",
        ));
    }
    Ok(name.to_string())
}

fn is_unique_violation(error: &rusqlite::Error) -> bool {
    matches!(error, rusqlite::Error::SqliteFailure(inner, _) if inner.code == rusqlite::ErrorCode::ConstraintViolation)
}

fn db_error(error: rusqlite::Error) -> AppError {
    AppError::Db(format!("database: {error}"))
}

fn normalize_settings(mut settings: AppSettings) -> AppSettings {
    if !matches!(settings.theme.as_str(), "light" | "dark" | "system") {
        settings.theme = AppSettings::default().theme;
    }
    if !matches!(settings.density.as_str(), "cozy" | "standard" | "dense") {
        settings.density = AppSettings::default().density;
    }
    settings.results_per_page = settings.results_per_page.clamp(4, 100);
    settings.thumbnail_cache_mb = settings.thumbnail_cache_mb.clamp(0, 8192);
    settings.metadata_cache_minutes = settings.metadata_cache_minutes.clamp(0, 10080);
    if settings.enabled_providers.is_empty() {
        settings.enabled_providers = AppSettings::default().enabled_providers;
    }
    settings
}

/// Apply a partial settings patch, validating each known field. Unknown fields are rejected so a
/// typo can never silently do nothing.
pub fn apply_settings_patch(
    settings: &mut AppSettings,
    patch: &Value,
    known_providers: &[String],
) -> AppResult<()> {
    let object = patch
        .as_object()
        .ok_or_else(|| AppError::validation("settings patch must be an object"))?;
    for (key, value) in object {
        match key.as_str() {
            "theme" => settings.theme = one_of(value, key, &["light", "dark", "system"])?,
            "density" => settings.density = one_of(value, key, &["cozy", "standard", "dense"])?,
            "resultsPerPage" => settings.results_per_page = number(value, key, 4, 100)? as u32,
            "thumbnailCacheMb" => settings.thumbnail_cache_mb = number(value, key, 0, 8192)? as u32,
            "metadataCacheMinutes" => {
                settings.metadata_cache_minutes = number(value, key, 0, 10080)? as u32
            }
            "confirmOverwrite" => settings.confirm_overwrite = boolean(value, key)?,
            "writeAttributionSidecar" => settings.write_attribution_sidecar = boolean(value, key)?,
            "openFolderAfterDownload" => settings.open_folder_after_download = boolean(value, key)?,
            "safeSearch" => settings.safe_search = boolean(value, key)?,
            "reducedMotion" => settings.reduced_motion = boolean(value, key)?,
            "cacheThumbnails" => settings.cache_thumbnails = boolean(value, key)?,
            "historyEnabled" => settings.history_enabled = boolean(value, key)?,
            "downloadDir" => settings.download_dir = download_dir(value)?,
            "pexelsApiKey" => settings.pexels_api_key = secret(value, key)?,
            "pixabayApiKey" => settings.pixabay_api_key = secret(value, key)?,
            "freepikApiKey" => settings.freepik_api_key = secret(value, key)?,
            "enabledProviders" => {
                let list = value
                    .as_array()
                    .ok_or_else(|| AppError::validation("enabledProviders must be an array"))?;
                let mut ids = Vec::new();
                for item in list {
                    let id = item.as_str().ok_or_else(|| {
                        AppError::validation("enabledProviders must contain strings")
                    })?;
                    if !known_providers.iter().any(|known| known == id) {
                        return Err(AppError::validation(format!("unknown provider: {id}")));
                    }
                    if !ids.iter().any(|existing| existing == id) {
                        ids.push(id.to_string());
                    }
                }
                if ids.is_empty() {
                    return Err(AppError::validation(
                        "at least one source must stay enabled",
                    ));
                }
                settings.enabled_providers = ids;
            }
            other => return Err(AppError::validation(format!("unknown setting: {other}"))),
        }
    }
    Ok(())
}

fn one_of(value: &Value, key: &str, allowed: &[&str]) -> AppResult<String> {
    let text = value
        .as_str()
        .ok_or_else(|| AppError::validation(format!("{key} must be a string")))?
        .to_ascii_lowercase();
    if !allowed.contains(&text.as_str()) {
        return Err(AppError::validation(format!(
            "{key} must be one of {}",
            allowed.join(", ")
        )));
    }
    Ok(text)
}

fn number(value: &Value, key: &str, min: u64, max: u64) -> AppResult<u64> {
    let number = value
        .as_u64()
        .ok_or_else(|| AppError::validation(format!("{key} must be a non-negative integer")))?;
    if number < min || number > max {
        return Err(AppError::validation(format!(
            "{key} must be between {min} and {max}"
        )));
    }
    Ok(number)
}

fn boolean(value: &Value, key: &str) -> AppResult<bool> {
    value
        .as_bool()
        .ok_or_else(|| AppError::validation(format!("{key} must be a boolean")))
}

fn secret(value: &Value, key: &str) -> AppResult<String> {
    let text = value
        .as_str()
        .ok_or_else(|| AppError::validation(format!("{key} must be a string")))?
        .trim();
    if text.chars().count() > 200 {
        return Err(AppError::validation(format!("{key} is too long")));
    }
    Ok(text.to_string())
}

fn download_dir(value: &Value) -> AppResult<String> {
    let text = value
        .as_str()
        .ok_or_else(|| AppError::validation("downloadDir must be a string"))?
        .trim();
    if text.is_empty() {
        return Ok(String::new());
    }
    if text.contains('\0') {
        return Err(AppError::validation(
            "downloadDir contains an invalid character",
        ));
    }
    let path = Path::new(text);
    if !path.is_absolute() {
        return Err(AppError::validation("downloadDir must be an absolute path"));
    }
    Ok(text.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Database {
        Database::open_in_memory().unwrap()
    }

    fn asset(id: &str, title: &str) -> Asset {
        let mut asset = Asset::new("test", id);
        asset.title = Some(title.into());
        asset.page_url = Some(format!("https://example.org/{id}"));
        asset
    }

    #[test]
    fn settings_round_trip_and_patch_validation() {
        let db = db();
        let mut settings = db.load_settings().unwrap();
        assert_eq!(settings.results_per_page, 24);
        settings.density = "dense".into();
        settings.download_dir = "C:\\Temp\\Stockcraft".into();
        db.save_settings(&settings).unwrap();
        let loaded = db.load_settings().unwrap();
        assert_eq!(loaded.density, "dense");
        assert_eq!(loaded.download_dir, "C:\\Temp\\Stockcraft");

        let known = vec!["openverse".to_string(), "nasa".to_string()];
        let mut value = serde_json::json!({"density": "cozy", "resultsPerPage": 48});
        apply_settings_patch(&mut settings, &value, &known).unwrap();
        assert_eq!(settings.density, "cozy");
        assert_eq!(settings.results_per_page, 48);
        value =
            serde_json::json!({"pixabayApiKey": "pixabay-test", "freepikApiKey": "freepik-test"});
        apply_settings_patch(&mut settings, &value, &known).unwrap();
        assert_eq!(settings.pixabay_api_key, "pixabay-test");
        assert_eq!(settings.freepik_api_key, "freepik-test");
        value = serde_json::json!({"theme": "neon"});
        assert!(apply_settings_patch(&mut settings, &value, &known).is_err());
        value = serde_json::json!({"nope": 1});
        assert!(apply_settings_patch(&mut settings, &value, &known).is_err());
        value = serde_json::json!({"enabledProviders": ["unknown"]});
        assert!(apply_settings_patch(&mut settings, &value, &known).is_err());
        value = serde_json::json!({"downloadDir": "relative/path"});
        assert!(apply_settings_patch(&mut settings, &value, &known).is_err());
    }

    #[test]
    fn favorites_and_library_round_trip() {
        let db = db();
        let a = asset("1", "Mountain lake");
        assert!(db.toggle_favorite(&a).unwrap());
        assert_eq!(db.favorite_ids().unwrap(), vec!["test:1".to_string()]);
        let favorites = db.list_favorites("", 20, 0).unwrap();
        assert_eq!(favorites.len(), 1);
        assert_eq!(favorites[0].title.as_deref(), Some("Mountain lake"));
        assert!(db.list_favorites("lake", 20, 0).unwrap().len() == 1);
        assert!(db.list_favorites("desert", 20, 0).unwrap().is_empty());
        assert!(!db.toggle_favorite(&a).unwrap());
        assert!(db.favorite_ids().unwrap().is_empty());
        assert_eq!(db.list_library("", None, 20, 0).unwrap().len(), 1);
    }

    #[test]
    fn collections_manage_membership_and_reject_duplicate_names() {
        let db = db();
        let collection = db.create_collection("Nature", "trees").unwrap();
        assert_eq!(collection.item_count, 0);
        assert!(db.create_collection("nature", "").is_err());
        assert!(db.create_collection("   ", "").is_err());
        let a = asset("2", "Tree");
        let collection = db.add_to_collection(collection.id, &a).unwrap();
        assert_eq!(collection.item_count, 1);
        // Idempotent membership.
        let collection = db.add_to_collection(collection.id, &a).unwrap();
        assert_eq!(collection.item_count, 1);
        assert_eq!(
            db.list_library("", Some(collection.id), 20, 0)
                .unwrap()
                .len(),
            1
        );
        let renamed = db
            .rename_collection(collection.id, "Forest", "more trees")
            .unwrap();
        assert_eq!(renamed.name, "Forest");
        let after = db.remove_from_collection(collection.id, &a.id).unwrap();
        assert_eq!(after.item_count, 0);
        db.delete_collection(collection.id).unwrap();
        assert!(db.get_collection(collection.id).is_err());
    }

    #[test]
    fn history_is_capped_and_deletable() {
        let db = db();
        let id = db.add_history("mountain", &["openverse".into()]).unwrap();
        db.update_history_count(id, 12).unwrap();
        let items = db.list_history(10).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].result_count, 12);
        db.delete_history_item(id).unwrap();
        assert!(db.list_history(10).unwrap().is_empty());
        db.add_history("sea", &[]).unwrap();
        db.clear_history().unwrap();
        assert!(db.list_history(10).unwrap().is_empty());
    }

    #[test]
    fn metadata_cache_honours_ttl() {
        let db = db();
        let page = crate::models::ProviderPage {
            assets: vec![asset("3", "Cached")],
            page: 1,
            has_more: false,
            total: Some(1),
        };
        db.put_metadata("k", "test", &page, 0).unwrap();
        assert!(
            db.get_metadata("k").unwrap().is_none(),
            "ttl 0 disables caching"
        );
        db.put_metadata("k", "test", &page, 10).unwrap();
        assert_eq!(db.get_metadata("k").unwrap().unwrap().assets.len(), 1);
        assert!(db.clear_metadata().unwrap() > 0);
    }

    #[test]
    fn downloads_store_and_clear() {
        let db = db();
        let a = asset("4", "Download me");
        let id = db
            .insert_download(&a, "C:\\Temp\\x.jpg", Some("attribution"))
            .unwrap();
        db.update_download(id, "done", 1234, None, None).unwrap();
        let record = db.get_download(id).unwrap();
        assert_eq!(record.status, "done");
        assert_eq!(record.bytes, 1234);
        assert_eq!(db.list_downloads(10).unwrap().len(), 1);
        assert_eq!(db.clear_downloads().unwrap(), 1);
        assert!(db.get_download(id).is_err());
    }
}
