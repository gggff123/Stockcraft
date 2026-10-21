//! Small shared helpers: URL safety, safe filenames, plain-text cleanup and colour naming.

use std::net::IpAddr;
use std::path::{Path, PathBuf};

use url::Url;

use crate::error::{AppError, AppResult};

/// Validate a URL that the app may open in a browser or fetch bytes from.
///
/// Rules: http(s) only, no embedded credentials, no local/private/link-local hosts, no unusual
/// ports. This blocks the classic SSRF targets (localhost services, cloud metadata endpoints) and
/// also stops a hostile provider payload from turning the thumbnail proxy into a LAN scanner.
pub fn validate_remote_url(raw: &str) -> AppResult<Url> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(AppError::validation("URL is empty"));
    }
    if trimmed.len() > 4096 {
        return Err(AppError::validation("URL is too long"));
    }
    let url = Url::parse(trimmed).map_err(|e| AppError::validation(format!("invalid URL: {e}")))?;
    match url.scheme() {
        "http" | "https" => {}
        other => {
            return Err(AppError::validation(format!(
                "unsupported URL scheme: {other}"
            )))
        }
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(AppError::validation(
            "URLs with embedded credentials are not allowed",
        ));
    }
    let host = url
        .host_str()
        .ok_or_else(|| AppError::validation("URL has no host"))?
        .trim_matches(['[', ']'])
        .to_ascii_lowercase();
    if host.is_empty() {
        return Err(AppError::validation("URL has no host"));
    }
    if let Some(port) = url.port() {
        if port != 80 && port != 443 {
            return Err(AppError::validation("only ports 80 and 443 are allowed"));
        }
    }
    if is_private_host(&host) {
        return Err(AppError::validation(
            "local and private network addresses are blocked",
        ));
    }
    Ok(url)
}

fn is_private_host(host: &str) -> bool {
    if host == "localhost"
        || host.ends_with(".localhost")
        || host.ends_with(".local")
        || host.ends_with(".internal")
        || host.ends_with(".home.arpa")
        || host == "metadata.google.internal"
        || host == "instance-data"
    {
        return true;
    }
    let bare = host.trim_start_matches('[').trim_end_matches(']');
    if let Ok(ip) = bare.parse::<IpAddr>() {
        return ip_is_private(ip);
    }
    false
}

fn ip_is_private(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_documentation()
                || v4.is_unspecified()
                || v4.is_multicast()
                || v4.octets()[0] == 0
                || (v4.octets()[0] == 100 && (64..128).contains(&v4.octets()[1]))
                || (v4.octets()[0] == 198 && (18..20).contains(&v4.octets()[1]))
                || v4.octets()[0] >= 240
        }
        IpAddr::V6(v6) => {
            v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || (v6.segments()[0] & 0xfe00) == 0xfc00
                || (v6.segments()[0] & 0xffc0) == 0xfe80
        }
    }
}

/// True when a string parses as a web URL that passes the public-host rules.
pub fn is_allowed_web_url(raw: &str) -> bool {
    validate_remote_url(raw).is_ok()
}

/// Normalise a URL for duplicate detection: lowercase scheme/host, no fragment, tracking
/// parameters removed, remaining query pairs sorted. Invalid URLs are returned trimmed/lowercased.
pub fn canonical_url(raw: &str) -> String {
    let Ok(mut url) = Url::parse(raw.trim()) else {
        return raw.trim().to_ascii_lowercase();
    };
    url.set_fragment(None);
    let mut pairs: Vec<(String, String)> = url
        .query_pairs()
        .filter(|(key, _)| {
            let key = key.to_ascii_lowercase();
            !(key.starts_with("utm_")
                || matches!(
                    key.as_str(),
                    "fbclid" | "gclid" | "mc_cid" | "mc_eid" | "igshid" | "ref_src" | "ref_url"
                ))
        })
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    pairs.sort();
    if pairs.is_empty() {
        url.set_query(None);
    } else {
        url.set_query(Some(
            &pairs
                .iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect::<Vec<_>>()
                .join("&"),
        ));
    }
    url.to_string()
}

