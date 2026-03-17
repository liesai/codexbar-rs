use anyhow::Result;
use async_trait::async_trait;

use super::{
    FetchSource, Provider, ProviderConfig, ProviderHealth, SourceMode, StatusRequest,
    UsageSnapshot, UsageWindow,
};

const DEFAULT_USAGE_USED: u64 = 5;
const DEFAULT_USAGE_LIMIT: u64 = 100;

pub struct MockProvider;

impl MockProvider {
    pub fn new(_config: ProviderConfig) -> Self {
        Self
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

    async fn status(&self, request: StatusRequest) -> Result<UsageSnapshot> {
        let snapshot = match request.source_mode {
            SourceMode::Auto => self.status_auto(),
            SourceMode::Api => self.status_api(),
            SourceMode::Cli => self.status_cli(),
        };

        Ok(snapshot)
    }
}
