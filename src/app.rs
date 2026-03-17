use crate::cli::{Cli, Command};
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
        Command::Status { json: _, source } => {
            let usage = fetch_provider_usage(StatusRequest {
                source_mode: source,
            })
            .await?;
            Ok(success(json!({
                "providers": usage
            })))
        }
    }
}
