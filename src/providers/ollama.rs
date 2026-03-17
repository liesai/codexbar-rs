use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::cmp::max;

use super::{
    FetchSource, Provider, ProviderConfig, ProviderHealth, ProviderRequest, ProviderResponse,
    SourceMode, StatusRequest, UsageSnapshot, UsageWindow,
};

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

    fn status_cli(&self) -> UsageSnapshot {
        let mut snapshot = UsageSnapshot::new(
            self.name(),
            UsageWindow::new(None, None),
            FetchSource::Cli,
            ProviderHealth::Degraded,
        );
        snapshot.stale = true;
        snapshot.error = Some("ollama CLI status strategy is not implemented".to_string());
        snapshot
    }

    async fn status_auto(&self) -> Result<UsageSnapshot> {
        self.status_api().await
    }

    async fn status_api(&self) -> Result<UsageSnapshot> {
        let default_window =
            UsageWindow::new(Some(STATUS_FALLBACK_USED), Some(STATUS_FALLBACK_LIMIT));
        let status_url = format!("{}/api/status", self.base_url);

        let response = match self.client.get(&status_url).send().await {
            Ok(resp) if resp.status().is_success() => resp,
            _ => {
                let mut snapshot = UsageSnapshot::new(
                    self.name(),
                    default_window,
                    FetchSource::Api,
                    ProviderHealth::Degraded,
                );
                snapshot.stale = true;
                snapshot.error =
                    Some("failed to fetch ollama status; using fallback values".to_string());
                return Ok(snapshot);
            }
        };

        let status_body = match response.json::<OllamaStatusResponse>().await {
            Ok(body) => body,
            Err(_) => {
                let mut snapshot = UsageSnapshot::new(
                    self.name(),
                    default_window,
                    FetchSource::Api,
                    ProviderHealth::Degraded,
                );
                snapshot.stale = true;
                snapshot.error =
                    Some("failed to decode ollama status; using fallback values".to_string());
                return Ok(snapshot);
            }
        };

        let usage = status_body.usage;
        let used = max(usage.current, usage.used);
        let used = if used > 0 { used } else { STATUS_FALLBACK_USED };
        let limit = if usage.limit > 0 {
            usage.limit
        } else {
            STATUS_FALLBACK_LIMIT
        };

        Ok(UsageSnapshot::new(
            self.name(),
            UsageWindow::new(Some(used), Some(limit)),
            FetchSource::Api,
            ProviderHealth::Ok,
        ))
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

    async fn status(&self, request: StatusRequest) -> Result<UsageSnapshot> {
        match request.source_mode {
            SourceMode::Auto => self.status_auto().await,
            SourceMode::Api => self.status_api().await,
            SourceMode::Cli => Ok(self.status_cli()),
        }
    }
}
