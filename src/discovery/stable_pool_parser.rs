use crate::core::{
    constants::*, layouts::StablePoolState, PoolInfo, PoolState, PoolType, SwapError, SwapResult,
    TokenInfo,
};
use borsh::BorshDeserialize;
use log::{debug, warn};
use solana_account_decoder::UiAccountEncoding;
use solana_client::{
    nonblocking::rpc_client::RpcClient,
    rpc_config::{RpcAccountInfoConfig, RpcProgramAccountsConfig},
    rpc_filter::{Memcmp, MemcmpEncodedBytes, RpcFilterType},
};
use solana_sdk::{commitment_config::CommitmentConfig, pubkey::Pubkey, clock::Clock, sysvar};
use std::sync::Arc;

pub struct StablePoolParser {
    rpc_client: Arc<RpcClient>,
}

impl StablePoolParser {
    pub fn new(rpc_client: Arc<RpcClient>) -> Self {
        Self { rpc_client }
    }

    /// Parse Stable pool from account data
    pub async fn parse_pool(
        &self,
        address: Pubkey,
        data: &[u8],
    ) -> SwapResult<Option<PoolInfo>> {
        debug!(
            "Parsing Stable pool {} with data length {}",
            address,
            data.len()
        );

        // Parse pool state using Borsh deserialization
        let pool_state = match StablePoolState::try_from_slice(data) {
            Ok(state) => state,
            Err(e) => {
                debug!("Failed to parse Stable pool {}: {}", address, e);
                return Ok(None);
            }
        };

        // Check if pool is initialized
        if !pool_state.is_initialized {
            debug!("Stable pool {} is not initialized", address);
            return Ok(None);
        }

        // Check if pool is not paused
        if pool_state.is_paused {
            debug!("Stable pool {} is paused", address);
            return Ok(None);
        }

        // Get token reserves from token accounts
        let token_a_balance = self.get_token_balance(&pool_state.token_a_account).await?;
        let token_b_balance = self.get_token_balance(&pool_state.token_b_account).await?;

        // Skip pools with no liquidity
        if token_a_balance == 0 || token_b_balance == 0 {
            debug!("Stable pool {} has no liquidity", address);
            return Ok(None);
        }

        // Get token metadata
        let token_a_info = self.get_token_info(&pool_state.token_mint_a).await?;
        let token_b_info = self.get_token_info(&pool_state.token_mint_b).await?;

        // Get current timestamp for amp calculation
        let current_timestamp = self.get_current_timestamp().await?;
        let current_amp = pool_state.get_current_amp(current_timestamp);

        // Calculate liquidity in USD (simplified)
        let liquidity_usd = self.estimate_liquidity_usd(
            token_a_balance,
            token_b_balance,
            &token_a_info,
            &token_b_info,
        );

        Ok(Some(PoolInfo {
            pool_type: PoolType::Stable,
            address,
            token_a: token_a_info,
            token_b: token_b_info,
            liquidity_usd,
            volume_24h_usd: 0.0, // Would need to track swaps or use API
            fee_rate: pool_state.get_trade_fee_rate(),
            program_id: *STABLE_PROGRAM,
            pool_state: PoolState::Stable {
                reserves: vec![token_a_balance, token_b_balance],
                amp_factor: current_amp,
            },
        }))
    }

