use crate::core::{PoolInfo, PoolType, PoolState, QuoteResult};
use crate::selection::QuotesByType;
use colored::*;
use console::style;
use indicatif::{ProgressBar, ProgressStyle};

/// Display helper for pool information
pub struct PoolDisplay;

impl PoolDisplay {
    /// Display a single quote result
    pub fn display_quote(quote: &QuoteResult, is_best: bool) {
        let pool_type_str = format!("{:?}", quote.pool_info.pool_type);
        let pool_type_colored = match quote.pool_info.pool_type {
            PoolType::AMM => pool_type_str.blue(),
            PoolType::Stable => pool_type_str.green(),
            PoolType::CLMM => pool_type_str.yellow(),
            PoolType::Standard => pool_type_str.white(),
        };

        let best_marker = if is_best {
            " ⭐ BEST".bright_green().bold()
        } else {
            "".normal()
        };

        println!(
            "{} {} Pool{}",
            style("►").cyan(),
            pool_type_colored.bold(),
            best_marker
        );

        // Determine input and output tokens based on swap direction
        let (input_token, output_token) = if quote.token_in == quote.pool_info.token_a.mint {
            (&quote.pool_info.token_a, &quote.pool_info.token_b)
        } else {
            (&quote.pool_info.token_b, &quote.pool_info.token_a)
        };

        println!(
            "  {} {} → {} {}",
            format_amount(quote.amount_in, input_token),
            input_token.symbol,
            format_amount(quote.amount_out, output_token),
            output_token.symbol
        );

        println!(
            "  Price Impact: {} | Fee: {} {}",
            format_impact(quote.price_impact),
            format_amount(quote.fee, input_token),
            input_token.symbol
        );

        println!(
            "  Min Output: {} {} ({}% slippage)",
            format_amount(quote.min_amount_out, output_token),
            output_token.symbol,
            ((quote.amount_out - quote.min_amount_out) as f64 / quote.amount_out as f64 * 100.0)
        );

        println!(
            "  Pool: {}",
            style(format!("{}", quote.pool_info.address)).dim()
        );
        println!();
    }

    /// Display all quotes grouped by type
    pub fn display_quotes_by_type(quotes: &QuotesByType) {
        let summary = quotes.summary();
        
        println!("\n{}", style("📊 Pool Discovery Summary").bold().underlined());
        println!(
            "Found {} pools: {} AMM | {} Stable | {} CLMM | {} Standard\n",
            quotes.total(),
            summary.amm_count,
            summary.stable_count,
            summary.clmm_count,
            summary.standard_count
        );

        if let Some(best) = quotes.best_quote() {
            println!("{}", style("🏆 Best Quote").bold().green());
            Self::display_quote(best, true);
        }

        // Display by pool type
        if !quotes.amm.is_empty() {
            println!("{}", style("AMM Pools").bold().blue());
            for quote in &quotes.amm {
                Self::display_quote(quote, false);
            }
        }

        if !quotes.stable.is_empty() {
            println!("{}", style("Stable Pools").bold().green());
            for quote in &quotes.stable {
                Self::display_quote(quote, false);
            }
        }

        if !quotes.clmm.is_empty() {
            println!("{}", style("CLMM Pools").bold().yellow());
            for quote in &quotes.clmm {
                Self::display_quote(quote, false);
            }
        }

        if !quotes.standard.is_empty() {
            println!("{}", style("Standard Pools").bold().white());
            for quote in &quotes.standard {
                Self::display_quote(quote, false);
            }
        }
    }

