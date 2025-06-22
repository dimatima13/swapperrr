use crate::cli::{display::PoolDisplay, SwapArgs};
use crate::core::{Config, QuoteRequest, SwapError, SwapParams, SwapResult};
use crate::discovery::PoolDiscovery;
use crate::quotes::QuoteEngine;
use crate::selection::PoolSelector;
use crate::transaction::TransactionExecutor;
use colored::*;
use console::style;
use dialoguer::{theme::ColorfulTheme, Confirm, Password};
use log::{info, warn};
use solana_sdk::{
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    signer::SeedDerivable,
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

    // SOL/wSOL mint address
    let sol_mint = Pubkey::from_str("So11111111111111111111111111111111111111112").unwrap();
    
    // Determine token_out - always SOL if not specified or if auto mode
    let token_out = if args.auto || args.token_out.is_none() {
        // If token_in is SOL, this will fail later with appropriate message
        sol_mint
    } else {
        args.token_out.unwrap()
    };
    
    // Check if trying to swap SOL to SOL
    if args.token_in == token_out {
        pb.finish_and_clear();
        println!("{}", "❌ Cannot swap token to itself".red().bold());
        return Ok(());
    }

    // Convert amount to smallest units based on token decimals
    // For now, assume 6 decimals for pump tokens, 9 for SOL
    // TODO: Get actual decimals from token metadata
    let decimals = if args.token_in == sol_mint { 9 } else { 6 };
    let amount_in = (args.amount * 10f64.powi(decimals)) as u64;

    let request = QuoteRequest {
        token_in: args.token_in,
        token_out,
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
    
    let private_key = if let Ok(key) = std::env::var("WALLET_PRIVATE_KEY") {
        key
    } else if let Ok(key) = std::env::var("PRIVATE_KEY") {
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
    let keypair = if private_key.starts_with('[') && private_key.ends_with(']') {
        // JSON array format
        let bytes: Vec<u8> = serde_json::from_str(&private_key)
            .map_err(|_| SwapError::ConfigError("Invalid private key format".to_string()))?;
        Keypair::from_bytes(&bytes)
            .map_err(|_| SwapError::ConfigError("Invalid private key".to_string()))?
    } else {
        // Try base58 encoded
        let mut bytes = bs58::decode(&private_key)
            .into_vec()
            .map_err(|_| SwapError::ConfigError("Invalid base58 private key format".to_string()))?;
        
        // Some wallets export keys with extra bytes (checksum, version, etc)
        // Try to handle common formats
        if bytes.len() == 65 {
            // Remove first byte (might be version byte)
            bytes.remove(0);
        } else if bytes.len() > 64 {
            // Take first 64 bytes
            bytes.truncate(64);
        }
        
        // Try different interpretations
        if bytes.len() == 32 {
            // This might be just the secret key (seed)
            Keypair::from_seed(&bytes)
                .map_err(|e| SwapError::ConfigError(format!("Invalid seed: {}", e)))?
        } else if bytes.len() == 64 {
            // This should be a full keypair
            // Try standard format first
            match Keypair::from_bytes(&bytes) {
                Ok(kp) => kp,
                Err(_) => {
                    // Maybe it's just the private key repeated or in a different format
                    // Try using first 32 bytes as seed
                    let seed = &bytes[..32];
                    Keypair::from_seed(seed)
                        .map_err(|e| SwapError::ConfigError(format!("Invalid seed from first 32 bytes: {}", e)))?
                }
            }
        } else {
            return Err(SwapError::ConfigError(
                format!("Private key must be 32 (seed) or 64 (keypair) bytes, got {}", bytes.len()),
            ));
        }
    };

    let user_pubkey = keypair.pubkey();
    info!("Using wallet: {}", user_pubkey);

    pb.set_message("Preparing transaction...");

    // Create transaction executor
    let mut executor = TransactionExecutor::new(config.rpc_url.clone(), keypair);
    
    // Set transaction version based on --legacy flag
    if args.legacy {
        executor.set_transaction_version(crate::transaction::TransactionVersion::Legacy);
        info!("Using legacy transaction format");
    } else {
        info!("Using v0 transaction format");
    }
    
    // Enable ALT if requested
    if args.use_alt && !args.legacy {
        executor.enable_alts(config.rpc_url.clone()).await;
        info!("Address Lookup Tables enabled");
    } else if args.use_alt && args.legacy {
        warn!("ALT is only supported with v0 transactions, ignoring --use-alt flag");
    }

    let swap_params = SwapParams {
        quote: quote.clone(),
        user_pubkey,
        slippage_bps: args.slippage,
        token_in: args.token_in,
        token_out,
    };

    pb.set_message("Executing swap...");

    // Execute swap
    match executor.execute_swap(swap_params).await {
        Ok(result) => {
            pb.finish_and_clear();
            
            // Get output token info
            let output_token = if quote.token_out == quote.pool_info.token_a.mint {
                &quote.pool_info.token_a
            } else {
                &quote.pool_info.token_b
            };
            
            PoolDisplay::display_transaction_result(
                &result.signature,
                result.pool_type,
                result.expected_amount_out,
                result.amount_out,
                result.actual_slippage,
                output_token,
            );
            
            println!(
                "\n{}",
                style(format!(
                    "View on Solscan: https://solscan.io/tx/{}",
                    result.signature
                )).dim()
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