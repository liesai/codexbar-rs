use clap::{Parser, Subcommand};

use crate::providers::SourceMode;

#[derive(Debug, Parser)]
#[command(
    name = "codexbar-rs",
    version,
    about = "Async Rust CLI with JSON output and pluggable providers"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Print health status JSON.
    Ping {
        /// Message echoed in the response payload.
        #[arg(long, default_value = "ok")]
        message: String,
    },
    /// Execute a prompt using a named provider.
    Run {
        /// Provider key, for example: mock, ollama.
        #[arg(long)]
        provider: String,
        /// Prompt text passed to the provider.
        #[arg(long)]
        prompt: String,
        /// Optional model override (used by providers that support it, e.g. ollama).
        #[arg(long)]
        model: Option<String>,
        /// Optional provider base URL (used by providers that support it, e.g. ollama).
        #[arg(long)]
        base_url: Option<String>,
    },
    /// List available providers.
    Providers,
    /// Report provider consumption status.
    Status {
        /// Serialize the status map as JSON.
        #[arg(long)]
        json: bool,
        /// Select the status source strategy.
        #[arg(long, value_enum)]
        source: Option<SourceMode>,
        /// Bypass any cached status snapshot and force live collection.
        #[arg(long)]
        refresh: bool,
        /// Disable cache reads and writes for this command.
        #[arg(long)]
        no_cache: bool,
    },
}
