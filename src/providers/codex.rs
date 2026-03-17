use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

use super::{
    FetchSource, Provider, ProviderConfig, ProviderHealth, SourceMode, StatusRequest,
    UsageSnapshot, UsageWindow,
};

pub struct CodexProvider {
    auth_path: PathBuf,
}

impl CodexProvider {
    pub fn new(_config: ProviderConfig) -> Self {
        Self {
            auth_path: resolve_auth_path(),
        }
    }

    fn status_api(&self) -> UsageSnapshot {
        let mut snapshot = UsageSnapshot::new(
            self.name(),
            UsageWindow::new(None, None),
            FetchSource::Api,
            ProviderHealth::Degraded,
        );
        snapshot.stale = true;
        snapshot.error = Some("codex API status strategy is not implemented".to_string());
        snapshot
    }

    async fn status_auto(&self) -> Result<UsageSnapshot> {
        self.status_cli().await
    }

    async fn status_cli(&self) -> Result<UsageSnapshot> {
        let auth_state = load_auth_state(&self.auth_path)?;

        if !auth_state.exists {
            let mut snapshot = UsageSnapshot::new(
                self.name(),
                UsageWindow::new(None, None),
                FetchSource::Cli,
                ProviderHealth::MissingCredentials,
            );
            snapshot.stale = true;
            snapshot.error = Some(format!(
                "codex auth file not found at {}",
                self.auth_path.display()
            ));
            return Ok(snapshot);
        }

        match self.read_account_via_app_server() {
            Ok(account) => Ok(snapshot_from_account(
                self.name(),
                &auth_state,
                Some(&account),
            )),
            Err(error) => {
                let mut snapshot = snapshot_from_account(self.name(), &auth_state, None);
                snapshot.health = ProviderHealth::Degraded;
                snapshot.stale = true;
                snapshot.error = Some(format!("failed to query codex app-server: {error}"));
                Ok(snapshot)
            }
        }
    }

    fn read_account_via_app_server(&self) -> Result<CodexAccountReadResult> {
        let initialize = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "clientInfo": {
                    "name": "codexbar-rs",
                    "version": env!("CARGO_PKG_VERSION"),
                },
                "capabilities": {
                    "experimentalApi": true
                }
            }
        });
        let account_read = json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "account/read",
            "params": {
                "refreshToken": false
            }
        });
        let rate_limits_read = json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "account/rateLimits/read",
            "params": {}
        });
        let script = format!(
            "printf '%s\\n%s\\n%s\\n' '{initialize}' '{account_read}' '{rate_limits_read}' | timeout 5s codex -s read-only -a never app-server --listen stdio://"
        );

        let output = Command::new("sh")
            .args(["-lc", &script])
            .output()
            .context("failed to query codex app-server via shell pipeline")?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        if !stdout.trim().is_empty() {
            return parse_account_read_response(&stdout);
        }

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let message = if stderr.is_empty() {
                format!("codex app-server exited with status {}", output.status)
            } else {
                format!("codex app-server failed: {stderr}")
            };
            bail!(message);
        }

        bail!("codex app-server returned no output")
    }
}

#[derive(Debug, Default)]
struct CodexAuthState {
    exists: bool,
    auth_mode: Option<String>,
    last_refresh: Option<String>,
}

fn resolve_auth_path() -> PathBuf {
    if let Ok(codex_home) = std::env::var("CODEX_HOME") {
        return PathBuf::from(codex_home).join("auth.json");
    }

    match std::env::var("HOME") {
        Ok(home) => PathBuf::from(home).join(".codex").join("auth.json"),
        Err(_) => PathBuf::from(".codex").join("auth.json"),
    }
}

fn load_auth_state(path: &PathBuf) -> Result<CodexAuthState> {
    if !path.exists() {
        return Ok(CodexAuthState::default());
    }

    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read codex auth file {}", path.display()))?;
    let auth = serde_json::from_str::<CodexAuthFile>(&raw)
        .with_context(|| format!("failed to parse codex auth file {}", path.display()))?;

    Ok(CodexAuthState {
        exists: true,
        auth_mode: auth.auth_mode,
        last_refresh: auth.last_refresh,
    })
}