/// Turn remote HTML-ish metadata into safe display text. No HTML may reach the DOM.
pub fn strip_html(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut in_tag = false;
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '<' => {
                in_tag = true;
                // Keep a space where an inline tag separated two words.
                if !out.ends_with(' ') {
                    out.push(' ');
                }
            }
            '>' if in_tag => in_tag = false,
            _ if in_tag => {}
            '&' => {
                let mut entity = String::new();
                while let Some(&next) = chars.peek() {
                    if next == ';' || entity.len() > 10 {
                        break;
                    }
                    entity.push(next);
                    chars.next();
                }
                if chars.peek() == Some(&';') {
                    chars.next();
                }
                out.push_str(&decode_entity(&entity));
            }
            _ => out.push(ch),
        }
    }
    collapse_whitespace(&out)
}

fn decode_entity(entity: &str) -> String {
    match entity.to_ascii_lowercase().as_str() {
        "amp" => "&".into(),
        "lt" => "<".into(),
        "gt" => ">".into(),
        "quot" => "\"".into(),
        "apos" => "'".into(),
        "nbsp" => " ".into(),
        "ndash" => "–".into(),
        "mdash" => "—".into(),
        "hellip" => "…".into(),
        other => {
            if let Some(hex) = other.strip_prefix("#x") {
                if let Ok(code) = u32::from_str_radix(hex, 16) {
                    if let Some(ch) = char::from_u32(code) {
                        return ch.to_string();
                    }
                }
            } else if let Some(dec) = other.strip_prefix('#') {
                if let Ok(code) = dec.parse::<u32>() {
                    if let Some(ch) = char::from_u32(code) {
                        return ch.to_string();
                    }
                }
            }
            String::new()
        }
    }
}

fn collapse_whitespace(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut space = false;
    for ch in input.chars() {
        if ch.is_whitespace() {
            space = true;
            continue;
        }
        if space && !out.is_empty() {
            out.push(' ');
        }
        space = false;
        out.push(ch);
    }
    out.trim().to_string()
}

/// A filesystem-safe filename stem. Never returns an empty string.
pub fn safe_stem(input: &str) -> String {
    let mut cleaned: String = input
        .chars()
        .map(|ch| {
            if ch.is_control() || matches!(ch, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*')
            {
                ' '
            } else {
                ch
            }
        })
        .collect();
    cleaned = collapse_whitespace(&cleaned);
    cleaned = cleaned.trim_matches(['.', ' ']).to_string();
    if cleaned.is_empty() {
        cleaned = "stockcraft-image".into();
    }
    let upper = cleaned.to_ascii_uppercase();
    let reserved = matches!(
        upper.as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    );
    if reserved {
        cleaned = format!("{cleaned}-image");
    }
    truncate_chars(&cleaned, 110)
}

/// Like `safe_stem`, but removes a trailing image extension so callers can append the extension
/// chosen from the MIME type without producing names such as `photo.jpg.jpg`.
pub fn safe_filename_stem(input: &str) -> String {
    let stem = safe_stem(input);
    let lower = stem.to_ascii_lowercase();
    for extension in [
        "jpeg", "jpg", "png", "webp", "gif", "avif", "svg", "tif", "tiff", "bmp",
    ] {
        if lower == extension {
            return "stockcraft-image".into();
        }
        let suffix = format!(".{extension}");
        if lower.ends_with(&suffix) {
            let base = stem[..stem.len() - suffix.len()].trim_end_matches(['.', ' ']);
            if !base.is_empty() {
                return safe_stem(base);
            }
        }
    }
    stem
}

fn truncate_chars(input: &str, max: usize) -> String {
    if input.chars().count() <= max {
        return input.to_string();
    }
    input
        .chars()
        .take(max)
        .collect::<String>()
        .trim_end()
        .to_string()
}

/// Pick an extension from a MIME type, falling back to the URL path, then `jpg`.
pub fn extension_for(mime: Option<&str>, url: &str) -> String {
    if let Some(mime) = mime {
        let mime = mime.to_ascii_lowercase();
        let ext = match mime.split(';').next().unwrap_or("").trim() {
            "image/jpeg" | "image/jpg" | "image/pjpeg" => Some("jpg"),
            "image/png" => Some("png"),
            "image/webp" => Some("webp"),
            "image/gif" => Some("gif"),
            "image/avif" => Some("avif"),
            "image/svg+xml" => Some("svg"),
            "image/tiff" => Some("tif"),
            "image/bmp" => Some("bmp"),
            _ => None,
        };
        if let Some(ext) = ext {
            return ext.to_string();
        }
    }
    if let Ok(url) = Url::parse(url) {
        let candidate = url
            .path_segments()
            .and_then(|mut segments| segments.next_back())
            .unwrap_or_default();
        if let Some((_, ext)) = candidate.rsplit_once('.') {
            let ext = ext.to_ascii_lowercase();
            if matches!(
                ext.as_str(),
                "jpg" | "jpeg" | "png" | "webp" | "gif" | "avif" | "svg" | "tif" | "tiff" | "bmp"
            ) {
                return if ext == "jpeg" { "jpg".into() } else { ext };
            }
        }
    }
    "jpg".into()
}

/// Return `path` if free, otherwise add a numbered suffix before the extension.
pub fn unique_path(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "image".into());
    let ext = path.extension().map(|s| s.to_string_lossy().to_string());
    for index in 1..=9999 {
        let name = match &ext {
            Some(ext) => format!("{stem} ({index}).{ext}"),
            None => format!("{stem} ({index})"),
        };
        let candidate = parent.join(name);
        if !candidate.exists() {
            return candidate;
        }
    }
    parent.join(format!("{stem}-{}", uuid::Uuid::new_v4()))
}

