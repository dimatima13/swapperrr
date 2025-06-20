use clap::{Parser, Subcommand};
use solana_sdk::pubkey::Pubkey;

pub mod commands;
pub mod display;

#[derive(Parser)]
#[command(name = "raydium-swap")]
#[command(about = "Multi-pool DeFi tool for Raydium protocol", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Get swap quotes from all available pool types
    Quote(QuoteArgs),
    
    /// Execute a swap through the best available pool
    Swap(SwapArgs),
    
    /// List all available pools for a token pair
    Pools(PoolsArgs),
}

#[derive(Parser)]
pub struct QuoteArgs {
    /// Input token mint address
    #[arg(value_parser = parse_pubkey)]
    pub token_in: Pubkey,
    
    /// Output token mint address
    #[arg(value_parser = parse_pubkey)]
    pub token_out: Pubkey,
    
    /// Amount to swap (in token units, considering decimals)
    pub amount: f64,
    
    /// Slippage tolerance in basis points (default: 50 = 0.5%)
    #[arg(short, long, default_value = "50")]
    pub slippage: u16,
    
    /// Show quotes from all pools, not just the best
    #[arg(short, long)]
    pub all: bool,
}

#[derive(Parser)]
pub struct SwapArgs {
    /// Input token mint address
    #[arg(value_parser = parse_pubkey)]
    pub token_in: Pubkey,
    
    /// Output token mint address
    #[arg(value_parser = parse_pubkey)]
    pub token_out: Pubkey,
    
    /// Amount to swap (in token units, considering decimals)
    pub amount: f64,
    
    /// Slippage tolerance in basis points (default: 50 = 0.5%)
    #[arg(short, long, default_value = "50")]
    pub slippage: u16,
    
    /// Skip confirmation prompt
    #[arg(long)]
    pub yes: bool,
}

#[derive(Parser)]
pub struct PoolsArgs {
    /// First token mint address
    #[arg(value_parser = parse_pubkey)]
    pub token_a: Pubkey,
    
    /// Second token mint address
    #[arg(value_parser = parse_pubkey)]
    pub token_b: Pubkey,
    
    /// Show detailed pool information
    #[arg(short, long)]
    pub detailed: bool,
}

fn parse_pubkey(s: &str) -> Result<Pubkey, String> {
    s.parse::<Pubkey>()
        .map_err(|e| format!("Invalid pubkey: {}", e))
}