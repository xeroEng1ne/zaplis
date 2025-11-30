use anyhow::Result;
use clap::Parser;

mod cli;
mod model;
mod dag;
mod executor;
mod runner;

#[tokio::main]
async fn main() -> Result<()> {
    // -------- Logging Setup --------
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    // -------- Parse CLI arguments --------
    let args = cli::ZaplisArgs::parse();

    // -------- Branch based on command --------
    match args.command.as_str() {
        "run" => {
            tracing::info!("Running DAG...");
            runner::run(args).await?;
        }
        "plan" => {
            tracing::info!("Planning DAG...");
            dag::print_plan(&args)?;
        }
        _ => {
            println!("Unknown command. Use `run` or `plan`.");
        }
    }

    Ok(())
}