    /// Find all Stable pools for a token pair
    pub async fn find_pools_for_pair(
        &self,
        token_a: Pubkey,
        token_b: Pubkey,
    ) -> SwapResult<Vec<PoolInfo>> {
        debug!("Searching for Stable pools: {}/{}", token_a, token_b);

        // Calculate offsets for token_mint_a and token_mint_b in StablePoolState
        // StablePoolState fields (Borsh serialized):
        // - is_initialized: bool (1 byte)
        // - is_paused: bool (1 byte)
        // - nonce: u8 (1 byte)
        // - initial_amp_factor: u64 (8 bytes)
        // - target_amp_factor: u64 (8 bytes)
        // - start_ramp_timestamp: i64 (8 bytes)
        // - stop_ramp_timestamp: i64 (8 bytes)
        // - future_admin_deadline: i64 (8 bytes)
        // - future_admin_account: Pubkey (32 bytes)
        // - admin_account: Pubkey (32 bytes)
        // - token_mint_a: Pubkey (32 bytes) - offset: 107
        // - token_mint_b: Pubkey (32 bytes) - offset: 139
        
        const TOKEN_MINT_A_OFFSET: usize = 107;
        const TOKEN_MINT_B_OFFSET: usize = 139;
        
        let mut all_pools = Vec::new();

        // Search pattern 1: token_a as mint_a, token_b as mint_b
        let filters1 = vec![
            RpcFilterType::Memcmp(Memcmp::new_raw_bytes(
                TOKEN_MINT_A_OFFSET,
                token_a.to_bytes().to_vec(),
            )),
            RpcFilterType::Memcmp(Memcmp::new_raw_bytes(
                TOKEN_MINT_B_OFFSET,
                token_b.to_bytes().to_vec(),
            )),
        ];

        let config1 = RpcProgramAccountsConfig {
            filters: Some(filters1),
            account_config: RpcAccountInfoConfig {
                encoding: Some(UiAccountEncoding::Base64Zstd),
                commitment: Some(CommitmentConfig::confirmed()),
                ..Default::default()
            },
            ..Default::default()
        };

        match self
            .rpc_client
            .get_program_accounts_with_config(&STABLE_PROGRAM, config1)
            .await
        {
            Ok(accounts) => {
                debug!("Found {} Stable accounts with token_a as mint_a", accounts.len());
                for (address, account) in accounts {
                    debug!(
                        "Processing account {} with data length {}",
                        address,
                        account.data.len()
                    );
                    
                    if let Some(pool) = self.parse_pool(address, &account.data).await? {
                        all_pools.push(pool);
                    }
                }
            }
            Err(e) => {
                warn!("Error searching Stable pools (pattern 1): {}", e);
            }
        }

        // Search pattern 2: token_b as mint_a, token_a as mint_b (reversed)
        let filters2 = vec![
            RpcFilterType::Memcmp(Memcmp::new_raw_bytes(
                TOKEN_MINT_A_OFFSET,
                token_b.to_bytes().to_vec(),
            )),
            RpcFilterType::Memcmp(Memcmp::new_raw_bytes(
                TOKEN_MINT_B_OFFSET,
                token_a.to_bytes().to_vec(),
            )),
        ];

        let config2 = RpcProgramAccountsConfig {
            filters: Some(filters2),
            account_config: RpcAccountInfoConfig {
                encoding: Some(UiAccountEncoding::Base64Zstd),
                commitment: Some(CommitmentConfig::confirmed()),
                ..Default::default()
            },
            ..Default::default()
        };

        match self
            .rpc_client
            .get_program_accounts_with_config(&STABLE_PROGRAM, config2)
            .await
        {
            Ok(accounts) => {
                debug!("Found {} Stable accounts with token_b as mint_a", accounts.len());
                for (address, account) in accounts {
                    if let Some(pool) = self.parse_pool(address, &account.data).await? {
                        // Check we don't have duplicates
                        if !all_pools.iter().any(|p| p.address == address) {
                            all_pools.push(pool);
                        }
                    }
                }
            }
            Err(e) => {
                warn!("Error searching Stable pools (pattern 2): {}", e);
            }
        }

        debug!("Found {} total Stable pools for pair", all_pools.len());
        Ok(all_pools)
    }

