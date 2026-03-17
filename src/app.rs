use crate::cache;
use crate::cli::{Cli, Command};
use crate::config::load_config;
use crate::output::{JsonResponse, success};
use crate::providers::status::fetch_usage as fetch_provider_usage;
use crate::providers::{
    ProviderConfig, ProviderRequest, StatusRequest, create_provider, provider_names,
};
use anyhow::Result;
use serde_json::json;

pub async fn run(cli: Cli) -> Result<JsonResponse> {
    match cli.command {
        Command::Ping { message } => Ok(success(json!({
            "status": "ok",
            "message": message
        }))),
        Command::Providers => Ok(success(json!({
            "providers": provider_names()
        }))),
        Command::Run {
            provider,
            prompt,
            model,
            base_url,
        } => {
            let provider_impl = create_provider(&provider, ProviderConfig { model, base_url })?;
            let response = provider_impl.generate(ProviderRequest { prompt }).await?;

            Ok(success(json!({
                "provider": response.provider,
                "output": response.output
            })))
        }
        Command::Status {
            json: _,
            source,
            refresh,
            no_cache,
        } => {
            let app_config = load_config();
            let effective_source = source.unwrap_or(app_config.status.default_source);
            let request = StatusRequest {
                source_mode: effective_source,
            };
            let cache_enabled = app_config.status.cache_enabled && !no_cache;
            let ttl_seconds = app_config.status.cache_ttl_seconds;

            let cached_record = if cache_enabled {
                cache::load_status_cache(effective_source).ok().flatten()
            } else {
                None
            };

            let usage = if cache_enabled && !refresh {
                match &cached_record {
                    Some(record) if record.is_fresh(ttl_seconds) => record.providers.clone(),
                    _ => fetch_live_status(request, cache_enabled, cached_record).await?,
                }
            } else {
                fetch_live_status(request, cache_enabled, cached_record).await?
            };

            Ok(success(json!({
                "providers": usage
            })))
        }
    }
}

async fn fetch_live_status(
    request: StatusRequest,
    cache_enabled: bool,
    cached_record: Option<cache::StatusCacheRecord>,
) -> Result<std::collections::BTreeMap<String, crate::providers::UsageSnapshot>> {
    match fetch_provider_usage(request).await {
        Ok(usage) => {
            if cache_enabled {
                let record = cache::StatusCacheRecord::new(request.source_mode, usage.clone());
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
