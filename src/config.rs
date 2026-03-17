use crate::providers::SourceMode;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

const APP_DIR: &str = "codexbar";
const CONFIG_FILE: &str = "config.json";
const DEFAULT_CACHE_TTL_SECONDS: u64 = 30;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub status: StatusConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusConfig {
    pub default_source: SourceMode,
    pub cache_ttl_seconds: u64,
    pub cache_enabled: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            status: StatusConfig::default(),
        }
    }
}

impl Default for StatusConfig {
    fn default() -> Self {
        Self {
            default_source: SourceMode::Auto,
            cache_ttl_seconds: DEFAULT_CACHE_TTL_SECONDS,
            cache_enabled: true,
        }
    }
}

pub fn load_config() -> AppConfig {
    let path = config_path();
    let Ok(raw) = fs::read_to_string(path) else {
        return AppConfig::default();
    };

    serde_json::from_str::<AppConfig>(&raw).unwrap_or_default()
}

pub fn config_path() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        return PathBuf::from(xdg).join(APP_DIR).join(CONFIG_FILE);
    }

    match std::env::var("HOME") {
        Ok(home) => PathBuf::from(home)
            .join(".config")
            .join(APP_DIR)
            .join(CONFIG_FILE),
        Err(_) => PathBuf::from(CONFIG_FILE),
    }
}
