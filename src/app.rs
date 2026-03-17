use crate::backend;
use crate::cli::{Cli, Command, ConfigCommand};
use crate::output::{JsonResponse, success};
use anyhow::Result;
use serde_json::json;

pub async fn run(cli: Cli) -> Result<JsonResponse> {
    match cli.command {
        Command::Ping { message } => Ok(success(json!({
            "status": "ok",
            "message": message
        }))),
        Command::Providers => Ok(success(json!({
            "providers": backend::get_provider_names()
        }))),
        Command::Config { command } => match command {
            ConfigCommand::Path => Ok(success(json!(backend::get_config_path()?))),
        },
        Command::Doctor { json: _, source } => Ok(success(json!(backend::get_doctor(
            backend::BackendDoctorInput { source }
        )?))),
        Command::Status {
            json: _,
            provider,
            source,
            refresh,
            no_cache,
        } => Ok(success(json!(
            backend::get_status(backend::BackendStatusInput {
                source,
                provider,
                refresh,
                no_cache,
            })
            .await?
        ))),
    }
}