/// Is `path` inside `root` (after lexical normalisation)? Used by `open_path`.
pub fn path_within(path: &Path, root: &Path) -> bool {
    let Ok(path) = normalize_lexically(path) else {
        return false;
    };
    let Ok(root) = normalize_lexically(root) else {
        return false;
    };
    path.starts_with(root)
}

fn normalize_lexically(path: &Path) -> AppResult<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let mut out = PathBuf::new();
    for component in absolute.components() {
        use std::path::Component::*;
        match component {
            Prefix(prefix) => out.push(prefix.as_os_str()),
            RootDir => out.push("\\"),
            CurDir => {}
            ParentDir => {
                out.pop();
            }
            Normal(part) => out.push(part),
        }
    }
    Ok(out)
}

/// Map a provider colour (hex) onto the named palette the UI filters by.
pub fn nearest_color_name(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() || value.eq_ignore_ascii_case("any") {
        return None;
    }
    let palette: [(&str, (u8, u8, u8)); 12] = [
        ("red", (0xd6, 0x45, 0x45)),
        ("orange", (0xe0, 0x8a, 0x2e)),
        ("yellow", (0xe0, 0xc5, 0x3a)),
        ("green", (0x3f, 0x9a, 0x5a)),
        ("teal", (0x2f, 0x9a, 0x9a)),
        ("blue", (0x3a, 0x6f, 0xd8)),
        ("purple", (0x7a, 0x4f, 0xd8)),
        ("pink", (0xd8, 0x56, 0x9a)),
        ("brown", (0x8a, 0x62, 0x42)),
        ("grey", (0x8b, 0x93, 0xa3)),
        ("black", (0x1b, 0x21, 0x30)),
        ("white", (0xf2, 0xf4, 0xf8)),
    ];
    let lower = value.to_ascii_lowercase();
    if let Some((name, _)) = palette.iter().find(|(name, _)| *name == lower) {
        return Some((*name).to_string());
    }
    let hex = lower.trim_start_matches('#');
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    let mut best = palette[0];
    let mut best_distance = i32::MAX;
    for (name, (pr, pg, pb)) in palette {
        let distance = (r as i32 - pr as i32).pow(2)
            + (g as i32 - pg as i32).pow(2)
            + (b as i32 - pb as i32).pow(2);
        if distance < best_distance {
            best_distance = distance;
            best = (name, (pr, pg, pb));
        }
    }
    Some(best.0.to_string())
}

