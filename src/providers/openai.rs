use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use super::{
    FetchSource, Provider, ProviderConfig, ProviderHealth, SourceMode, StatusRequest,
    UsageSnapshot, UsageWindow,
};

const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";
const DEFAULT_MODEL: &str = "gpt-4o-mini";
const STATUS_PROBE_PROMPT: &str = "status";

pub struct OpenAiProvider {
    client: Client,
    base_url: String,
    api_key: Option<String>,
    model: String,
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
            .unwrap_or_else(|| DEFAULT_MODEL.to_string());

        let api_key = std::env::var("OPENAI_API_KEY")
            .ok()
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
        format!("{}/chat/completions", self.base_url)
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
                UsageWindow::new(Some(0), None),
                FetchSource::Unknown,
                ProviderHealth::MissingCredentials,
            );
            snapshot.stale = true;
            snapshot.error = Some("OPENAI_API_KEY is not set".to_string());
            return Ok(snapshot);
        }

        let response = match self.chat_completion(STATUS_PROBE_PROMPT).await {
            Ok(value) => value,
            Err(_) => {
                let mut snapshot = UsageSnapshot::new(
                    self.name(),
                    UsageWindow::new(Some(0), None),
                    FetchSource::Api,
                    ProviderHealth::Error,
                );
                snapshot.stale = true;
                snapshot.error = Some("failed to fetch usage probe from OpenAI".to_string());
                return Ok(snapshot);
            }
        };

        let usage = response.usage;
        let total_tokens = resolve_total_tokens(&usage);
        let mut snapshot = UsageSnapshot::new(
            self.name(),
            UsageWindow::new(total_tokens.map(u64::from), None),
            FetchSource::Api,
            ProviderHealth::Ok,
        );
        snapshot.prompt_tokens = usage.prompt_tokens;
        snapshot.completion_tokens = usage.completion_tokens;
        snapshot.total_tokens = total_tokens;

        Ok(snapshot)
    }

    async fn chat_completion(&self, prompt: &str) -> Result<OpenAiChatCompletionResponse> {
        let api_key = match self.api_key.as_deref() {
            Some(value) => value,
            None => bail!("OPENAI_API_KEY is not set"),
        };

        let payload = OpenAiChatCompletionRequest {
            model: &self.model,
            messages: [OpenAiChatMessage {
                role: "user",
                content: prompt,
            }],
        };

        let response = self
            .client
            .post(self.endpoint())
            .bearer_auth(api_key)
            .json(&payload)
            .send()
            .await
            .context("failed to send request to openai")?;

        let status = response.status();
        if !status.is_success() {
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| String::from("<failed to read error body>"));
            bail!("openai request failed with status {}: {}", status, body);
        }

        response
            .json::<OpenAiChatCompletionResponse>()
            .await
            .context("failed to decode openai response JSON")
    }
}

#[derive(Debug, Serialize)]
struct OpenAiChatCompletionRequest<'a> {
    model: &'a str,
    messages: [OpenAiChatMessage<'a>; 1],
}

#[derive(Debug, Serialize)]
struct OpenAiChatMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Debug, Deserialize)]
struct OpenAiChatCompletionResponse {
    #[serde(default)]
    usage: OpenAiUsage,
}

#[derive(Debug, Deserialize, Default)]
struct OpenAiUsage {
    #[serde(default)]
    prompt_tokens: Option<u32>,
    #[serde(default)]
    completion_tokens: Option<u32>,
    #[serde(default)]
    total_tokens: Option<u32>,
}

fn resolve_total_tokens(usage: &OpenAiUsage) -> Option<u32> {
    usage.total_tokens.or_else(|| {
        usage
            .prompt_tokens
            .zip(usage.completion_tokens)
            .and_then(|(prompt, completion)| prompt.checked_add(completion))
    })
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
