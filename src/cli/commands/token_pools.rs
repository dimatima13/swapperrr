use crate::cli::{display::PoolDisplay, TokenPoolsArgs};
use crate::core::{Config, SwapError, SwapResult, PoolType};
use crate::discovery::PoolDiscovery;
use colored::*;
use console::style;
use std::sync::Arc;
use std::collections::HashMap;
use solana_sdk::pubkey::Pubkey;

pub async fn execute(args: TokenPoolsArgs) -> SwapResult<()> {
    println!("{}", "🔍 Searching pools containing token...".bold().cyan());
    
    // Load configuration
    let config = Config::from_env()?;
    config.validate()?;

    // Create progress bar
    let pb = PoolDisplay::create_progress_bar("Discovering pools...");

    // Initialize pool discovery
    let discovery = Arc::new(PoolDiscovery::new(config.clone())?);

    // Filter by pool type if specified
    let pool_types = if let Some(pool_type_str) = &args.pool_type {
        match pool_type_str.to_lowercase().as_str() {
            "amm" => vec![PoolType::AMM],
            "stable" => vec![PoolType::Stable],
            "clmm" => vec![PoolType::CLMM],
            _ => {
                pb.finish_and_clear();
                return Err(SwapError::InvalidInput(
                    "Invalid pool type. Use: amm, stable, or clmm".to_string()
                ));
            }
        }
    } else {
        vec![PoolType::AMM, PoolType::Stable, PoolType::CLMM]
    };

    pb.set_message("Searching AMM pools...");

    // Find all pools containing the token
    pb.set_message("Searching for pools...");
    
    let all_pools = if pool_types.len() == 1 && pool_types[0] == PoolType::AMM {
        // If only searching AMM pools, use the optimized method
        discovery.find_pools_by_token(args.token).await?
    } else {
        // For other pool types or all types, we need to check against common pairs
        let mut pools = Vec::new();
        
        // Common tokens to check against
        let common_tokens = vec![
            // SOL
            Pubkey::try_from("So11111111111111111111111111111111111111112").unwrap(),
            // USDC
            Pubkey::try_from("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v").unwrap(),
            // USDT
            Pubkey::try_from("Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB").unwrap(),
            // RAY
            Pubkey::try_from("4k3Dyjzvzp8eMZWUXbBCjEvwSkkk59S5iCNLY3QrkX6R").unwrap(),
        ];

        for pool_type in &pool_types {
            match pool_type {
                PoolType::AMM => {
                    pb.set_message("Searching AMM pools...");
                    if let Ok(amm_pools) = discovery.find_pools_by_token(args.token).await {
                        pools.extend(amm_pools);
                    }
                }
                PoolType::Stable | PoolType::CLMM => {
                    pb.set_message(format!("Searching {} pools...", pool_type));
                    // For Stable and CLMM, check against common pairs
                    for other_token in &common_tokens {
                        if *other_token == args.token {
                            continue;
                        }
                        
                        // Try both directions and filter by type
                        if let Ok(found_pools) = discovery.find_pools_by_type(args.token, *other_token, *pool_type).await {
                            for pool in found_pools {
                                if !pools.iter().any(|p: &crate::core::PoolInfo| p.address == pool.address) {
                                    pools.push(pool);
                                }
                            }
                        }
                        
                        if let Ok(found_pools) = discovery.find_pools_by_type(*other_token, args.token, *pool_type).await {
                            for pool in found_pools {
                                if !pools.iter().any(|p: &crate::core::PoolInfo| p.address == pool.address) {
                                    pools.push(pool);
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        
        pools
    };

    pb.finish_and_clear();

    if all_pools.is_empty() {
        println!(
            "{}",
            "❌ No pools found containing this token".red().bold()
        );
        return Ok(());
    }

    // Group pools by trading pair
    let mut pools_by_pair: HashMap<String, Vec<crate::core::PoolInfo>> = HashMap::new();
    
    for pool in all_pools {
        let pair_key = if pool.token_a.mint == args.token {
            format!("{} → {}", pool.token_a.symbol, pool.token_b.symbol)
        } else {
            format!("{} → {}", pool.token_b.symbol, pool.token_a.symbol)
        };
        
        pools_by_pair.entry(pair_key).or_insert_with(Vec::new).push(pool);
    }

    // Display results
    println!(
        "\n{} {} {} {}\n",
        "Found".green().bold(),
        pools_by_pair.values().map(|v| v.len()).sum::<usize>().to_string().cyan().bold(),
        "pools containing".green().bold(),
        args.token.to_string().bright_yellow()
    );

    for (pair, pools) in pools_by_pair.iter() {
        println!("\n{} {}", "Trading Pair:".bright_blue().bold(), pair.bright_white());
        println!("{}", "─".repeat(60));

        if args.detailed {
            for pool in pools {
                PoolDisplay::display_pool_detailed(pool);
                println!();
            }
        } else {
            for pool in pools {
                let pool_type_str = match pool.pool_type {
                    PoolType::AMM => "AMM   ".bright_green(),
                    PoolType::Stable => "STABLE".bright_blue(),
                    PoolType::CLMM => "CLMM  ".bright_magenta(),
                    PoolType::Standard => "STD   ".white(),
                };

                println!(
                    "  {} {} | Liquidity: ${:>10.2} | Volume 24h: ${:>10.2}",
                    pool_type_str,
                    style(pool.address.to_string()).dim(),
                    pool.liquidity_usd,
                    pool.volume_24h_usd
                );
            }
        }
    }

    println!(
        "\n{}: Use {} to see detailed pool information",
        "Tip".bright_yellow().bold(),
        "--detailed".bright_cyan()
    );

    Ok(())
}