/// Parse a provider timestamp into the ISO-8601 form the UI expects, when possible.
pub fn iso_timestamp(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(raw) {
        return Some(
            dt.with_timezone(&chrono::Utc)
                .to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        );
    }
    for format in ["%Y-%m-%d %H:%M:%S", "%Y-%m-%dT%H:%M:%S", "%Y-%m-%d"] {
        if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(raw, format) {
            return Some(
                dt.and_utc()
                    .to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            );
        }
        if let Ok(date) = chrono::NaiveDate::parse_from_str(raw, format) {
            return Some(
                date.and_hms_opt(0, 0, 0)?
                    .and_utc()
                    .to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            );
        }
    }
    None
}

pub fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

/// Local post-filter mirrored from the frontend so providers that ignore a parameter never leak
/// assets that contradict an explicit user filter.
pub fn matches_filters(
    asset: &crate::models::Asset,
    filters: &crate::models::AssetFilters,
) -> bool {
    use crate::models::AssetFilters;
    let AssetFilters {
        orientation,
        min_width,
        min_height,
        media_type,
        license,
        color,
        ..
    } = filters;

    if orientation != "any" {
        let (w, h) = match (asset.width, asset.height) {
            (Some(w), Some(h)) if h > 0 => (w, h),
            _ => return false,
        };
        let ratio = w as f32 / h as f32;
        let actual = if ratio > 1.08 {
            "landscape"
        } else if ratio < 0.92 {
            "portrait"
        } else {
            "square"
        };
        if actual != orientation {
            return false;
        }
    }
    if *min_width > 0 && asset.width.unwrap_or(0) < *min_width {
        return false;
    }
    if *min_height > 0 && asset.height.unwrap_or(0) < *min_height {
        return false;
    }
    if media_type != "any" {
        let actual = asset.media_type.to_ascii_lowercase();
        if media_type == "diagram" {
            if !(actual.contains("diagram")
                || actual.contains("map")
                || actual.contains("chart")
                || actual.contains("drawing"))
            {
                return false;
            }
        } else if &actual != media_type {
            return false;
        }
    }
    if license != "any" && !license_matches(license, asset) {
        return false;
    }
    if color != "any" {
        match asset.dominant_color.as_deref() {
            Some(actual) if actual == color => {}
            _ => return false,
        }
    }
    true
}

