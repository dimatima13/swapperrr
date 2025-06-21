use anyhow::Result;
use clap::Parser;
use env_logger::Env;
use log::info;

mod cli;
mod core;
mod discovery;
mod quotes;
mod selection;
mod transaction;
mod utils;

use cli::{Cli, Commands};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize environment from .env file
    dotenv::dotenv().ok();

    // Initialize logger
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
    
    info!("Starting Raydium Multi-Pool Swap Tool");
    
    // Parse CLI arguments
    let cli = Cli::parse();
    
    // Execute command
    match cli.command {
        Commands::Quote(args) => {
            cli::commands::quote::execute(args).await?;
        }
        Commands::Swap(args) => {
            cli::commands::swap::execute(args).await?;
        }
        Commands::Pools(args) => {
            cli::commands::pools::execute(args).await?;
        }
    }
    
    Ok(())
}