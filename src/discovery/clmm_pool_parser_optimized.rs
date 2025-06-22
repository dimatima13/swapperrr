use crate::core::{
    constants::*, layouts::ClmmPoolState, PoolInfo, PoolState, PoolType, SwapError, SwapResult,
    TokenInfo,
};
use borsh::BorshDeserialize;
use dashmap::DashMap;
use log::{debug, info, warn};
use solana_account_decoder::UiAccountEncoding;
use solana_client::{
    nonblocking::rpc_client::RpcClient,
    rpc_config::{RpcAccountInfoConfig, RpcProgramAccountsConfig},
    rpc_filter::RpcFilterType,
};
use solana_sdk::{commitment_config::CommitmentConfig, pubkey::Pubkey};
use std::sync::Arc;
use tokio::sync::Semaphore;

/// Optimized CLMM pool parser with improved filtering and caching
pub struct OptimizedClmmPoolParser {
    rpc_client: Arc<RpcClient>,
    rpc_url: String,
    /// Cache for parsed pools to avoid re-parsing
    pool_cache: Arc<DashMap<Pubkey, PoolInfo>>,
    /// Semaphore for controlling concurrent RPC calls
    rpc_semaphore: Arc<Semaphore>,
    /// Cache for token metadata
    token_cache: Arc<DashMap<Pubkey, TokenInfo>>,
}

impl OptimizedClmmPoolParser {
    pub fn new(rpc_client: Arc<RpcClient>, rpc_url: String) -> Self {
        Self {
            rpc_client,
            rpc_url,
            pool_cache: Arc::new(DashMap::new()),
            rpc_semaphore: Arc::new(Semaphore::new(10)), // Limit concurrent RPC calls
            token_cache: Arc::new(DashMap::new()),
        }
    }

    /// Find all CLMM pools for a token pair with optimized filtering
    pub async fn find_pools_for_pair(
        &self,
        token_a: Pubkey,
        token_b: Pubkey,
    ) -> SwapResult<Vec<PoolInfo>> {
        info!("Searching for CLMM pools: {}/{} (optimized)", token_a, token_b);

        // Check cache first
        let _cache_key_1 = format!("{}:{}", token_a, token_b);
        let _cache_key_2 = format!("{}:{}", token_b, token_a);
        
        let mut cached_pools = Vec::new();
        for entry in self.pool_cache.iter() {
            let pool = entry.value();
            if (pool.token_a.mint == token_a && pool.token_b.mint == token_b) ||
               (pool.token_a.mint == token_b && pool.token_b.mint == token_a) {
                cached_pools.push(pool.clone());
            }
        }
        
        if !cached_pools.is_empty() {
            debug!("Found {} cached CLMM pools", cached_pools.len());
            return Ok(cached_pools);
        }

        // Fetch pools in parallel
        let (pools1, pools2) = tokio::join!(
            self.fetch_pools_pattern(token_a, token_b, false),
            self.fetch_pools_pattern(token_b, token_a, true)
        );

        let mut all_pools = Vec::new();
        
        // Process first pattern results
        match pools1 {
            Ok(pools) => {
                debug!("Found {} CLMM pools (pattern 1)", pools.len());
                all_pools.extend(pools);
            }
            Err(e) => warn!("Error fetching CLMM pools (pattern 1): {}", e),
        }

        // Process second pattern results
        match pools2 {
            Ok(pools) => {
                debug!("Found {} CLMM pools (pattern 2)", pools.len());
                for pool in pools {
                    // Avoid duplicates
                    if !all_pools.iter().any(|p| p.address == pool.address) {
                        all_pools.push(pool);
                    }
                }
            }
            Err(e) => warn!("Error fetching CLMM pools (pattern 2): {}", e),
        }

        // Sort pools by liquidity (highest first)
        all_pools.sort_by(|a, b| b.liquidity_usd.partial_cmp(&a.liquidity_usd).unwrap());
        
        info!("Found {} total CLMM pools for pair", all_pools.len());
        Ok(all_pools)
    }

