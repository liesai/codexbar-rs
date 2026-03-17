use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::cmp::max;

use super::{Provider, ProviderConfig, ProviderRequest, ProviderResponse, ProviderUsage};

const DEFAULT_BASE_URL: &str = "http://127.0.0.1:11434";
const DEFAULT_MODEL: &str = "llama3.2";
const STATUS_FALLBACK_USED: u64 = 12;
const STATUS_FALLBACK_LIMIT: u64 = 100;

pub struct OllamaProvider {
    client: Client,
    base_url: String,
    model: String,
}

impl OllamaProvider {
    pub fn new(config: ProviderConfig) -> Result<Self> {
        let client = Client::builder()
            .build()
            .context("failed to build HTTP client for ollama provider")?;

        let base_url = config
            .base_url
            .or_else(|| std::env::var("OLLAMA_BASE_URL").ok())
            .unwrap_or_else(|| DEFAULT_BASE_URL.to_string())
            .trim_end_matches('/')
            .to_string();

        let model = config
            .model
            .or_else(|| std::env::var("OLLAMA_MODEL").ok())
            .unwrap_or_else(|| DEFAULT_MODEL.to_string());

        Ok(Self {
            client,
            base_url,
            model,
        })
    }

    fn endpoint(&self) -> String {
        format!("{}/api/generate", self.base_url)
    }
}

#[derive(Debug, Serialize)]
struct OllamaGenerateRequest<'a> {
    model: &'a str,
    prompt: &'a str,
    stream: bool,
}

#[derive(Debug, Deserialize)]
struct OllamaGenerateResponse {
    #[serde(default)]
    response: String,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct OllamaStatusResponse {
    #[serde(default)]
    usage: OllamaUsageInfo,
}

#[derive(Debug, Deserialize, Default)]
struct OllamaUsageInfo {
    #[serde(default)]
    current: u64,
    #[serde(default)]
    used: u64,
    #[serde(default)]
    limit: u64,
}

#[async_trait]
impl Provider for OllamaProvider {
    fn name(&self) -> &'static str {
        "ollama"
    }

    async fn generate(&self, request: ProviderRequest) -> Result<ProviderResponse> {
        let payload = OllamaGenerateRequest {
            model: &self.model,
            prompt: &request.prompt,
            stream: false,
        };

        let response = self
            .client
            .post(self.endpoint())
            .json(&payload)
            .send()
            .await
            .context("failed to send request to ollama")?;

        let status = response.status();
        if !status.is_success() {
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| String::from("<failed to read error body>"));
            bail!("ollama request failed with status {}: {}", status, body);
        }

        let body: OllamaGenerateResponse = response
            .json()
            .await
            .context("failed to decode ollama response JSON")?;

        if let Some(message) = body.error {
            bail!("ollama API returned an error: {message}");
        }

        Ok(ProviderResponse {
            provider: self.name().to_string(),
            output: body.response,
        })
    }

    async fn status(&self) -> Result<ProviderUsage> {
        let default_usage = ProviderUsage {
            used: STATUS_FALLBACK_USED,
            limit: STATUS_FALLBACK_LIMIT,
        };
        let status_url = format!("{}/api/status", self.base_url);

        let response = match self.client.get(&status_url).send().await {
            Ok(resp) if resp.status().is_success() => resp,
            _ => return Ok(default_usage),
        };

        let status_body = match response.json::<OllamaStatusResponse>().await {
            Ok(body) => body,
            Err(_) => return Ok(default_usage),
        };

        let usage = status_body.usage;
        let used = max(usage.current, usage.used);
        let used = if used > 0 { used } else { default_usage.used };
        let limit = if usage.limit > 0 {
            usage.limit
        } else {
            default_usage.limit
        };

        Ok(ProviderUsage { used, limit })
    }
}