    /// Display detailed information about a single pool
    pub fn display_pool_detailed(pool: &PoolInfo) {
        let pool_type_str = match pool.pool_type {
            PoolType::AMM => "AMM Pool".bright_green(),
            PoolType::Stable => "Stable Pool".bright_blue(),
            PoolType::CLMM => "CLMM Pool".bright_magenta(),
            PoolType::Standard => "Standard Pool".white(),
        };

        println!("{}", pool_type_str.bold());
        println!("Address: {}", style(pool.address.to_string()).dim());
        println!("Tokens: {} / {}", pool.token_a.symbol, pool.token_b.symbol);
        println!("Liquidity: ${:.2}", pool.liquidity_usd);
        println!("Volume 24h: ${:.2}", pool.volume_24h_usd);
        println!("Fee Rate: {:.2}%", pool.fee_rate * 100.0);
        
        // Display pool-specific state
        match &pool.pool_state {
            PoolState::AMM { reserve_a, reserve_b, .. } => {
                println!("Reserves: {} {} / {} {}", 
                    format_amount(*reserve_a, &pool.token_a),
                    pool.token_a.symbol,
                    format_amount(*reserve_b, &pool.token_b),
                    pool.token_b.symbol
                );
            }
            PoolState::Stable { reserves, amp_factor } => {
                println!("Reserves: {} {} / {} {}", 
                    format_amount(*reserves.get(0).unwrap_or(&0), &pool.token_a),
                    pool.token_a.symbol,
                    format_amount(*reserves.get(1).unwrap_or(&0), &pool.token_b),
                    pool.token_b.symbol
                );
                println!("Amplification Factor: {}", amp_factor);
            }
            PoolState::CLMM { current_tick, tick_spacing, liquidity, fee_tier } => {
                println!("Current Tick: {}", current_tick);
                println!("Tick Spacing: {}", tick_spacing);
                println!("Liquidity: {}", liquidity);
                println!("Fee Tier: {} bps", fee_tier);
            }
            PoolState::Standard { reserve_a, reserve_b } => {
                println!("Reserves: {} {} / {} {}", 
                    format_amount(*reserve_a, &pool.token_a),
                    pool.token_a.symbol,
                    format_amount(*reserve_b, &pool.token_b),
                    pool.token_b.symbol
                );
            }
        }
    }

    /// Display pool list
    pub fn display_pool_list(pools: &[PoolInfo], detailed: bool) {
        println!("\n{}", style("🏊 Available Pools").bold().underlined());
        
        for (i, pool) in pools.iter().enumerate() {
            let pool_type_str = format!("{:?}", pool.pool_type);
            let pool_type_colored = match pool.pool_type {
                PoolType::AMM => pool_type_str.blue(),
                PoolType::Stable => pool_type_str.green(),
                PoolType::CLMM => pool_type_str.yellow(),
                PoolType::Standard => pool_type_str.white(),
            };

            println!(
                "{}. {} Pool: {}/{}",
                i + 1,
                pool_type_colored.bold(),
                pool.token_a.symbol,
                pool.token_b.symbol
            );

            if detailed {
                println!("   Address: {}", style(format!("{}", pool.address)).dim());
                println!(
                    "   Liquidity: ${:.2} | Volume 24h: ${:.2}",
                    pool.liquidity_usd, pool.volume_24h_usd
                );
                println!("   Fee: {:.2}%", pool.fee_rate * 100.0);
                
                match &pool.pool_state {
                    crate::core::PoolState::AMM { reserve_a, reserve_b, .. } => {
                        println!(
                            "   Reserves: {} {} | {} {}",
                            format_amount(*reserve_a, &pool.token_a),
                            pool.token_a.symbol,
                            format_amount(*reserve_b, &pool.token_b),
                            pool.token_b.symbol
                        );
                    }
                    crate::core::PoolState::Stable { reserves, amp_factor } => {
                        println!(
                            "   Reserves: {} {} | {} {} | Amp: {}",
                            format_amount(reserves[0], &pool.token_a),
                            pool.token_a.symbol,
                            format_amount(reserves[1], &pool.token_b),
                            pool.token_b.symbol,
                            amp_factor
                        );
                    }
                    crate::core::PoolState::CLMM {
                        current_tick,
                        liquidity,
                        fee_tier,
                        ..
                    } => {
                        println!(
                            "   Tick: {} | Liquidity: {} | Fee Tier: {:.2}%",
                            current_tick,
                            liquidity,
                            *fee_tier as f64 / 10000.0
                        );
                    }
                    crate::core::PoolState::Standard { reserve_a, reserve_b } => {
                        println!(
                            "   Reserves: {} {} | {} {}",
                            format_amount(*reserve_a, &pool.token_a),
                            pool.token_a.symbol,
                            format_amount(*reserve_b, &pool.token_b),
                            pool.token_b.symbol
                        );
                    }
                }
                println!();
            }
        }
    }

