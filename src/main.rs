use clap::Parser;
use raydium_multipool_swap::cli::{Cli, Commands};
use raydium_multipool_swap::core::{Config, SwapError};

#[tokio::main]
async fn main() -> Result<(), SwapError> {
    // Initialize logger
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    // Load configuration
    dotenv::dotenv().ok();
    let config = Config::from_env()?;

    // Parse CLI arguments
    let cli = Cli::parse();

    // Execute command
    match cli.command {
        Commands::Quote(args) => {
            raydium_multipool_swap::cli::commands::quote::execute(args).await?;
        }
        Commands::Swap(args) => {
            raydium_multipool_swap::cli::commands::swap::execute(args).await?;
        }
        Commands::Pools(args) => {
            raydium_multipool_swap::cli::commands::pools::execute(args).await?;
        }
        Commands::TokenPools(args) => {
            raydium_multipool_swap::cli::commands::token_pools::execute(args).await?;
        }
        Commands::Wrap(args) => {
            use raydium_multipool_swap::cli::commands::wrap::WrapCommand;
            let wrap_cmd = WrapCommand {
                amount: args.amount,
                unwrap: args.unwrap,
            };
            wrap_cmd.execute(config).await?;
        }
    }

    Ok(())
}