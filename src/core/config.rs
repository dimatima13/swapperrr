use crate::core::{constants::*, error::SwapResult, SwapError};
use serde::{Deserialize, Serialize};
use std::env;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub rpc_url: String,
    pub helius_api_key: Option<String>,
    pub max_retries: u32,
    pub timeout_secs: u64,
    pub default_slippage_bps: u16,
    pub max_slippage_bps: u16,
    pub cache_ttl_secs: u64,
    pub max_pools_per_type: usize,
    pub min_liquidity_usd: f64,
}

impl Config {
    pub fn from_env() -> SwapResult<Self> {
        let rpc_url = env::var("RPC_URL")
            .or_else(|_| env::var("HELIUS_RPC_URL"))
            .unwrap_or_else(|_| {
                if let Ok(api_key) = env::var("HELIUS_API_KEY") {
                    format!("https://mainnet.helius-rpc.com/?api-key={}", api_key)
                } else {
                    "https://api.mainnet-beta.solana.com".to_string()
                }
            });

        let helius_api_key = env::var("HELIUS_API_KEY").ok();

        Ok(Self {
            rpc_url,
            helius_api_key,
            max_retries: env::var("MAX_RETRIES")
                .unwrap_or_default()
                .parse()
                .unwrap_or(MAX_RPC_RETRIES),
            timeout_secs: env::var("TIMEOUT_SECS")
                .unwrap_or_default()
                .parse()
                .unwrap_or(DEFAULT_RPC_TIMEOUT),
            default_slippage_bps: env::var("DEFAULT_SLIPPAGE_BPS")
                .unwrap_or_default()
                .parse()
                .unwrap_or(DEFAULT_SLIPPAGE_BPS),
            max_slippage_bps: env::var("MAX_SLIPPAGE_BPS")
                .unwrap_or_default()
                .parse()
                .unwrap_or(MAX_SLIPPAGE_BPS),
            cache_ttl_secs: env::var("CACHE_TTL_SECS")
                .unwrap_or_default()
                .parse()
                .unwrap_or(POOL_CACHE_TTL),
            max_pools_per_type: env::var("MAX_POOLS_PER_TYPE")
                .unwrap_or_default()
                .parse()
                .unwrap_or(MAX_POOLS_PER_TYPE),
            min_liquidity_usd: env::var("MIN_LIQUIDITY_USD")
                .unwrap_or_default()
                .parse()
                .unwrap_or(MIN_LIQUIDITY_USD),
        })
    }

    pub fn validate(&self) -> SwapResult<()> {
        if self.max_slippage_bps > 10000 {
            return Err(SwapError::ConfigError(
                "Max slippage cannot exceed 100%".to_string(),
            ));
        }

        if self.default_slippage_bps > self.max_slippage_bps {
            return Err(SwapError::ConfigError(
                "Default slippage cannot exceed max slippage".to_string(),
            ));
        }

        if self.timeout_secs == 0 {
            return Err(SwapError::ConfigError(
                "Timeout must be greater than 0".to_string(),
            ));
        }

        Ok(())
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            rpc_url: "https://api.mainnet-beta.solana.com".to_string(),
            helius_api_key: None,
            max_retries: MAX_RPC_RETRIES,
            timeout_secs: DEFAULT_RPC_TIMEOUT,
            default_slippage_bps: DEFAULT_SLIPPAGE_BPS,
            max_slippage_bps: MAX_SLIPPAGE_BPS,
            cache_ttl_secs: POOL_CACHE_TTL,
            max_pools_per_type: MAX_POOLS_PER_TYPE,
            min_liquidity_usd: MIN_LIQUIDITY_USD,
        }
    }
}