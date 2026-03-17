use crate::cache;
use crate::config::{AppConfig, config_exists, config_path, load_config};
use crate::providers::status::fetch_usage as fetch_provider_usage;
use crate::providers::{SourceMode, StatusRequest, UsageSnapshot, provider_names};
use anyhow::Result;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;

#[derive(Debug, Clone)]
pub struct BackendStatusInput {
    pub source: Option<SourceMode>,
    pub provider: Option<String>,
    pub refresh: bool,
    pub no_cache: bool,
}

#[derive(Debug, Clone)]
pub struct BackendDoctorInput {
    pub source: Option<SourceMode>,
}

#[derive(Debug, Serialize)]
pub struct BackendStatusOutput {
    pub providers: BTreeMap<String, UsageSnapshot>,
}

#[derive(Debug, Serialize)]
pub struct BackendConfigPathOutput {
    pub config_path: PathBuf,
}

#[derive(Debug, Serialize)]
pub struct BackendDoctorOutput {
    pub source_mode: SourceMode,
    pub summary: DoctorSummary,
    pub checks: Vec<DoctorCheck>,
}

#[derive(Debug, Serialize)]
pub struct DoctorSummary {
    ok: usize,
    warning: usize,
    error: usize,
}

#[derive(Debug, Serialize)]
pub struct DoctorCheck {
    name: String,
    status: DoctorStatus,
    message: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum DoctorStatus {
    Ok,
    Warning,
}

pub async fn get_status(input: BackendStatusInput) -> Result<BackendStatusOutput> {
    let app_config = load_config();
    let effective_source = input.source.unwrap_or(app_config.status.default_source);
    let provider = input.provider.map(|value| value.to_ascii_lowercase());
    let request = StatusRequest {
        source_mode: effective_source,
        provider: provider.clone(),
    };
    let cache_enabled = app_config.status.cache_enabled && !input.no_cache;
    let ttl_seconds = app_config.status.cache_ttl_seconds;

    let cached_record = if cache_enabled {
        cache::load_status_cache(effective_source, provider.as_deref())
            .ok()
            .flatten()
    } else {
        None
    };

    let providers = if cache_enabled && !input.refresh {
        match &cached_record {
            Some(record) if record.is_fresh(ttl_seconds) => record.providers.clone(),
            _ => fetch_live_status(request, cache_enabled, cached_record).await?,
        }
    } else {
        fetch_live_status(request, cache_enabled, cached_record).await?
    };

    Ok(BackendStatusOutput { providers })
}

pub fn get_config_path() -> Result<BackendConfigPathOutput> {
    Ok(BackendConfigPathOutput {
        config_path: config_path(),
    })
}

pub fn get_doctor(input: BackendDoctorInput) -> Result<BackendDoctorOutput> {
    let app_config = load_config();
    let source_mode = input.source.unwrap_or(app_config.status.default_source);
    Ok(build_doctor_report(source_mode, &app_config))
}

pub fn get_provider_names() -> &'static [&'static str] {
    provider_names()
}

async fn fetch_live_status(
    request: StatusRequest,
    cache_enabled: bool,
    cached_record: Option<cache::StatusCacheRecord>,
) -> Result<BTreeMap<String, UsageSnapshot>> {
    match fetch_provider_usage(request.clone()).await {
        Ok(usage) => {
            if cache_enabled {
                let record = cache::StatusCacheRecord::new(
                    request.source_mode,
                    request.provider.clone(),
                    usage.clone(),
                );
                let _ = cache::save_status_cache(&record);
            }
            Ok(usage)
        }
        Err(error) => match cached_record {
            Some(record) => Ok(cache::stale_cached_providers(
                &record.providers,
                &format!("live status collection failed: {error}"),
            )),
            None => Err(error),
        },
    }
}

