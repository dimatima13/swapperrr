use crate::cli::{display::PoolDisplay, PoolsArgs};
use crate::core::{Config, SwapResult};
use crate::discovery::PoolDiscovery;
use colored::*;
use log::info;

pub async fn execute(args: PoolsArgs) -> SwapResult<()> {
    println!("{}", "🚀 Raydium Multi-Pool Discovery Tool".bold().cyan());
    
    // Load configuration
    let config = Config::from_env()?;
    config.validate()?;

    // Create progress bar
    let pb = PoolDisplay::create_progress_bar("Discovering pools...");

    // Initialize pool discovery
    let discovery = PoolDiscovery::new(config)?;

    info!(
        "Discovering pools for {}/{}",
        args.token_a, args.token_b
    );

    // Discover all pools
    let pools = discovery.discover_all_pools(args.token_a, args.token_b).await?;

    pb.finish_and_clear();

    if pools.is_empty() {
        println!(
            "{}",
            "❌ No pools found for this token pair".red().bold()
        );
        return Ok(());
    }

    // Group pools by type for summary
    let mut pool_counts = std::collections::HashMap::new();
    for pool in &pools {
        *pool_counts.entry(pool.pool_type).or_insert(0) += 1;
    }

    println!(
        "\n{} Found {} pools total",
        "📊".bold(),
        pools.len().to_string().green().bold()
    );

    for (pool_type, count) in pool_counts {
        let type_str = format!("{:?}", pool_type);
        let colored_type = match pool_type {
            crate::core::PoolType::AMM => type_str.blue(),
            crate::core::PoolType::Stable => type_str.green(),
            crate::core::PoolType::CLMM => type_str.yellow(),
            crate::core::PoolType::Standard => type_str.white(),
        };
        println!("   {} {}: {}", "•", colored_type, count);
    }

    // Display pool list
    PoolDisplay::display_pool_list(&pools, args.detailed);

    if !args.detailed {
        println!(
            "{}",
            "💡 Tip: Use --detailed flag for more pool information".dim()
        );
    }

    Ok(())
}