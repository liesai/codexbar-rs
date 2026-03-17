use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use std::time::{SystemTime, UNIX_EPOCH};

use super::{
    FetchSource, Provider, ProviderConfig, ProviderHealth, SourceMode, StatusRequest,
    UsageSnapshot, UsageWindow,
};

const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";
const DEFAULT_MODEL: &str = "gpt-4o-mini";
const USAGE_WINDOW_SECONDS: u64 = 24 * 60 * 60;

pub struct OpenAiProvider {
    client: Client,
    base_url: String,
    api_key: Option<String>,
    model: Option<String>,
}

impl OpenAiProvider {
    pub fn new(config: ProviderConfig) -> Result<Self> {
        let client = Client::builder()
            .build()
            .context("failed to build HTTP client for openai provider")?;

        let base_url = config
            .base_url
            .or_else(|| std::env::var("OPENAI_BASE_URL").ok())
            .unwrap_or_else(|| DEFAULT_BASE_URL.to_string())
            .trim_end_matches('/')
            .to_string();

        let model = config
            .model
            .or_else(|| std::env::var("OPENAI_MODEL").ok())
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .or_else(|| Some(DEFAULT_MODEL.to_string()));

        let api_key = std::env::var("OPENAI_ADMIN_KEY")
            .ok()
            .or_else(|| std::env::var("OPENAI_API_KEY").ok())
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());

        Ok(Self {
            client,
            base_url,
            api_key,
            model,
        })
    }

    fn endpoint(&self) -> String {
        format!("{}/organization/usage/completions", self.base_url)
    }

    fn status_cli(&self) -> UsageSnapshot {
        let mut snapshot = UsageSnapshot::new(
            self.name(),
            UsageWindow::new(None, None),
            FetchSource::Cli,
            ProviderHealth::Degraded,
        );
        snapshot.stale = true;
        snapshot.error = Some("openai CLI status strategy is not implemented".to_string());
        snapshot
    }

    async fn status_auto(&self) -> Result<UsageSnapshot> {
        self.status_api().await
    }

    async fn status_api(&self) -> Result<UsageSnapshot> {
        if self.api_key.is_none() {
            let mut snapshot = UsageSnapshot::new(
                self.name(),
                UsageWindow::new(None, None),
                FetchSource::Unknown,
                ProviderHealth::MissingCredentials,
            );
            snapshot.stale = true;
            snapshot.error = Some("OPENAI_ADMIN_KEY or OPENAI_API_KEY is not set".to_string());
            return Ok(snapshot);
        }

        match self.fetch_usage_bucket().await {
            Ok(bucket) => Ok(self.snapshot_from_bucket(bucket)),
            Err(error) => {
                let mut snapshot = UsageSnapshot::new(
                    self.name(),
                    UsageWindow::new(None, None),
                    FetchSource::Api,
                    ProviderHealth::Error,
                );
                snapshot.stale = true;
                snapshot.error = Some(error.to_string());
                Ok(snapshot)
            }
        }
    }

    async fn fetch_usage_bucket(&self) -> Result<OpenAiUsageBucket> {
        let api_key = match self.api_key.as_deref() {
            Some(value) => value,
            None => bail!("OPENAI_ADMIN_KEY or OPENAI_API_KEY is not set"),
        };

        let now = now_unix_seconds();
        let start_time = now.saturating_sub(USAGE_WINDOW_SECONDS);

        let mut request = self
            .client
            .get(self.endpoint())
            .bearer_auth(api_key)
            .query(&[
                ("start_time", start_time.to_string()),
                ("end_time", now.to_string()),
                ("bucket_width", "1d".to_string()),
            ]);

        if let Some(model) = &self.model {
            request = request.query(&[("models[]", model.as_str())]);
        }

        let response = request
            .send()
            .await
            .context("failed to send request to OpenAI usage API")?;

        let status = response.status();
        if !status.is_success() {
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| String::from("<failed to read error body>"));

            if status.as_u16() == 403 {
                bail!(
                    "OpenAI usage API rejected the key (403). An admin-scoped key is required for organization usage endpoints: {}",
                    body
                );
            }

            bail!(
                "OpenAI usage API request failed with status {}: {}",
                status,
                body
            );
        }

        let payload = response
            .json::<OpenAiUsageResponse>()
            .await
            .context("failed to decode OpenAI usage response JSON")?;

        payload
            .data
            .into_iter()
            .max_by_key(|bucket| bucket.end_time)
            .ok_or_else(|| anyhow::anyhow!("OpenAI usage API returned no usage buckets"))
    }

    fn snapshot_from_bucket(&self, bucket: OpenAiUsageBucket) -> UsageSnapshot {
        let prompt_tokens = bucket
            .results
            .iter()
            .filter_map(|result| result.input_tokens)
            .sum::<u64>();
        let completion_tokens = bucket
            .results
            .iter()
            .filter_map(|result| result.output_tokens)
            .sum::<u64>();
        let total_requests = bucket
            .results
            .iter()
            .filter_map(|result| result.num_model_requests)
            .sum::<u64>();
        let total_tokens = prompt_tokens.checked_add(completion_tokens);

        let mut snapshot = UsageSnapshot::new(
            self.name(),
            UsageWindow::new(total_tokens, None),
            FetchSource::Api,
            ProviderHealth::Ok,
        );

        snapshot.prompt_tokens = u32::try_from(prompt_tokens).ok();
        snapshot.completion_tokens = u32::try_from(completion_tokens).ok();
        snapshot.total_tokens = total_tokens.and_then(|value| u32::try_from(value).ok());
        snapshot.secondary = Some(UsageWindow::new(Some(total_requests), None));
        snapshot.updated_at = Some(bucket.end_time.to_string());

        snapshot
    }
}

#[derive(Debug, Deserialize)]
struct OpenAiUsageResponse {
    #[serde(default)]
    data: Vec<OpenAiUsageBucket>,
}

#[derive(Debug, Deserialize)]
struct OpenAiUsageBucket {
    end_time: u64,
    #[serde(default)]
    results: Vec<OpenAiUsageResult>,
}

#[derive(Debug, Deserialize)]
struct OpenAiUsageResult {
    #[serde(default)]
    input_tokens: Option<u64>,
    #[serde(default)]
    output_tokens: Option<u64>,
    #[serde(default)]
    num_model_requests: Option<u64>,
}

fn now_unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

#[async_trait]
impl Provider for OpenAiProvider {
    fn name(&self) -> &'static str {
        "openai"
    }

    async fn status(&self, request: StatusRequest) -> Result<UsageSnapshot> {
        match request.source_mode {
            SourceMode::Auto => self.status_auto().await,
            SourceMode::Api => self.status_api().await,
            SourceMode::Cli => Ok(self.status_cli()),
        }
    }
}
