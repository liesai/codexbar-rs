use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

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
            Ok(status) => Ok(snapshot_from_app_server_status(
                self.name(),
                &auth_state,
                &status,
            )),
            Err(error) => {
                let mut snapshot = snapshot_from_app_server_status(
                    self.name(),
                    &auth_state,
                    &CodexAppServerStatus::default(),
                );
                snapshot.health = ProviderHealth::Degraded;
                snapshot.stale = true;
                snapshot.error = Some(format!("failed to query codex app-server: {error}"));
                Ok(snapshot)
            }
        }
    }

    fn try_status_tty_snapshot(&self) -> Result<CodexTtyStatusSnapshot> {
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows: 30,
                cols: 100,
                pixel_width: 0,
                pixel_height: 0,
            })
            .context("failed to open PTY for codex")?;

        let cmd = CommandBuilder::new("codex");
        let mut cmd = cmd;
        cmd.arg("--no-alt-screen");

        let mut child = pair
            .slave
            .spawn_command(cmd)
            .context("failed to spawn codex in PTY mode")?;
        drop(pair.slave);

        let reader = pair
            .master
            .try_clone_reader()
            .context("failed to clone codex PTY reader")?;
        let output = Arc::new(Mutex::new(Vec::new()));
        let output_clone = Arc::clone(&output);

        let reader_thread = thread::spawn(move || {
            let mut reader = reader;
            let mut buffer = [0_u8; 4096];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(read) => {
                        if let Ok(mut output) = output_clone.lock() {
                            output.extend_from_slice(&buffer[..read]);
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        let mut writer = pair
            .master
            .take_writer()
            .context("failed to open codex PTY writer")?;

        thread::sleep(Duration::from_secs(7));
        writer
            .write_all(b"/status\r")
            .context("failed to write /status to codex PTY")?;
        writer
            .flush()
            .context("failed to flush /status to codex PTY")?;

        thread::sleep(Duration::from_secs(1));
        writer
            .write_all(b"\r")
            .context("failed to confirm /status in codex PTY")?;
        writer
            .flush()
            .context("failed to flush confirm key to codex PTY")?;

        thread::sleep(Duration::from_secs(6));
        let _ = child.kill();
        let _ = child.wait();
        drop(writer);
        let _ = reader_thread.join();

        let raw = String::from_utf8_lossy(
            &output
                .lock()
                .map_err(|_| anyhow::anyhow!("failed to lock codex PTY output buffer"))?,
        )
        .to_string();

        parse_tty_status_snapshot(&raw)
    }

    fn read_account_via_app_server(&self) -> Result<CodexAppServerStatus> {
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
            return parse_app_server_status_response(&stdout);
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

#[derive(Debug, Default)]
struct CodexTokenUsage {
    prompt_tokens: Option<u32>,
    completion_tokens: Option<u32>,
    total_tokens: Option<u32>,
    updated_at: Option<String>,
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

fn resolve_sessions_root() -> PathBuf {
    if let Ok(codex_home) = std::env::var("CODEX_HOME") {
        return PathBuf::from(codex_home).join("sessions");
    }

    match std::env::var("HOME") {
        Ok(home) => PathBuf::from(home).join(".codex").join("sessions"),
        Err(_) => PathBuf::from(".codex").join("sessions"),
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

fn snapshot_from_app_server_status(
    provider_name: &str,
    auth_state: &CodexAuthState,
    status: &CodexAppServerStatus,
) -> UsageSnapshot {
    let mut snapshot = UsageSnapshot::new(
        provider_name,
        UsageWindow::new(None, None),
        FetchSource::Cli,
        ProviderHealth::Ok,
    );
    snapshot.updated_at = auth_state.last_refresh.clone();
    snapshot.auth_mode = auth_state.auth_mode.clone();

    match status.account.as_ref() {
        Some(result) if result.account.is_some() => {
            if let Some(account) = result.account.as_ref() {
                snapshot.account = account.email.clone();
                snapshot.plan = account.plan_type.clone();
            }
        }
        _ => {
            snapshot.health = ProviderHealth::MissingCredentials;
            snapshot.stale = true;
            snapshot.error = Some(format!(
                "codex account is not available via app-server{}",
                auth_mode_suffix(auth_state)
            ));
            return snapshot;
        }
    }

    if let Some(rate_limit) = select_codex_rate_limit(status.rate_limits.as_ref()) {
        if let Some(primary) = rate_limit.primary.as_ref() {
            snapshot.primary = rate_limit_window_to_usage_window(primary);
        }
        snapshot.secondary = rate_limit
            .secondary
            .as_ref()
            .map(rate_limit_window_to_usage_window);
    }

    if snapshot.primary.used.is_none() {
        if let Ok(tty_status) = try_status_tty_snapshot() {
            if let Some(primary) = tty_status.primary {
                snapshot.primary = primary;
            }
            if let Some(secondary) = tty_status.secondary {
                snapshot.secondary = Some(secondary);
            }
        }
    }

    if let Ok(Some(tokens)) = load_latest_session_token_usage() {
        snapshot.prompt_tokens = tokens.prompt_tokens;
        snapshot.completion_tokens = tokens.completion_tokens;
        snapshot.total_tokens = tokens.total_tokens;
        snapshot.updated_at = tokens.updated_at.or(snapshot.updated_at);
    }

    if let Some(error) = &status.rate_limits_error {
        snapshot.health = ProviderHealth::Degraded;
        snapshot.error = Some(error.clone());
    }

    snapshot
}

fn try_status_tty_snapshot() -> Result<CodexTtyStatusSnapshot> {
    CodexProvider {
        auth_path: resolve_auth_path(),
    }
    .try_status_tty_snapshot()
}

fn load_latest_session_token_usage() -> Result<Option<CodexTokenUsage>> {
    let mut latest_file = None;
    collect_latest_jsonl_file(&resolve_sessions_root(), &mut latest_file)?;

    let Some(path) = latest_file.map(|entry| entry.path) else {
        return Ok(None);
    };

    let raw = fs::read_to_string(&path)
        .with_context(|| format!("failed to read codex session log {}", path.display()))?;
    let mut latest_usage = None;

    for line in raw.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let Ok(event) = serde_json::from_str::<CodexSessionEvent>(line) else {
            continue;
        };

        if event.event_type != "event_msg" {
            continue;
        }

        let Some(payload) = event.payload else {
            continue;
        };
        if payload.payload_type != "token_count" {
            continue;
        }
        let Some(info) = payload.info else {
            continue;
        };
        let Some(total) = info.total_token_usage else {
            continue;
        };

        let completion_tokens = total
            .output_tokens
            .unwrap_or(0)
            .checked_add(total.reasoning_output_tokens.unwrap_or(0));

        latest_usage = Some(CodexTokenUsage {
            prompt_tokens: total
                .input_tokens
                .and_then(|value| u32::try_from(value).ok()),
            completion_tokens: completion_tokens.and_then(|value| u32::try_from(value).ok()),
            total_tokens: total
                .total_tokens
                .and_then(|value| u32::try_from(value).ok()),
            updated_at: event.timestamp,
        });
    }

    Ok(latest_usage)
}

fn collect_latest_jsonl_file(dir: &PathBuf, latest: &mut Option<LatestJsonlFile>) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }

    for entry in fs::read_dir(dir)
        .with_context(|| format!("failed to read session dir {}", dir.display()))?
    {
        let entry = entry.with_context(|| format!("failed to read entry in {}", dir.display()))?;
        let path = entry.path();
        let metadata = entry
            .metadata()
            .with_context(|| format!("failed to stat {}", path.display()))?;

        if metadata.is_dir() {
            collect_latest_jsonl_file(&path, latest)?;
            continue;
        }

        if path.extension().and_then(|value| value.to_str()) != Some("jsonl") {
            continue;
        }

        let modified = metadata.modified().ok();
        let should_replace = match latest {
            Some(current) => modified > current.modified,
            None => true,
        };

        if should_replace {
            *latest = Some(LatestJsonlFile { path, modified });
        }
    }

    Ok(())
}

fn auth_mode_suffix(auth_state: &CodexAuthState) -> String {
    auth_state
        .auth_mode
        .as_ref()
        .map(|mode| format!(" (auth_mode={mode})"))
        .unwrap_or_default()
}

fn parse_app_server_status_response(stdout: &str) -> Result<CodexAppServerStatus> {
    let mut status = CodexAppServerStatus::default();

    for line in stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let response = serde_json::from_str::<JsonRpcResponse<Value>>(line)
            .with_context(|| "failed to decode codex app-server JSON-RPC response")?;

        match response.id {
            Some(2) => {
                if let Some(error) = response.error {
                    bail!(
                        "codex account/read returned error {}: {}",
                        error.code,
                        error.message
                    );
                }

                status.account = Some(
                    serde_json::from_value(
                        response
                            .result
                            .context("codex account/read returned no result payload")?,
                    )
                    .context("failed to decode codex account/read result payload")?,
                );
            }
            Some(3) => {
                if let Some(error) = response.error {
                    status.rate_limits_error = Some(format!(
                        "codex rateLimits/read returned error {}: {}",
                        error.code, error.message
                    ));
                    continue;
                }

                if let Some(result) = response.result {
                    status.rate_limits = Some(
                        serde_json::from_value(result)
                            .context("failed to decode codex rateLimits/read result payload")?,
                    );
                }
            }
            _ => {}
        }
    }

    if status.account.is_none() {
        bail!("codex account/read returned no response");
    }

    Ok(status)
}

fn select_codex_rate_limit(
    rate_limits: Option<&CodexRateLimitsReadResult>,
) -> Option<&CodexRateLimitSnapshot> {
    let rate_limits = rate_limits?;

    if let Some(by_limit_id) = rate_limits.rate_limits_by_limit_id.as_ref() {
        if let Some(snapshot) = by_limit_id.get("codex") {
            return Some(snapshot);
        }

        if let Some(snapshot) = by_limit_id
            .iter()
            .find(|(limit_id, _)| limit_id.eq_ignore_ascii_case("codex"))
            .map(|(_, snapshot)| snapshot)
        {
            return Some(snapshot);
        }
    }

    match rate_limits.rate_limits.as_ref() {
        Some(snapshot) if is_codex_limit(snapshot) || snapshot_has_usage(snapshot) => {
            Some(snapshot)
        }
        _ => None,
    }
}

fn is_codex_limit(snapshot: &CodexRateLimitSnapshot) -> bool {
    snapshot
        .limit_id
        .as_deref()
        .is_some_and(|value| value.eq_ignore_ascii_case("codex"))
        || snapshot
            .limit_name
            .as_deref()
            .is_some_and(|value| value.eq_ignore_ascii_case("codex"))
}

fn snapshot_has_usage(snapshot: &CodexRateLimitSnapshot) -> bool {
    snapshot.primary.is_some() || snapshot.secondary.is_some()
}

fn rate_limit_window_to_usage_window(window: &CodexRateLimitWindow) -> UsageWindow {
    let used = Some(u64::from(window.used_percent));
    let limit = Some(100);
    let mut usage_window = UsageWindow::new(used, limit);
    usage_window.resets_at = window.resets_at.map(|value| value.to_string());
    usage_window
}

fn parse_tty_status_snapshot(raw: &str) -> Result<CodexTtyStatusSnapshot> {
    let sanitized = strip_ansi(raw);
    let lines: Vec<&str> = sanitized.lines().map(str::trim).collect();

    let mut primary = None;
    let mut secondary = None;

    for (index, line) in lines.iter().enumerate() {
        let next_reset_line = find_next_reset_line(&lines, index);
        if let Some(window) = parse_tty_window_line(line, next_reset_line) {
            if line.contains("5h limit:") {
                primary = Some(window);
            } else if line.contains("Weekly limit:") {
                secondary = Some(window);
            }
        }
    }

    if primary.is_none() && secondary.is_none() {
        bail!("codex /status did not expose rate limit windows");
    }

    Ok(CodexTtyStatusSnapshot { primary, secondary })
}

fn find_next_reset_line<'a>(lines: &'a [&'a str], index: usize) -> Option<&'a str> {
    for line in lines.iter().skip(index + 1).take(4) {
        if line.is_empty() {
            continue;
        }
        if line.contains("limit:") {
            break;
        }
        if line.contains("(resets ") {
            return Some(*line);
        }
    }

    None
}

fn parse_tty_window_line(line: &str, next_line: Option<&str>) -> Option<UsageWindow> {
    let left_marker = "% left";
    let left_index = line.find(left_marker)?;
    let left_digits = line[..left_index]
        .chars()
        .rev()
        .skip_while(|ch| !ch.is_ascii_digit())
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>();
    let remaining = left_digits.parse::<u64>().ok()?;
    let used = 100_u64.saturating_sub(remaining);

    let mut window = UsageWindow::new(Some(used), Some(100));
    if let Some(next_line) = next_line {
        if let Some(start) = next_line.find("(resets ") {
            let resets = next_line[start + 8..]
                .trim()
                .trim_matches('│')
                .trim()
                .trim_end_matches(')')
                .trim()
                .to_string();
            if !resets.is_empty() {
                window.resets_at = Some(resets);
            }
        }
    }

    Some(window)
}

fn strip_ansi(raw: &str) -> String {
    let mut output = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' {
            match chars.peek().copied() {
                Some('[') => {
                    chars.next();
                    while let Some(next) = chars.next() {
                        if ('@'..='~').contains(&next) {
                            break;
                        }
                    }
                }
                Some(']') => {
                    chars.next();
                    while let Some(next) = chars.next() {
                        if next == '\u{7}' {
                            break;
                        }
                        if next == '\\' {
                            break;
                        }
                    }
                }
                _ => {}
            }
            continue;
        }

        if ch != '\r' {
            output.push(ch);
        }
    }

    output
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
#[allow(dead_code)]
struct CodexAccountReadResult {
    #[serde(default)]
    account: Option<CodexAccount>,
    #[serde(rename = "requiresOpenaiAuth", default)]
    requires_openai_auth: bool,
}

#[derive(Debug, Default, Deserialize)]
struct CodexAppServerStatus {
    #[serde(default)]
    account: Option<CodexAccountReadResult>,
    #[serde(default)]
    rate_limits: Option<CodexRateLimitsReadResult>,
    #[serde(default)]
    rate_limits_error: Option<String>,
}

#[derive(Debug, Default)]
struct CodexTtyStatusSnapshot {
    primary: Option<UsageWindow>,
    secondary: Option<UsageWindow>,
}

struct LatestJsonlFile {
    path: PathBuf,
    modified: Option<std::time::SystemTime>,
}

#[derive(Debug, Deserialize)]
struct CodexSessionEvent {
    #[serde(default)]
    timestamp: Option<String>,
    #[serde(rename = "type")]
    event_type: String,
    #[serde(default)]
    payload: Option<CodexSessionPayload>,
}

#[derive(Debug, Deserialize)]
struct CodexSessionPayload {
    #[serde(rename = "type")]
    payload_type: String,
    #[serde(default)]
    info: Option<CodexSessionTokenInfo>,
}

#[derive(Debug, Deserialize)]
struct CodexSessionTokenInfo {
    #[serde(rename = "total_token_usage", default)]
    total_token_usage: Option<CodexSessionTokenTotals>,
}

#[derive(Debug, Deserialize)]
struct CodexSessionTokenTotals {
    #[serde(default)]
    input_tokens: Option<u64>,
    #[serde(default)]
    output_tokens: Option<u64>,
    #[serde(default)]
    reasoning_output_tokens: Option<u64>,
    #[serde(default)]
    total_tokens: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct CodexAccount {
    #[serde(rename = "type")]
    account_type: String,
    #[serde(default)]
    email: Option<String>,
    #[serde(rename = "planType", default)]
    plan_type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CodexRateLimitsReadResult {
    #[serde(rename = "rateLimits", default)]
    rate_limits: Option<CodexRateLimitSnapshot>,
    #[serde(rename = "rateLimitsByLimitId", default)]
    rate_limits_by_limit_id: Option<BTreeMap<String, CodexRateLimitSnapshot>>,
}

#[derive(Debug, Deserialize)]
struct CodexRateLimitSnapshot {
    #[serde(rename = "limitId", default)]
    limit_id: Option<String>,
    #[serde(rename = "limitName", default)]
    limit_name: Option<String>,
    #[serde(default)]
    primary: Option<CodexRateLimitWindow>,
    #[serde(default)]
    secondary: Option<CodexRateLimitWindow>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct CodexRateLimitWindow {
    #[serde(rename = "usedPercent")]
    used_percent: u32,
    #[serde(rename = "resetsAt", default)]
    resets_at: Option<u64>,
    #[serde(rename = "windowDurationMins", default)]
    window_duration_mins: Option<u64>,
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
    use super::{
        parse_app_server_status_response, parse_tty_status_snapshot, select_codex_rate_limit,
    };

    #[test]
    fn parse_account_read_response_extracts_account() {
        let raw = r#"{"id":1,"result":{"userAgent":"x","platformFamily":"unix","platformOs":"linux"}}
{"id":2,"result":{"account":{"type":"chatgpt","email":"user@example.com","planType":"plus"},"requiresOpenaiAuth":true}}
"#;

        let status = parse_app_server_status_response(raw).unwrap();
        let result = status.account.unwrap();
        let account = result.account.unwrap();
        assert_eq!(account.account_type, "chatgpt");
        assert_eq!(account.plan_type.as_deref(), Some("plus"));
        assert!(result.requires_openai_auth);
    }

    #[test]
    fn parse_account_read_response_rejects_missing_result() {
        let raw =
            r#"{"id":1,"result":{"userAgent":"x","platformFamily":"unix","platformOs":"linux"}}"#;
        let error = parse_app_server_status_response(raw).unwrap_err();
        assert!(error.to_string().contains("no response"));
    }

    #[test]
    fn parse_app_server_status_response_extracts_rate_limits() {
        let raw = r#"{"id":1,"result":{"userAgent":"x","platformFamily":"unix","platformOs":"linux"}}
{"id":2,"result":{"account":{"type":"chatgpt","email":"user@example.com","planType":"plus"},"requiresOpenaiAuth":true}}
{"id":3,"result":{"rateLimits":{"limitId":"other","primary":{"usedPercent":21,"resetsAt":1774000000}},"rateLimitsByLimitId":{"codex":{"limitId":"codex","primary":{"usedPercent":42,"resetsAt":1775000000},"secondary":{"usedPercent":7,"resetsAt":1775003600}}}}}
"#;

        let status = parse_app_server_status_response(raw).unwrap();
        let snapshot = select_codex_rate_limit(status.rate_limits.as_ref()).unwrap();
        assert_eq!(snapshot.limit_id.as_deref(), Some("codex"));
        assert_eq!(
            snapshot.primary.as_ref().map(|value| value.used_percent),
            Some(42)
        );
        assert_eq!(
            snapshot.secondary.as_ref().map(|value| value.used_percent),
            Some(7)
        );
    }

    #[test]
    fn parse_tty_status_snapshot_extracts_windows() {
        let raw = r#"
│  5h limit:             [███████████████████░] 95% left
│                        (resets 02:00 on 18 Mar)
│  Weekly limit:         [███████░░░░░░░░░░░░░] 33% left
│                        (resets 13:00 on 18 Mar)
"#;

        let snapshot = parse_tty_status_snapshot(raw).unwrap();
        assert_eq!(snapshot.primary.as_ref().and_then(|w| w.used), Some(5));
        assert_eq!(snapshot.primary.as_ref().and_then(|w| w.limit), Some(100));
        assert_eq!(
            snapshot
                .primary
                .as_ref()
                .and_then(|w| w.resets_at.as_deref()),
            Some("02:00 on 18 Mar")
        );
        assert_eq!(snapshot.secondary.as_ref().and_then(|w| w.used), Some(67));
        assert_eq!(
            snapshot
                .secondary
                .as_ref()
                .and_then(|w| w.resets_at.as_deref()),
            Some("13:00 on 18 Mar")
        );
    }
}
