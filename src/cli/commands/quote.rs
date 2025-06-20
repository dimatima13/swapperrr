use crate::cli::{display::PoolDisplay, QuoteArgs};
use crate::core::{Config, QuoteRequest, SwapError, SwapResult};
use crate::discovery::PoolDiscovery;
use crate::quotes::QuoteEngine;
use crate::selection::PoolSelector;
use colored::*;
use log::info;
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

    // Convert amount to smallest units based on token decimals
    // For now, assume 9 decimals for input token (will be improved with token metadata)
    let amount_in = (args.amount * 10f64.powi(9)) as u64;

    let request = QuoteRequest {
        token_in: args.token_in,
        token_out: args.token_out,
        amount_in,
        slippage_bps: args.slippage,
    };

    info!(
        "Getting quotes for {} -> {} (amount: {}, slippage: {} bps)",
        args.token_in, args.token_out, amount_in, args.slippage
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

    println!("{}", "💡 Tip: Use --all flag to see all available pools".dim());

    Ok(())
}