    /// Create a progress bar for operations
    pub fn create_progress_bar(message: &str) -> ProgressBar {
        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.cyan} {msg}")
                .unwrap()
                .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈ "),
        );
        pb.set_message(message.to_string());
        pb.enable_steady_tick(std::time::Duration::from_millis(80));
        pb
    }

    /// Display transaction confirmation
    pub fn display_swap_confirmation(quote: &QuoteResult) {
        println!("\n{}", style("💱 Swap Confirmation").bold().underlined());
        println!(
            "Pool Type: {}",
            match quote.pool_info.pool_type {
                PoolType::AMM => "AMM".blue(),
                PoolType::Stable => "Stable".green(),
                PoolType::CLMM => "CLMM".yellow(),
                PoolType::Standard => "Standard".white(),
            }
            .bold()
        );
        
        // Determine input and output tokens based on swap direction
        let (input_token, output_token) = if quote.token_in == quote.pool_info.token_a.mint {
            (&quote.pool_info.token_a, &quote.pool_info.token_b)
        } else {
            (&quote.pool_info.token_b, &quote.pool_info.token_a)
        };
        
        println!(
            "Swap: {} {} → {} {}",
            format_amount(quote.amount_in, input_token),
            input_token.symbol.bold(),
            format_amount(quote.amount_out, output_token),
            output_token.symbol.bold()
        );
        println!(
            "Price Impact: {} | Fee: {} {}",
            format_impact(quote.price_impact),
            format_amount(quote.fee, input_token),
            input_token.symbol
        );
        println!(
            "Min Output: {} {} (with slippage)",
            format_amount(quote.min_amount_out, output_token),
            output_token.symbol
        );
        println!("Pool: {}", style(format!("{}", quote.pool_info.address)).dim());
    }

    /// Display transaction result
    pub fn display_transaction_result(
        signature: &str,
        pool_type: PoolType,
        expected_out: u64,
        actual_out: u64,
        actual_slippage: f64,
        output_token: &crate::core::TokenInfo,
    ) {
        println!("\n{}", style("✅ Transaction Successful!").bold().green());
        println!("Signature: {}", style(signature).dim());
        println!(
            "Pool Type: {}",
            match pool_type {
                PoolType::AMM => "AMM".blue(),
                PoolType::Stable => "Stable".green(),
                PoolType::CLMM => "CLMM".yellow(),
                PoolType::Standard => "Standard".white(),
            }
            .bold()
        );
        
        let slippage_str = format!("{:.3}%", actual_slippage);
        let slippage_colored = if actual_slippage < 0.1 {
            slippage_str.green()
        } else if actual_slippage < 0.5 {
            slippage_str.yellow()
        } else {
            slippage_str.red()
        };
        
        println!(
            "Expected Output: {} {} | Actual Output: {} {} | Slippage: {}",
            format_amount(expected_out, output_token),
            output_token.symbol,
            format_amount(actual_out, output_token),
            output_token.symbol,
            slippage_colored
        );
    }
}

/// Format token amount with decimals
fn format_amount(amount: u64, token_info: &crate::core::TokenInfo) -> String {
    let divisor = 10u64.pow(token_info.decimals as u32);
    let whole = amount / divisor;
    let fraction = amount % divisor;
    
    if fraction == 0 {
        format!("{}", whole)
    } else {
        let fraction_str = format!("{:0width$}", fraction, width = token_info.decimals as usize);
        let trimmed = fraction_str.trim_end_matches('0');
        format!("{}.{}", whole, trimmed)
    }
}

/// Format price impact with color
fn format_impact(impact: f64) -> ColoredString {
    let impact_str = format!("{:.3}%", impact);
    if impact < 0.1 {
        impact_str.green()
    } else if impact < 1.0 {
        impact_str.yellow()
    } else {
        impact_str.red()
    }
}