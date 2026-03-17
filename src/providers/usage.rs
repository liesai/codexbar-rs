use clap::ValueEnum;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FetchSource {
    Api,
    Cli,
    Web,
    Mock,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderHealth {
    Ok,
    Degraded,
    MissingCredentials,
    Error,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum SourceMode {
    #[default]
    Auto,
    Api,
    Cli,
}

impl SourceMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Api => "api",
            Self::Cli => "cli",
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StatusRequest {
    pub source_mode: SourceMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UsageWindow {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub used: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remaining: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resets_at: Option<String>,
}

impl UsageWindow {
    pub fn new(used: Option<u64>, limit: Option<u64>) -> Self {
        let remaining = match (used, limit) {
            (Some(used), Some(limit)) if limit >= used => Some(limit - used),
            _ => None,
        };

        Self {
            used,
            limit,
            remaining,
            resets_at: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageSnapshot {
    pub provider: String,
    pub primary: UsageWindow,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secondary: Option<UsageWindow>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completion_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_tokens: Option<u32>,
    pub source: FetchSource,
    pub health: ProviderHealth,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    pub stale: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl UsageSnapshot {
    pub fn new(
        provider: impl Into<String>,
        primary: UsageWindow,
        source: FetchSource,
        health: ProviderHealth,
    ) -> Self {
        Self {
            provider: provider.into(),
            primary,
            secondary: None,
            prompt_tokens: None,
            completion_tokens: None,
            total_tokens: None,
            source,
            health,
            updated_at: None,
            stale: false,
            error: None,
        }
    }
}