fn snapshot_from_account(
    provider_name: &str,
    auth_state: &CodexAuthState,
    account: Option<&CodexAccountReadResult>,
) -> UsageSnapshot {
    let mut snapshot = UsageSnapshot::new(
        provider_name,
        UsageWindow::new(None, None),
        FetchSource::Cli,
        ProviderHealth::Ok,
    );
    snapshot.updated_at = auth_state.last_refresh.clone();

    match account {
        Some(result) if result.account.is_some() => {
            let _ = result.requires_openai_auth;
        }
        _ => {
            snapshot.health = ProviderHealth::MissingCredentials;
            snapshot.stale = true;
            snapshot.error = Some(format!(
                "codex account is not available via app-server{}",
                auth_mode_suffix(auth_state)
            ));
        }
    }

    snapshot
}
fn auth_mode_suffix(auth_state: &CodexAuthState) -> String {
    auth_state
        .auth_mode
        .as_ref()
        .map(|mode| format!(" (auth_mode={mode})"))
        .unwrap_or_default()
}

fn parse_account_read_response(stdout: &str) -> Result<CodexAccountReadResult> {
    for line in stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let response = serde_json::from_str::<JsonRpcResponse<CodexAccountReadResult>>(line)
            .with_context(|| "failed to decode codex app-server JSON-RPC response")?;

        if response.id == Some(2) {
            if let Some(error) = response.error {
                bail!(
                    "codex account/read returned error {}: {}",
                    error.code,
                    error.message
                );
            }

            return response
                .result
                .context("codex account/read returned no result");
        }
    }

    bail!("codex account/read returned no response")
}

#[derive(Debug, Deserialize)]
struct CodexAuthFile {
    #[serde(default)]
    auth_mode: Option<String>,
    #[serde(default)]
    last_refresh: Option<String>,
}

#[derive(Debug, Deserialize)]
struct JsonRpcResponse<T> {
    id: Option<u64>,
    result: Option<T>,
    error: Option<JsonRpcError>,
}

#[derive(Debug, Deserialize)]
struct JsonRpcError {
    code: i64,
    message: String,
}

#[derive(Debug, Deserialize)]
struct CodexAccountReadResult {
    #[serde(default)]
    account: Option<CodexAccount>,
    #[serde(rename = "requiresOpenaiAuth", default)]
    requires_openai_auth: bool,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct CodexAccount {
    #[serde(rename = "type")]
    account_type: String,
    #[serde(rename = "planType", default)]
    plan_type: Option<String>,
}

#[async_trait]
impl Provider for CodexProvider {
    fn name(&self) -> &'static str {
        "codex"
    }

    async fn status(&self, request: StatusRequest) -> Result<UsageSnapshot> {
        match request.source_mode {
            SourceMode::Auto => self.status_auto().await,
            SourceMode::Api => Ok(self.status_api()),
            SourceMode::Cli => self.status_cli().await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::parse_account_read_response;

    #[test]
    fn parse_account_read_response_extracts_account() {
        let raw = r#"{"id":1,"result":{"userAgent":"x","platformFamily":"unix","platformOs":"linux"}}
{"id":2,"result":{"account":{"type":"chatgpt","email":"user@example.com","planType":"plus"},"requiresOpenaiAuth":true}}
"#;

        let result = parse_account_read_response(raw).unwrap();
        let account = result.account.unwrap();
        assert_eq!(account.account_type, "chatgpt");
        assert_eq!(account.plan_type.as_deref(), Some("plus"));
        assert!(result.requires_openai_auth);
    }

    #[test]
    fn parse_account_read_response_rejects_missing_result() {
        let raw =
            r#"{"id":1,"result":{"userAgent":"x","platformFamily":"unix","platformOs":"linux"}}"#;
        let error = parse_account_read_response(raw).unwrap_err();
        assert!(error.to_string().contains("no response"));
    }
}
