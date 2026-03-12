//! On-disk cache for URI imports.
//!
//! Stores fetched content and metadata under `$XDG_CACHE_HOME/gnomon/uri`
//! (or `~/.cache/gnomon/uri` on Linux/macOS). Uses the document's
//! `refresh_interval` (iCalendar REFRESH-INTERVAL) to determine freshness.
//!
//! All filesystem errors are silently ignored — the cache is best-effort.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{fs, io};

use serde::{Deserialize, Serialize};

/// Default refresh interval when the document doesn't specify one: 1 day.
const DEFAULT_REFRESH_SECS: u64 = 86_400;

/// Hard ceiling: entries older than 30 days are always evicted.
const MAX_CACHE_AGE_SECS: u64 = 30 * 86_400;

#[derive(Serialize, Deserialize)]
struct CacheMeta {
    url: String,
    fetched_at: u64,
    content_type: String,
    refresh_interval_secs: Option<u64>,
}

/// Outcome of a cache lookup.
pub enum CacheLookup {
    /// Cached content is fresh.
    Hit { content: String, content_type: String },
    /// No cache entry or entry is stale.
    Miss,
}

// r[impl expr.import.cache.location]
/// Return the cache directory: `<xdg-cache>/gnomon/uri`.
fn cache_dir() -> Option<PathBuf> {
    use etcetera::{BaseStrategy, choose_base_strategy};
    let strategy = choose_base_strategy().ok()?;
    Some(strategy.cache_dir().join("gnomon").join("uri"))
}

// r[impl expr.import.cache.key]
/// Deterministic cache key for a URL (UUID v5 with URL namespace).
fn cache_key(url: &str) -> String {
    uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_URL, url.as_bytes()).to_string()
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// r[impl expr.import.cache.freshness]
// r[impl expr.import.cache.hit]
// r[impl expr.import.cache.best-effort]
/// Check if a cached entry exists and is fresh.
pub fn lookup(url: &str) -> CacheLookup {
    let dir = match cache_dir() {
        Some(d) => d,
        None => return CacheLookup::Miss,
    };
    let key = cache_key(url);
    let meta_path = dir.join(format!("{key}.meta.json"));
    let content_path = dir.join(format!("{key}.content"));

    let meta: CacheMeta = match std::fs::read_to_string(&meta_path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
    {
        Some(m) => m,
        None => return CacheLookup::Miss,
    };

    let max_age = meta.refresh_interval_secs.unwrap_or(DEFAULT_REFRESH_SECS);
    if now_secs().saturating_sub(meta.fetched_at) >= max_age {
        return CacheLookup::Miss;
    }

    match std::fs::read_to_string(&content_path) {
        Ok(content) => CacheLookup::Hit {
            content,
            content_type: meta.content_type,
        },
        Err(_) => CacheLookup::Miss,
    }
}

// r[impl expr.import.cache.content]
// r[impl expr.import.cache.miss]
/// Store fetched content and metadata in the cache.
///
/// `format_hint` is `"icalendar"`, `"jscalendar"`, or `"gnomon"`.
pub fn store(url: &str, content: &str, content_type: &str, format_hint: &str) {
    let dir = match cache_dir() {
        Some(d) => d,
        None => return,
    };
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }

    let refresh_interval_secs = if format_hint == "icalendar" {
        gnomon_import::extract_ical_refresh_interval_secs(content)
    } else {
        None
    };

    let meta = CacheMeta {
        url: url.to_string(),
        fetched_at: now_secs(),
        content_type: content_type.to_string(),
        refresh_interval_secs,
    };

    let key = cache_key(url);
    let content_path = dir.join(format!("{key}.content"));
    let meta_path = dir.join(format!("{key}.meta.json"));

    // Write content first; if it fails, don't write metadata.
    if fs::write(&content_path, content).is_err() {
        return;
    }
    if let Ok(json) = serde_json::to_string_pretty(&meta) {
        let _ = fs::write(&meta_path, json);
    }

    // r[impl expr.import.cache.evict]
    evict_expired(&dir);
}

/// Remove cache entries older than `MAX_CACHE_AGE_SECS`.
fn evict_expired(dir: &PathBuf) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    let now = now_secs();
    for entry in entries.flatten() {
        let path = entry.path();
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };
        if !name.ends_with(".meta.json") {
            continue;
        }
        let meta: CacheMeta = match fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
        {
            Some(m) => m,
            None => continue,
        };
        if now.saturating_sub(meta.fetched_at) >= MAX_CACHE_AGE_SECS {
            let stem = name.trim_end_matches(".meta.json");
            let _ = fs::remove_file(&path);
            let _ = fs::remove_file(dir.join(format!("{stem}.content")));
        }
    }
}

// r[impl cli.subcommand.clean]
/// Remove all cached URI import entries. Returns the number of entries removed.
pub fn clean() -> io::Result<usize> {
    let dir = match cache_dir() {
        Some(d) => d,
        None => return Ok(0),
    };
    let entries = match fs::read_dir(&dir) {
        Ok(e) => e,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(0),
        Err(e) => return Err(e),
    };
    let mut count = 0usize;
    for entry in entries.flatten() {
        let path = entry.path();
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };
        if name.ends_with(".meta.json") {
            count += 1;
        }
        fs::remove_file(&path)?;
    }
    Ok(count)
}
