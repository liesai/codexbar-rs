mod mock;
mod ollama;
pub mod status;

use anyhow::{Result, bail};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

use self::mock::MockProvider;
use self::ollama::OllamaProvider;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderRequest {
    pub prompt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderResponse {
    pub provider: String,
    pub output: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderUsage {
    pub used: u64,
    pub limit: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub model: Option<String>,
    pub base_url: Option<String>,
}

#[async_trait]
pub trait Provider: Send + Sync {
    fn name(&self) -> &'static str;
    async fn generate(&self, request: ProviderRequest) -> Result<ProviderResponse>;
    async fn status(&self) -> Result<ProviderUsage>;
}

pub fn provider_names() -> &'static [&'static str] {
    &["mock", "ollama"]
}

pub fn create_provider(name: &str, config: ProviderConfig) -> Result<Box<dyn Provider>> {
    match normalize_provider_name(name).as_ref() {
        "mock" => Ok(Box::new(MockProvider::new(config))),
        "ollama" => Ok(Box::new(OllamaProvider::new(config)?)),
        _ => bail!("provider '{name}' is not available"),
    }
}

fn normalize_provider_name(name: &str) -> Cow<'_, str> {
    if name.bytes().all(|ch| !ch.is_ascii_uppercase()) {
        Cow::Borrowed(name)
    } else {
        Cow::Owned(name.to_ascii_lowercase())
    }
}