    /// Fetch pools with a specific token order pattern
    async fn fetch_pools_pattern(
        &self,
        token_0: Pubkey,
        token_1: Pubkey,
        _reversed: bool,
    ) -> SwapResult<Vec<PoolInfo>> {
        // More lenient filters - only filter by data size initially
        let filters = vec![
            // Data size filter only
            RpcFilterType::DataSize(ClmmPoolState::LEN as u64),
        ];

        let config = RpcProgramAccountsConfig {
            filters: Some(filters),
            account_config: RpcAccountInfoConfig {
                encoding: Some(UiAccountEncoding::Base64Zstd), // Use compression
                commitment: Some(CommitmentConfig::confirmed()),
                data_slice: None, // Get full data but compressed
                min_context_slot: None,
            },
            with_context: Some(false),
        };

        // Rate limiting
        let _permit = self.rpc_semaphore.acquire().await
            .map_err(|_| SwapError::Other("Failed to acquire RPC semaphore".to_string()))?;

        let accounts = self
            .rpc_client
            .get_program_accounts_with_config(&CLMM_PROGRAM, config)
            .await
            .map_err(SwapError::RpcError)?;

        info!("Fetched {} CLMM accounts to check for tokens {}/{}", accounts.len(), token_0, token_1);

        // Parse accounts in parallel with limited concurrency
        let mut pools = Vec::new();
        let chunk_size = 5; // Process in chunks to avoid overwhelming the system

        for chunk in accounts.chunks(chunk_size) {
            let mut chunk_futures = Vec::new();
            
            for (address, account) in chunk {
                let parser = self.clone();
                let address = *address;
                let data = account.data.clone();
                let target_token_0 = token_0;
                let target_token_1 = token_1;
                
                chunk_futures.push(tokio::spawn(async move {
                    match parser.parse_pool_optimized(address, &data).await {
                        Ok(Some(pool)) => {
                            // Filter by tokens after parsing
                            if (pool.token_a.mint == target_token_0 && pool.token_b.mint == target_token_1) ||
                               (pool.token_a.mint == target_token_1 && pool.token_b.mint == target_token_0) {
                                Some(pool)
                            } else {
                                None
                            }
                        },
                        _ => None,
                    }
                }));
            }

            // Wait for chunk to complete
            for future in chunk_futures {
                if let Ok(Some(pool)) = future.await {
                    pools.push(pool);
                }
            }
        }

        info!("Filtered to {} CLMM pools matching token pair", pools.len());
        Ok(pools)
    }

