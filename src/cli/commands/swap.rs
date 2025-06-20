use crate::cli::{display::PoolDisplay, SwapArgs};
use crate::core::{Config, QuoteRequest, SwapError, SwapParams, SwapResult};
use crate::discovery::PoolDiscovery;
use crate::quotes::QuoteEngine;
use crate::selection::PoolSelector;
use crate::transaction::TransactionExecutor;
use colored::*;
use dialoguer::{theme::ColorfulTheme, Confirm, Password};
use log::info;
use solana_sdk::{
    pubkey::Pubkey,
    signature::{Keypair, Signer},
};
use std::str::FromStr;
use std::sync::Arc;

pub async fn execute(args: SwapArgs) -> SwapResult<()> {
    println!("{}", "🚀 Raydium Multi-Pool Swap Tool".bold().cyan());
    
    // Load configuration
    let config = Config::from_env()?;
    config.validate()?;

    // Create progress bar
    let pb = PoolDisplay::create_progress_bar("Initializing...");

    // Initialize components
    let discovery = Arc::new(PoolDiscovery::new(config.clone())?);
    let quote_engine = Arc::new(QuoteEngine::new());
    let selector = PoolSelector::new(discovery.clone(), quote_engine);

    pb.set_message("Finding best pool...");

    // Convert amount to smallest units
    let amount_in = (args.amount * 10f64.powi(9)) as u64;

    let request = QuoteRequest {
        token_in: args.token_in,
        token_out: args.token_out,
        amount_in,
        slippage_bps: args.slippage,
    };

    // Find best pool
    let best_quote = selector.select_best_pool(&request).await?;

    pb.finish_and_clear();

    let quote = match best_quote {
        Some(q) => q,
        None => {
            println!(
                "{}",
                "❌ No pools found for this token pair".red().bold()
            );
            return Ok(());
        }
    };

    // Display swap details
    PoolDisplay::display_swap_confirmation(&quote);

    // Confirm swap
    let proceed = if args.yes {
        true
    } else {
        Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt("Do you want to proceed with this swap?")
            .default(false)
            .interact()
            .unwrap()
    };

    if !proceed {
        println!("{}", "❌ Swap cancelled".yellow());
        return Ok(());
    }

    // Get private key
    let pb = PoolDisplay::create_progress_bar("Loading wallet...");
    
    let private_key = if let Ok(key) = std::env::var("PRIVATE_KEY") {
        key
    } else {
        pb.finish_and_clear();
        println!("{}", "🔑 Enter your private key".yellow());
        Password::with_theme(&ColorfulTheme::default())
            .with_prompt("Private Key")
            .interact()
            .unwrap()
    };

    // Parse keypair
    let keypair = if private_key.len() == 88 {
        // Base58 encoded
        let bytes = bs58::decode(&private_key)
            .into_vec()
            .map_err(|_| SwapError::ConfigError("Invalid private key format".to_string()))?;
        Keypair::from_bytes(&bytes)
            .map_err(|_| SwapError::ConfigError("Invalid private key".to_string()))?
    } else if private_key.starts_with('[') && private_key.ends_with(']') {
        // JSON array format
        let bytes: Vec<u8> = serde_json::from_str(&private_key)
            .map_err(|_| SwapError::ConfigError("Invalid private key format".to_string()))?;
        Keypair::from_bytes(&bytes)
            .map_err(|_| SwapError::ConfigError("Invalid private key".to_string()))?
    } else {
        return Err(SwapError::ConfigError(
            "Private key must be base58 encoded or JSON array format".to_string(),
        ));
    };

    let user_pubkey = keypair.pubkey();
    info!("Using wallet: {}", user_pubkey);

    pb.set_message("Preparing transaction...");

    // Create transaction executor
    let executor = TransactionExecutor::new(config.rpc_url.clone(), keypair);

    let swap_params = SwapParams {
        quote: quote.clone(),
        user_pubkey,
        slippage_bps: args.slippage,
    };

    pb.set_message("Executing swap...");

    // Execute swap
    match executor.execute_swap(swap_params).await {
        Ok(result) => {
            pb.finish_and_clear();
            
            PoolDisplay::display_transaction_result(
                &result.signature,
                result.pool_type,
                result.expected_amount_out,
                result.amount_out,
                result.actual_slippage,
            );
            
            println!(
                "\n{}",
                format!(
                    "View on Solscan: https://solscan.io/tx/{}",
                    result.signature
                )
                .dim()
            );
        }
        Err(e) => {
            pb.finish_and_clear();
            println!("{} {}", "❌ Swap failed:".red().bold(), e);
            return Err(e);
        }
    }

    Ok(())
}