use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;

// Raydium Program IDs (Mainnet)
pub const RAYDIUM_AMM_V4_PROGRAM_ID: &str = "675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8";
pub const RAYDIUM_CP_SWAP_PROGRAM_ID: &str = "CPMDWBwJDtYax9qW7AyRuVC19Cc4L4Vcy4n2BHAbHkCW";
pub const RAYDIUM_STABLE_PROGRAM_ID: &str = "5quBtoiQqxF9Jv6KYKctB59NT3gtJD2Y65kdnB1Uev3h";
pub const RAYDIUM_CLMM_PROGRAM_ID: &str = "CAMMCzo5YL8w4VFF8KVHrK22GGUsp5VTaW7grrKgrWqK";
pub const RAYDIUM_ROUTING_PROGRAM_ID: &str = "routeUGWgWzqBWFcrCfv8tritsqukccJPu3q5GPP3xS";

// Program IDs as Pubkey
lazy_static::lazy_static! {
    pub static ref AMM_V4_PROGRAM: Pubkey = Pubkey::from_str(RAYDIUM_AMM_V4_PROGRAM_ID).unwrap();
    pub static ref CP_SWAP_PROGRAM: Pubkey = Pubkey::from_str(RAYDIUM_CP_SWAP_PROGRAM_ID).unwrap();
    pub static ref STABLE_PROGRAM: Pubkey = Pubkey::from_str(RAYDIUM_STABLE_PROGRAM_ID).unwrap();
    pub static ref CLMM_PROGRAM: Pubkey = Pubkey::from_str(RAYDIUM_CLMM_PROGRAM_ID).unwrap();
    pub static ref ROUTING_PROGRAM: Pubkey = Pubkey::from_str(RAYDIUM_ROUTING_PROGRAM_ID).unwrap();
}

// Common token addresses
pub const WSOL_MINT: &str = "So11111111111111111111111111111111111111112";
pub const USDC_MINT: &str = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v";
pub const USDT_MINT: &str = "Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB";

// Fee rates by pool type
pub const AMM_FEE_RATE: f64 = 0.0025; // 0.25%
pub const STABLE_FEE_RATE: f64 = 0.0004; // 0.04%
pub const STANDARD_FEE_RATE: f64 = 0.003; // 0.3%

// Cache TTL in seconds
pub const POOL_CACHE_TTL: u64 = 30;
pub const METADATA_CACHE_TTL: u64 = 300;
pub const TOKEN_INFO_CACHE_TTL: u64 = 3600;

// RPC Configuration
pub const DEFAULT_RPC_TIMEOUT: u64 = 30;
pub const MAX_RPC_RETRIES: u32 = 3;

// Transaction Configuration
pub const DEFAULT_SLIPPAGE_BPS: u16 = 50; // 0.5%
pub const MAX_SLIPPAGE_BPS: u16 = 1000; // 10%

// Pool Discovery Configuration
pub const MAX_POOLS_PER_TYPE: usize = 10;
pub const MIN_LIQUIDITY_USD: f64 = 1000.0;