fn license_matches(filter: &str, asset: &crate::models::Asset) -> bool {
    let haystack = format!(
        "{} {}",
        asset.license_name.as_deref().unwrap_or(""),
        asset.license_url.as_deref().unwrap_or("")
    )
    .to_ascii_lowercase();
    if haystack.trim().is_empty() {
        return filter == "other";
    }
    let is_cc0 = haystack.contains("cc0")
        || haystack.contains("public domain")
        || haystack.contains("pdm")
        || haystack.contains("no known copyright");
    let is_by_sa = haystack.contains("by-sa")
        || haystack.contains("sharealike")
        || haystack.contains("share-alike");
    let is_by = !is_by_sa
        && (haystack.contains("by ")
            || haystack.contains("by/")
            || haystack.contains("attribution")
            || haystack.contains("cc by"));
    match filter {
        "cc0" => is_cc0,
        "by" => is_by && !haystack.contains("by-nc") && !haystack.contains("by-nd"),
        "by-sa" => is_by_sa,
        "other" => !(is_cc0 || is_by || is_by_sa),
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Asset;

    #[test]
    fn blocks_local_and_private_urls() {
        for url in [
            "http://localhost:1420/",
            "http://127.0.0.1/x.jpg",
            "http://192.168.1.5/x.jpg",
            "http://169.254.169.254/latest/meta-data",
            "http://[::1]/x.jpg",
            "http://10.0.0.2/x.jpg",
            "http://metadata.google.internal/x",
            "http://example.com:8080/x.jpg",
            "file:///C:/Windows/win.ini",
            "http://user:pass@example.com/x.jpg",
        ] {
            assert!(
                validate_remote_url(url).is_err(),
                "{url} should be rejected"
            );
        }
        for url in [
            "https://api.openverse.org/v1/images/",
            "https://upload.wikimedia.org/wikipedia/commons/a/b/c.jpg",
            "http://images-assets.nasa.gov/image/x/x~orig.jpg",
        ] {
            assert!(validate_remote_url(url).is_ok(), "{url} should be accepted");
        }
    }

    #[test]
    fn canonical_url_removes_tracking_and_sorts_query() {
        assert_eq!(
            canonical_url("HTTPS://Example.COM/a.jpg?b=2&utm_source=x&a=1#frag"),
            "https://example.com/a.jpg?a=1&b=2"
        );
    }

    #[test]
    fn strips_html_and_decodes_entities() {
        assert_eq!(
            strip_html("<a href=\"x\">Claude&nbsp;Monet</a> &amp; <b>friends</b>"),
            "Claude Monet & friends"
        );
        assert_eq!(strip_html("line<br/>break"), "line break");
    }

    #[test]
    fn safe_stem_handles_windows_reserved_names_and_traversal() {
        assert_eq!(safe_stem("..\\..\\evil"), "evil");
        assert_eq!(safe_stem("CON"), "CON-image");
        assert_eq!(safe_stem("a/b:c*?"), "a b c");
        assert_eq!(safe_stem("   "), "stockcraft-image");
        assert!(safe_stem(&"x".repeat(300)).chars().count() <= 110);
    }

    #[test]
    fn filename_stem_does_not_duplicate_image_extensions() {
        assert_eq!(safe_filename_stem("photo.JPG"), "photo");
        assert_eq!(safe_filename_stem("archive.tar.png"), "archive.tar");
        assert_eq!(safe_filename_stem("photo.jpg.jpeg"), "photo.jpg");
        assert_eq!(safe_filename_stem(".jpg"), "stockcraft-image");
        assert_eq!(safe_filename_stem("CON.jpg"), "CON-image");
        assert_eq!(safe_filename_stem("holiday.2026"), "holiday.2026");
    }

    #[test]
    fn extensions_come_from_mime_then_url() {
        assert_eq!(extension_for(Some("image/png"), "https://x/y"), "png");
        assert_eq!(extension_for(None, "https://x/y.webp?x=1"), "webp");
        assert_eq!(
            extension_for(Some("application/octet-stream"), "https://x/y.jpeg"),
            "jpg"
        );
        assert_eq!(extension_for(None, "https://x/y"), "jpg");
    }

    #[test]
    fn colour_names_match_the_ui_palette() {
        assert_eq!(nearest_color_name("#3a6fd8").as_deref(), Some("blue"));
        assert_eq!(nearest_color_name("#000000").as_deref(), Some("black"));
        assert_eq!(nearest_color_name("pink").as_deref(), Some("pink"));
        assert_eq!(nearest_color_name("not-a-colour"), None);
    }

    #[test]
    fn local_filters_reject_unsupported_assets() {
        let mut asset = Asset::new("test", "1");
        asset.width = Some(1000);
        asset.height = Some(500);
        asset.media_type = "photo".into();
        asset.license_name = Some("CC0 1.0".into());
        asset.dominant_color = Some("blue".into());
        let mut filters = crate::models::AssetFilters::default();
        assert!(matches_filters(&asset, &filters));
        filters.orientation = "portrait".into();
        assert!(!matches_filters(&asset, &filters));
        filters.orientation = "any".into();
        filters.min_width = 2000;
        assert!(!matches_filters(&asset, &filters));
        filters.min_width = 0;
        filters.color = "red".into();
        assert!(!matches_filters(&asset, &filters));
        filters.color = "blue".into();
        filters.license = "cc0".into();
        assert!(matches_filters(&asset, &filters));
    }

    #[test]
    fn path_containment_is_lexical() {
        let root = Path::new("C:\\Users\\x\\Pictures\\Stockcraft");
        assert!(path_within(
            Path::new("C:\\Users\\x\\Pictures\\Stockcraft\\a.jpg"),
            root
        ));
        assert!(!path_within(
            Path::new("C:\\Users\\x\\Pictures\\Stockcraft\\..\\..\\secret.txt"),
            root
        ));
    }
}
