use crate::providers::{SourceMode, UsageSnapshot};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const APP_DIR: &str = "codexbar";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusCacheRecord {
    pub source_mode: SourceMode,
    pub provider_filter: Option<String>,
    pub cached_at_unix: u64,
    pub providers: BTreeMap<String, UsageSnapshot>,
}

impl StatusCacheRecord {
    pub fn new(
        source_mode: SourceMode,
        provider_filter: Option<String>,
        providers: BTreeMap<String, UsageSnapshot>,
    ) -> Self {
        Self {
            source_mode,
            provider_filter,
            cached_at_unix: now_unix_seconds(),
            providers,
        }
    }

    pub fn is_fresh(&self, ttl_seconds: u64) -> bool {
        let now = now_unix_seconds();
        now.saturating_sub(self.cached_at_unix) <= ttl_seconds
    }
}

pub fn load_status_cache(
    source_mode: SourceMode,
    provider_filter: Option<&str>,
) -> Result<Option<StatusCacheRecord>> {
    let path = cache_path(source_mode, provider_filter);
    if !path.exists() {
        return Ok(None);
    }

    let raw = fs::read_to_string(&path)
        .with_context(|| format!("failed to read status cache from {}", path.display()))?;
    let record = serde_json::from_str::<StatusCacheRecord>(&raw)
        .with_context(|| format!("failed to parse status cache from {}", path.display()))?;

    Ok(Some(record))
}

pub fn cache_path_for(source_mode: SourceMode, provider_filter: Option<&str>) -> PathBuf {
    cache_path(source_mode, provider_filter)
}

pub fn save_status_cache(record: &StatusCacheRecord) -> Result<()> {
    let path = cache_path(record.source_mode, record.provider_filter.as_deref());
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create cache directory {}", parent.display()))?;
    }

    let payload =
        serde_json::to_string_pretty(record).context("failed to serialize status cache payload")?;
    fs::write(&path, payload)
        .with_context(|| format!("failed to write status cache to {}", path.display()))?;

    Ok(())
}

pub fn stale_cached_providers(
    providers: &BTreeMap<String, UsageSnapshot>,
    reason: &str,
) -> BTreeMap<String, UsageSnapshot> {
    providers
        .iter()
        .map(|(name, snapshot)| {
            let mut snapshot = snapshot.clone();
            snapshot.stale = true;
            snapshot.error = Some(match &snapshot.error {
                Some(existing) => format!("{existing}; fallback cache used: {reason}"),
                None => format!("fallback cache used: {reason}"),
            });
            (name.clone(), snapshot)
        })
        .collect()
}

fn cache_path(source_mode: SourceMode, provider_filter: Option<&str>) -> PathBuf {
    let file_name = match provider_filter {
        Some(provider) => format!("status-cache-{}-{}.json", source_mode.as_str(), provider),
        None => format!("status-cache-{}.json", source_mode.as_str()),
    };

    if let Ok(xdg) = std::env::var("XDG_CACHE_HOME") {
        return PathBuf::from(xdg).join(APP_DIR).join(file_name);
    }

    match std::env::var("HOME") {
        Ok(home) => PathBuf::from(home)
            .join(".cache")
            .join(APP_DIR)
            .join(file_name),
        Err(_) => PathBuf::from(file_name),
    }
}

fn now_unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}