fn build_doctor_report(source_mode: SourceMode, app_config: &AppConfig) -> BackendDoctorOutput {
    let cache_path = cache::cache_path_for(source_mode, None);
    let config_path = config_path();
    let cache_record = cache::load_status_cache(source_mode, None).ok().flatten();
    let mut checks = vec![
        DoctorCheck {
            name: "config_path".to_string(),
            status: if config_exists() {
                DoctorStatus::Ok
            } else {
                DoctorStatus::Warning
            },
            message: format!(
                "{} ({})",
                config_path.display(),
                if config_exists() {
                    "exists"
                } else {
                    "missing, defaults in use"
                }
            ),
        },
        DoctorCheck {
            name: "cache_path".to_string(),
            status: if path_has_parent(&cache_path) {
                DoctorStatus::Ok
            } else {
                DoctorStatus::Warning
            },
            message: format!("{}", cache_path.display()),
        },
        DoctorCheck {
            name: "cache_policy".to_string(),
            status: DoctorStatus::Ok,
            message: format!(
                "enabled={}, ttl={}s, default_source={}",
                app_config.status.cache_enabled,
                app_config.status.cache_ttl_seconds,
                app_config.status.default_source.as_str()
            ),
        },
        build_cache_state_check(cache_record.as_ref(), app_config.status.cache_ttl_seconds),
        build_codex_cli_check(),
        build_ollama_cli_check(),
        build_openai_api_key_check(),
        DoctorCheck {
            name: "provider_capabilities".to_string(),
            status: DoctorStatus::Ok,
            message: "codex=cli, mock=local, ollama=api+cli, openai=api".to_string(),
        },
    ];

    if matches!(source_mode, SourceMode::Cli) {
        checks.push(DoctorCheck {
            name: "openai_cli_support".to_string(),
            status: DoctorStatus::Warning,
            message: "openai --source cli is not implemented; only API-backed status is supported"
                .to_string(),
        });
        checks.push(DoctorCheck {
            name: "codex_api_support".to_string(),
            status: DoctorStatus::Warning,
            message: "codex --source api is not implemented; only CLI-backed status is supported"
                .to_string(),
        });
    }

    BackendDoctorOutput {
        source_mode,
        summary: summarize_checks(&checks),
        checks,
    }
}

fn path_has_parent(path: &Path) -> bool {
    path.parent().is_some()
}

fn build_cache_state_check(
    cache_record: Option<&cache::StatusCacheRecord>,
    ttl_seconds: u64,
) -> DoctorCheck {
    match cache_record {
        Some(record) if record.is_fresh(ttl_seconds) => DoctorCheck {
            name: "cache_state".to_string(),
            status: DoctorStatus::Ok,
            message: format!(
                "fresh cache available for source={} (cached_at_unix={})",
                record.source_mode.as_str(),
                record.cached_at_unix
            ),
        },
        Some(record) => DoctorCheck {
            name: "cache_state".to_string(),
            status: DoctorStatus::Warning,
            message: format!(
                "stale cache available for source={} (cached_at_unix={})",
                record.source_mode.as_str(),
                record.cached_at_unix
            ),
        },
        None => DoctorCheck {
            name: "cache_state".to_string(),
            status: DoctorStatus::Warning,
            message: "no cache file found for the selected source".to_string(),
        },
    }
}

fn build_ollama_cli_check() -> DoctorCheck {
    match ProcessCommand::new("ollama").arg("--version").output() {
        Ok(output) if output.status.success() => DoctorCheck {
            name: "ollama_cli".to_string(),
            status: DoctorStatus::Ok,
            message: "ollama CLI detected".to_string(),
        },
        Ok(output) => DoctorCheck {
            name: "ollama_cli".to_string(),
            status: DoctorStatus::Warning,
            message: format!("ollama CLI returned non-zero status: {}", output.status),
        },
        Err(error) => DoctorCheck {
            name: "ollama_cli".to_string(),
            status: DoctorStatus::Warning,
            message: format!("ollama CLI not available: {error}"),
        },
    }
}

fn build_codex_cli_check() -> DoctorCheck {
    match ProcessCommand::new("codex").arg("--version").output() {
        Ok(output) if output.status.success() => DoctorCheck {
            name: "codex_cli".to_string(),
            status: DoctorStatus::Ok,
            message: "codex CLI detected".to_string(),
        },
        Ok(output) => DoctorCheck {
            name: "codex_cli".to_string(),
            status: DoctorStatus::Warning,
            message: format!("codex CLI returned non-zero status: {}", output.status),
        },
        Err(error) => DoctorCheck {
            name: "codex_cli".to_string(),
            status: DoctorStatus::Warning,
            message: format!("codex CLI not available: {error}"),
        },
    }
}

fn build_openai_api_key_check() -> DoctorCheck {
    let admin_key_present = std::env::var("OPENAI_ADMIN_KEY")
        .ok()
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);
    let api_key_present = std::env::var("OPENAI_API_KEY")
        .ok()
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);

    DoctorCheck {
        name: "openai_credentials".to_string(),
        status: if admin_key_present || api_key_present {
            DoctorStatus::Ok
        } else {
            DoctorStatus::Warning
        },
        message: if admin_key_present {
            "OPENAI_ADMIN_KEY is set".to_string()
        } else if api_key_present {
            "OPENAI_API_KEY is set; organization usage endpoints may still require an admin-scoped key"
                .to_string()
        } else {
            "OPENAI_ADMIN_KEY or OPENAI_API_KEY is not set".to_string()
        },
    }
}

fn summarize_checks(checks: &[DoctorCheck]) -> DoctorSummary {
    let mut summary = DoctorSummary {
        ok: 0,
        warning: 0,
        error: 0,
    };

    for check in checks {
        match check.status {
            DoctorStatus::Ok => summary.ok += 1,
            DoctorStatus::Warning => summary.warning += 1,
        }
    }

    summary
}