    /// Parse CLMM pool with optimizations
    async fn parse_pool_optimized(
        &self,
        address: Pubkey,
        data: &[u8],
    ) -> SwapResult<Option<PoolInfo>> {
        // Check cache first
        if let Some(pool) = self.pool_cache.get(&address) {
            return Ok(Some(pool.clone()));
        }

        debug!("Parsing CLMM pool {} (optimized)", address);

        // Quick validation before deserialization
        if data.len() != ClmmPoolState::LEN {
            debug!("Invalid CLMM pool data length: {} (expected {})", data.len(), ClmmPoolState::LEN);
            return Ok(None);
        }

        // Deserialize pool state
        let pool_state = match ClmmPoolState::try_from_slice(data) {
            Ok(state) => state,
            Err(e) => {
                debug!("Failed to deserialize CLMM pool {}: {}", address, e);
                return Ok(None);
            }
        };

        // Quick liquidity check
        if pool_state.liquidity == 0 {
            debug!("CLMM pool {} has no liquidity", address);
            return Ok(None);
        }
        
        // Skip pools with very low liquidity (less than $10 worth)
        // This is a rough estimate based on liquidity value
        if pool_state.liquidity < 1000000 {
            debug!("CLMM pool {} has very low liquidity: {}", address, pool_state.liquidity);
            return Ok(None);
        }

        // Filter out pools with extreme tick spacing (likely test pools)
        if pool_state.tick_spacing > 200 {
            debug!("CLMM pool {} has unusual tick spacing: {}", address, pool_state.tick_spacing);
            return Ok(None);
        }

        // Get token metadata (with caching)
        let (token_0_info, token_1_info) = tokio::join!(
            self.get_token_info_cached(&pool_state.token_mint_0),
            self.get_token_info_cached(&pool_state.token_mint_1)
        );
        let token_0_info = token_0_info?;
        let token_1_info = token_1_info?;

        // Calculate vault addresses
        let (token_vault_0, _) = Pubkey::find_program_address(
            &[
                b"pool_vault",
                address.as_ref(),
                pool_state.token_mint_0.as_ref(),
            ],
            &CLMM_PROGRAM,
        );
        let (token_vault_1, _) = Pubkey::find_program_address(
            &[
                b"pool_vault",
                address.as_ref(),
                pool_state.token_mint_1.as_ref(),
            ],
            &CLMM_PROGRAM,
        );

        // Get vault balances in parallel
        let (balance_0, balance_1) = tokio::join!(
            self.get_token_balance(&token_vault_0),
            self.get_token_balance(&token_vault_1)
        );
        let token_0_balance = balance_0?;
        let token_1_balance = balance_1?;

        // Estimate liquidity
        let liquidity_usd = self.estimate_liquidity_usd(
            token_0_balance,
            token_1_balance,
            &token_0_info,
            &token_1_info,
        );

        // Filter out pools with very low liquidity
        if liquidity_usd < 300.0 {
            debug!("CLMM pool {} has low liquidity: ${}", address, liquidity_usd);
            return Ok(None);
        }

        // Convert fee rate
        let fee_rate = pool_state.fee_rate as f64 / 1_000_000.0;

        let pool_info = PoolInfo {
            pool_type: PoolType::CLMM,
            address,
            token_a: token_0_info,
            token_b: token_1_info,
            liquidity_usd,
            volume_24h_usd: 0.0, // Would need indexer for this
            fee_rate,
            program_id: *CLMM_PROGRAM,
            pool_state: PoolState::CLMM {
                current_tick: pool_state.current_tick,
                tick_spacing: pool_state.tick_spacing,
                liquidity: pool_state.liquidity,
                fee_tier: pool_state.get_fee_rate_bps(),
            },
        };

        // Cache the parsed pool
        self.pool_cache.insert(address, pool_info.clone());

        Ok(Some(pool_info))
    }

    /// Get token info with caching
    async fn get_token_info_cached(&self, mint: &Pubkey) -> SwapResult<TokenInfo> {
        // Check cache first
        if let Some(info) = self.token_cache.get(mint) {
            return Ok(info.clone());
        }

        // Use token metadata fetcher
        let fetcher = crate::core::token_metadata::TokenMetadataFetcher::new(
            self.rpc_url.clone()
        );
        
        match fetcher.get_token_metadata(mint) {
            Ok(info) => {
                self.token_cache.insert(*mint, info.clone());
                Ok(info)
            }
            Err(_) => {
                // Fallback to basic info
                self.get_token_info_fallback(mint).await
            }
        }
    }

