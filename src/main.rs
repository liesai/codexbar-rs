use clap::Parser;
use codexbar_rs::{app, cli::Cli, output};
use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();

    match app::run(cli).await {
        Ok(response) => {
            println!("{}", output::to_json_string(&response));
            ExitCode::SUCCESS
        }
        Err(err) => {
            println!("{}", output::to_json_string(&output::from_error(&err)));
            ExitCode::from(1)
        }
    }
}