    /// Get token balance for an account
    async fn get_token_balance(&self, token_account: &Pubkey) -> SwapResult<u64> {
        match self.rpc_client.get_account(token_account).await {
            Ok(account) => {
                // Parse SPL token account (first 8 bytes is mint, next 8 bytes is owner, then 8 bytes is amount)
                if account.data.len() >= 72 {
                    // SPL Token account layout: amount is at offset 64
                    Ok(u64::from_le_bytes(
                        account.data[64..72].try_into().unwrap(),
                    ))
                } else {
                    debug!("Invalid token account data length: {}", account.data.len());
                    Ok(0)
                }
            }
            Err(e) => {
                debug!("Failed to get token balance for {}: {}", token_account, e);
                Ok(0) // Return 0 balance if account not found
            }
        }
    }

    /// Get token metadata
    async fn get_token_info(&self, mint: &Pubkey) -> SwapResult<TokenInfo> {
        // In production, you would:
        // 1. Query token metadata from Metaplex
        // 2. Cache token info
        // 3. Use a token list API
        
        // For now, return basic info based on known tokens
        let (symbol, name, decimals) = match mint.to_string().as_str() {
            "So11111111111111111111111111111111111111112" => ("SOL", "Wrapped SOL", 9),
            "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v" => ("USDC", "USD Coin", 6),
            "Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB" => ("USDT", "Tether USD", 6),
            "USDH1SM1ojwWUga67PGrgFWUHibbjqMvuMaDkRJTgkX" => ("USDH", "USDH", 6),
            "mSoLzYCxHdYgdzU16g5QSh3i5K3z3KZK7ytfqcJm7So" => ("mSOL", "Marinade staked SOL", 9),
            _ => {
                // For unknown tokens, get decimals from mint account
                let decimals = self.get_token_decimals(mint).await.unwrap_or(9);
                ("UNKNOWN", "Unknown Token", decimals)
            }
        };

        Ok(TokenInfo {
            mint: *mint,
            symbol: symbol.to_string(),
            name: name.to_string(),
            decimals,
        })
    }

    /// Get token decimals from mint account
    async fn get_token_decimals(&self, mint: &Pubkey) -> SwapResult<u8> {
        let account = self
            .rpc_client
            .get_account(mint)
            .await
            .map_err(SwapError::RpcError)?;

        // SPL Token mint layout: decimals at offset 44
        if account.data.len() > 44 {
            Ok(account.data[44])
        } else {
            Ok(9) // Default to 9 decimals
        }
    }

    /// Get current timestamp from the clock sysvar
    async fn get_current_timestamp(&self) -> SwapResult<i64> {
        match self.rpc_client.get_account(&sysvar::clock::id()).await {
            Ok(account) => {
                let clock: Clock = bincode::deserialize(&account.data)
                    .map_err(|e| SwapError::SerializationError(e.to_string()))?;
                Ok(clock.unix_timestamp)
            }
            Err(e) => {
                warn!("Failed to get clock sysvar: {}, using system time", e);
                // Fallback to system time
                Ok(std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as i64)
            }
        }
    }

    /// Estimate liquidity in USD (simplified)
    fn estimate_liquidity_usd(
        &self,
        reserve_a: u64,
        reserve_b: u64,
        token_a: &TokenInfo,
        token_b: &TokenInfo,
    ) -> f64 {
        // In production, use price oracle
        // For now, use simple heuristics for stablecoins
        
        let value_a = match token_a.symbol.as_str() {
            "USDC" | "USDT" | "USDH" => reserve_a as f64 / 10f64.powi(token_a.decimals as i32),
            "SOL" => (reserve_a as f64 / 10f64.powi(token_a.decimals as i32)) * 40.0,
            "mSOL" => (reserve_a as f64 / 10f64.powi(token_a.decimals as i32)) * 42.0,
            _ => 0.0,
        };

        let value_b = match token_b.symbol.as_str() {
            "USDC" | "USDT" | "USDH" => reserve_b as f64 / 10f64.powi(token_b.decimals as i32),
            "SOL" => (reserve_b as f64 / 10f64.powi(token_b.decimals as i32)) * 40.0,
            "mSOL" => (reserve_b as f64 / 10f64.powi(token_b.decimals as i32)) * 42.0,
            _ => 0.0,
        };

        value_a + value_b
    }
}