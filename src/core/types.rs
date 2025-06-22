use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PoolType {
    AMM,
    Stable,
    CLMM,
    Standard,
}

impl fmt::Display for PoolType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PoolType::AMM => write!(f, "AMM"),
            PoolType::Stable => write!(f, "Stable"),
            PoolType::CLMM => write!(f, "CLMM"),
            PoolType::Standard => write!(f, "Standard"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolInfo {
    pub pool_type: PoolType,
    pub address: Pubkey,
    pub token_a: TokenInfo,
    pub token_b: TokenInfo,
    pub liquidity_usd: f64,
    pub volume_24h_usd: f64,
    pub fee_rate: f64,
    pub program_id: Pubkey,
    pub pool_state: PoolState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PoolState {
    AMM {
        reserve_a: u64,
        reserve_b: u64,
        nonce: u8,
    },
    Stable {
        reserves: Vec<u64>,
        amp_factor: u64,
    },
    CLMM {
        current_tick: i32,
        tick_spacing: u16,
        liquidity: u128,
        fee_tier: u32,
    },
    Standard {
        reserve_a: u64,
        reserve_b: u64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenInfo {
    pub mint: Pubkey,
    pub symbol: String,
    pub decimals: u8,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuoteRequest {
    pub token_in: Pubkey,
    pub token_out: Pubkey,
    pub amount_in: u64,
    pub slippage_bps: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuoteResult {
    pub pool_info: PoolInfo,
    pub amount_in: u64,
    pub amount_out: u64,
    pub min_amount_out: u64,
    pub price_impact: f64,
    pub fee: u64,
    pub route: Vec<Pubkey>,
    pub token_in: Pubkey,
    pub token_out: Pubkey,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwapParams {
    pub quote: QuoteResult,
    pub user_pubkey: Pubkey,
    pub slippage_bps: u16,
    pub token_in: Pubkey,
    pub token_out: Pubkey,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionResult {
    pub signature: String,
    pub pool_type: PoolType,
    pub pool_address: Pubkey,
    pub amount_in: u64,
    pub amount_out: u64,
    pub expected_amount_out: u64,
    pub actual_slippage: f64,
    pub fee_paid: u64,
    pub timestamp: i64,
    pub retry_attempts: u32,
    pub confirmation_time_ms: u64,
    pub finalized: bool,
    pub transaction_fee: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct PoolScore {
    pub pool: PoolInfo,
    pub score: f64,
    pub liquidity_score: f64,
    pub volume_score: f64,
    pub type_bonus: f64,
}

impl PoolScore {
    pub fn new(pool: PoolInfo, liquidity_score: f64, volume_score: f64, type_bonus: f64) -> Self {
        let score = (liquidity_score * 0.6 + volume_score * 0.4) * type_bonus;
        Self {
            pool,
            score,
            liquidity_score,
            volume_score,
            type_bonus,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SwapSide {
    Buy,
    Sell,
}

#[derive(Debug, Clone)]
pub struct MarketConfig {
    pub rpc_url: String,
    pub helius_api_key: Option<String>,
    pub max_retries: u32,
    pub timeout_secs: u64,
}