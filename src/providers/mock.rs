use anyhow::Result;
use async_trait::async_trait;
use tokio::time::{Duration, sleep};

use super::{
    FetchSource, Provider, ProviderConfig, ProviderHealth, ProviderRequest, ProviderResponse,
    SourceMode, StatusRequest, UsageSnapshot, UsageWindow,
};

const DEFAULT_MODEL: &str = "mock-v1";
const DEFAULT_USAGE_USED: u64 = 5;
const DEFAULT_USAGE_LIMIT: u64 = 100;

pub struct MockProvider {
    model: String,
}

impl MockProvider {
    pub fn new(config: ProviderConfig) -> Self {
        Self {
            model: config.model.unwrap_or_else(|| DEFAULT_MODEL.to_string()),
        }
    }

    fn status_auto(&self) -> UsageSnapshot {
        self.status_mock()
    }

    fn status_api(&self) -> UsageSnapshot {
        self.status_mock()
    }

    fn status_cli(&self) -> UsageSnapshot {
        self.status_mock()
    }

    fn status_mock(&self) -> UsageSnapshot {
        UsageSnapshot::new(
            self.name(),
            UsageWindow::new(Some(DEFAULT_USAGE_USED), Some(DEFAULT_USAGE_LIMIT)),
            FetchSource::Mock,
            ProviderHealth::Ok,
        )
    }
}

#[async_trait]
impl Provider for MockProvider {
    fn name(&self) -> &'static str {
        "mock"
    }

    async fn generate(&self, request: ProviderRequest) -> Result<ProviderResponse> {
        // Simulate network/model latency to behave similarly to real provider calls.
        sleep(Duration::from_millis(30)).await;

        let token_count = request.prompt.split_whitespace().count();
        let output = format!(
            "[model={}] tokens={} echo={}",
            self.model, token_count, request.prompt
        );

        Ok(ProviderResponse {
            provider: self.name().to_string(),
            output,
        })
    }

    async fn status(&self, request: StatusRequest) -> Result<UsageSnapshot> {
        let snapshot = match request.source_mode {
            SourceMode::Auto => self.status_auto(),
            SourceMode::Api => self.status_api(),
            SourceMode::Cli => self.status_cli(),
        };

        Ok(snapshot)
    }
}