    /// Fallback token info for known tokens
    async fn get_token_info_fallback(&self, mint: &Pubkey) -> SwapResult<TokenInfo> {
        let (symbol, name, decimals) = match mint.to_string().as_str() {
            "So11111111111111111111111111111111111111112" => ("SOL", "Wrapped SOL", 9),
            "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v" => ("USDC", "USD Coin", 6),
            "Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB" => ("USDT", "Tether USD", 6),
            _ => {
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

    /// Get token balance
    async fn get_token_balance(&self, token_account: &Pubkey) -> SwapResult<u64> {
        let _permit = self.rpc_semaphore.acquire().await
            .map_err(|_| SwapError::Other("Failed to acquire RPC semaphore".to_string()))?;

        match self.rpc_client.get_account(token_account).await {
            Ok(account) => {
                if account.data.len() >= 72 {
                    Ok(u64::from_le_bytes(
                        account.data[64..72].try_into().unwrap(),
                    ))
                } else {
                    Ok(0)
                }
            }
            Err(_) => Ok(0),
        }
    }

    /// Get token decimals
    async fn get_token_decimals(&self, mint: &Pubkey) -> SwapResult<u8> {
        let _permit = self.rpc_semaphore.acquire().await
            .map_err(|_| SwapError::Other("Failed to acquire RPC semaphore".to_string()))?;

        let account = self
            .rpc_client
            .get_account(mint)
            .await
            .map_err(SwapError::RpcError)?;

        if account.data.len() > 44 {
            Ok(account.data[44])
        } else {
            Ok(9)
        }
    }

    /// Estimate liquidity in USD
    fn estimate_liquidity_usd(
        &self,
        reserve_0: u64,
        reserve_1: u64,
        token_0: &TokenInfo,
        token_1: &TokenInfo,
    ) -> f64 {
        // In production, use price oracle
        let value_0 = match token_0.symbol.as_str() {
            "SOL" => (reserve_0 as f64 / 10f64.powi(token_0.decimals as i32)) * 140.0,
            "USDC" | "USDT" => reserve_0 as f64 / 10f64.powi(token_0.decimals as i32),
            "ETH" => (reserve_0 as f64 / 10f64.powi(token_0.decimals as i32)) * 3000.0,
            _ => 0.0,
        };

        let value_1 = match token_1.symbol.as_str() {
            "SOL" => (reserve_1 as f64 / 10f64.powi(token_1.decimals as i32)) * 140.0,
            "USDC" | "USDT" => reserve_1 as f64 / 10f64.powi(token_1.decimals as i32),
            "ETH" => (reserve_1 as f64 / 10f64.powi(token_1.decimals as i32)) * 3000.0,
            _ => 0.0,
        };

        value_0 + value_1
    }
}

impl Clone for OptimizedClmmPoolParser {
    fn clone(&self) -> Self {
        Self {
            rpc_client: self.rpc_client.clone(),
            rpc_url: self.rpc_url.clone(),
            pool_cache: self.pool_cache.clone(),
            rpc_semaphore: self.rpc_semaphore.clone(),
            token_cache: self.token_cache.clone(),
        }
    }
}

/// Additional optimization utilities
pub mod optimization_utils {
    use super::*;

    /// Pre-warm cache with known popular CLMM pools
    pub async fn prewarm_cache(parser: &OptimizedClmmPoolParser, popular_tokens: &[Pubkey]) {
        info!("Pre-warming CLMM pool cache with {} tokens", popular_tokens.len());
        
        let mut tasks = Vec::new();
        
        // Create all token pair combinations
        for i in 0..popular_tokens.len() {
            for j in (i + 1)..popular_tokens.len() {
                let parser = parser.clone();
                let token_a = popular_tokens[i];
                let token_b = popular_tokens[j];
                
                tasks.push(tokio::spawn(async move {
                    let _ = parser.find_pools_for_pair(token_a, token_b).await;
                }));
            }
        }

        // Wait for all tasks to complete
        for task in tasks {
            let _ = task.await;
        }
        
        info!("CLMM pool cache pre-warming complete");
    }

    /// Clear stale entries from cache
    pub fn clean_cache(parser: &OptimizedClmmPoolParser, _max_age_secs: u64) {
        // In a real implementation, you'd track timestamps and remove old entries
        let cache_size = parser.pool_cache.len();
        if cache_size > 1000 {
            // Simple size-based cleanup for now
            parser.pool_cache.clear();
            info!("Cleared CLMM pool cache ({} entries)", cache_size);
        }
    }
}