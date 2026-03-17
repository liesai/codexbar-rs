use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use std::cmp::max;
use std::process::Command;

use super::{
    FetchSource, Provider, ProviderConfig, ProviderHealth, SourceMode, StatusRequest,
    UsageSnapshot, UsageWindow,
};

const DEFAULT_BASE_URL: &str = "http://127.0.0.1:11434";
const STATUS_FALLBACK_USED: u64 = 12;
const STATUS_FALLBACK_LIMIT: u64 = 100;

pub struct OllamaProvider {
    client: Client,
    base_url: String,
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

        Ok(Self { client, base_url })
    }

    fn run_ollama_command(args: &[&str]) -> Result<String> {
        let output = Command::new("ollama")
            .args(args)
            .output()
            .with_context(|| format!("failed to execute ollama {}", args.join(" ")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let message = if stderr.is_empty() {
                format!(
                    "ollama {} exited with status {}",
                    args.join(" "),
                    output.status
                )
            } else {
                format!("ollama {} failed: {}", args.join(" "), stderr)
            };
            bail!(message);
        }

        String::from_utf8(output.stdout).context("ollama command output was not valid UTF-8")
    }

    fn parse_ollama_table_count(raw: &str) -> Result<u64> {
        let mut lines = raw.lines().map(str::trim).filter(|line| !line.is_empty());
        let Some(_header) = lines.next() else {
            bail!("ollama command returned no tabular output");
        };

        Ok(lines.count() as u64)
    }

    fn try_status_cli(&self) -> Result<UsageSnapshot> {
        let active_raw = Self::run_ollama_command(&["ps"])?;
        let active_models = Self::parse_ollama_table_count(&active_raw)?;

        let mut snapshot = UsageSnapshot::new(
            self.name(),
            UsageWindow::new(Some(active_models), None),
            FetchSource::Cli,
            ProviderHealth::Ok,
        );

        match Self::run_ollama_command(&["ls"]).and_then(|raw| Self::parse_ollama_table_count(&raw))
        {
            Ok(installed_models) => {
                snapshot.secondary = Some(UsageWindow::new(Some(installed_models), None));
            }
            Err(error) => {
                snapshot.health = ProviderHealth::Degraded;
                snapshot.error = Some(format!(
                    "collected active ollama models from CLI but failed to collect installed models: {error}"
                ));
            }
        }

        Ok(snapshot)
    }

    async fn status_cli(&self) -> Result<UsageSnapshot> {
        match self.try_status_cli() {
            Ok(snapshot) => Ok(snapshot),
            Err(error) => {
                let mut snapshot = UsageSnapshot::new(
                    self.name(),
                    UsageWindow::new(None, None),
                    FetchSource::Cli,
                    ProviderHealth::Degraded,
                );
                snapshot.stale = true;
                snapshot.error = Some(error.to_string());
                Ok(snapshot)
            }
        }
    }

    async fn status_auto(&self) -> Result<UsageSnapshot> {
        match self.try_status_cli() {
            Ok(snapshot) => Ok(snapshot),
            Err(_) => self.status_api().await,
        }
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

    async fn status(&self, request: StatusRequest) -> Result<UsageSnapshot> {
        match request.source_mode {
            SourceMode::Auto => self.status_auto().await,
            SourceMode::Api => self.status_api().await,
            SourceMode::Cli => self.status_cli().await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::OllamaProvider;

    #[test]
    fn parse_ollama_table_count_with_rows() {
        let raw = "\
NAME    ID    SIZE    PROCESSOR    CONTEXT    UNTIL
llama3.2:latest    abc123    2.0 GB    100% GPU    8192    4m
qwen2.5:latest    def456    6.0 GB    100% CPU    4096    1m
";

        let count = OllamaProvider::parse_ollama_table_count(raw).unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn parse_ollama_table_count_with_only_header() {
        let raw = "NAME    ID    SIZE    PROCESSOR    CONTEXT    UNTIL\n";
        let count = OllamaProvider::parse_ollama_table_count(raw).unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn parse_ollama_table_count_rejects_empty_output() {
        let error = OllamaProvider::parse_ollama_table_count("").unwrap_err();
        assert!(error.to_string().contains("no tabular output"));
    }
}
