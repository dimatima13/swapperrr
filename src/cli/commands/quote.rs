use crate::cli::{display::PoolDisplay, QuoteArgs};
use crate::core::{Config, QuoteRequest, SwapResult};
use crate::discovery::PoolDiscovery;
use crate::quotes::QuoteEngine;
use crate::selection::PoolSelector;
use colored::*;
use console::style;
use log::info;
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;
use std::sync::Arc;

pub async fn execute(args: QuoteArgs) -> SwapResult<()> {
    println!("{}", "🚀 Raydium Multi-Pool Quote Tool".bold().cyan());
    
    // Load configuration
    let config = Config::from_env()?;
    config.validate()?;

    // Create progress bar
    let pb = PoolDisplay::create_progress_bar("Initializing pool discovery...");

    // Initialize components
    let discovery = Arc::new(PoolDiscovery::new(config.clone())?);
    let quote_engine = Arc::new(QuoteEngine::new());
    let selector = PoolSelector::new(discovery.clone(), quote_engine);

    pb.set_message("Discovering pools...");

    // SOL/wSOL mint address
    let sol_mint = Pubkey::from_str("So11111111111111111111111111111111111111112").unwrap();
    
    // Determine token_out - always SOL if not specified
    let token_out = args.token_out.unwrap_or(sol_mint);
    
    // Check if trying to quote same token
    if args.token_in == token_out {
        pb.finish_and_clear();
        println!("{}", "❌ Cannot swap token to itself".red().bold());
        return Ok(());
    }

    // Convert amount to smallest units based on token decimals
    let decimals = crate::core::get_token_decimals(
        &solana_client::rpc_client::RpcClient::new(config.rpc_url.clone()),
        &args.token_in
    )?;
    let amount_in = (args.amount * 10f64.powi(decimals as i32)) as u64;

    let request = QuoteRequest {
        token_in: args.token_in,
        token_out,
        amount_in,
        slippage_bps: args.slippage,
    };

    info!(
        "Getting quotes for {} -> {} (amount: {}, slippage: {} bps)",
        args.token_in, token_out, amount_in, args.slippage
    );

    if args.all {
        // Get quotes from all pools
        pb.set_message("Getting quotes from all pools...");
        let quotes_by_type = selector.get_quotes_by_type(&request).await?;
        
        pb.finish_and_clear();
        
        if quotes_by_type.total() == 0 {
            println!(
                "{}",
                "❌ No pools found for this token pair".red().bold()
            );
            return Ok(());
        }

        PoolDisplay::display_quotes_by_type(&quotes_by_type);
    } else {
        // Get only the best quote
        pb.set_message("Finding best pool...");
        let best_quote = selector.select_best_pool(&request).await?;
        
        pb.finish_and_clear();

        match best_quote {
            Some(quote) => {
                println!("\n{}", "🏆 Best Quote Found".bold().green());
                PoolDisplay::display_quote(&quote, true);
            }
            None => {
                println!(
                    "{}",
                    "❌ No pools found for this token pair".red().bold()
                );
            }
        }
    }

    println!("{}", style("💡 Tip: Use --all flag to see all available pools").dim());

    Ok(())
}