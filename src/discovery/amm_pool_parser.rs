use crate::core::{
    constants::*, layouts::AmmInfoLayoutV4, PoolInfo, PoolState, PoolType, SwapError, SwapResult,
    TokenInfo,
};
use log::{debug, warn};
use solana_account_decoder::UiAccountEncoding;
use solana_client::{
    nonblocking::rpc_client::RpcClient,
    rpc_config::{RpcAccountInfoConfig, RpcProgramAccountsConfig},
    rpc_filter::{Memcmp, MemcmpEncodedBytes, RpcFilterType},
};
use solana_sdk::{commitment_config::CommitmentConfig, pubkey::Pubkey};
use std::sync::Arc;

pub struct AmmPoolParser {
    rpc_client: Arc<RpcClient>,
}

impl AmmPoolParser {
    pub fn new(rpc_client: Arc<RpcClient>) -> Self {
        Self { rpc_client }
    }

    /// Parse AMM pool from account data
    pub async fn parse_pool(
        &self,
        address: Pubkey,
        data: &[u8],
    ) -> SwapResult<Option<PoolInfo>> {
        debug!(
            "Parsing AMM pool {} with data length {}",
            address,
            data.len()
        );
        
        // Check data length
        if data.len() != AmmInfoLayoutV4::LEN {
            debug!(
                "Invalid AMM pool data length: {} (expected {})",
                data.len(),
                AmmInfoLayoutV4::LEN
            );
            return Ok(None);
        }

        // Parse pool state from raw bytes
        let pool_state = match AmmInfoLayoutV4::from_bytes(data) {
            Ok(state) => state,
            Err(e) => {
                debug!("Failed to parse AMM pool {}: {}", address, e);
                // Log first few fields for debugging
                if data.len() >= 64 {
                    debug!("  First 8 u64 values:");
                    for i in 0..8 {
                        let value = u64::from_le_bytes(data[i*8..(i+1)*8].try_into().unwrap());
                        debug!("    [{}] = {}", i, value);
                    }
                }
                return Ok(None);
            }
        };

        // Check if pool is enabled
        debug!("AMM pool {} status: {}", address, pool_state.status);
        if !pool_state.is_enabled() {
            debug!("AMM pool {} is not enabled (status != 1)", address);
            return Ok(None);
        }

        // Get token reserves
        // Use vault addresses from the pool state
        let token_a_balance = self.get_token_balance(&pool_state.pool_coin_token_account).await?;
        let token_b_balance = self.get_token_balance(&pool_state.pool_pc_token_account).await?;

        // Get token metadata
        let token_a_info = self.get_token_info(&pool_state.coin_mint_address).await?;
        let token_b_info = self.get_token_info(&pool_state.pc_mint_address).await?;

        // Calculate liquidity in USD (simplified - would need price oracle in production)
        let liquidity_usd = self.estimate_liquidity_usd(
            token_a_balance,
            token_b_balance,
            &token_a_info,
            &token_b_info,
        );

        Ok(Some(PoolInfo {
            pool_type: PoolType::AMM,
            address,
            token_a: token_a_info,
            token_b: token_b_info,
            liquidity_usd,
            volume_24h_usd: 0.0, // Would need to track swaps or use API
            fee_rate: pool_state.get_swap_fee_rate(),
            program_id: *AMM_V4_PROGRAM,
            pool_state: PoolState::AMM {
                reserve_a: token_a_balance,
                reserve_b: token_b_balance,
            },
        }))
    }

    /// Find all AMM pools for a token pair
    pub async fn find_pools_for_pair(
        &self,
        token_a: Pubkey,
        token_b: Pubkey,
    ) -> SwapResult<Vec<PoolInfo>> {
        debug!("Searching for AMM pools: {}/{}", token_a, token_b);

        // AMM pool layout offsets:
        // coin_mint_address: offset 400
        // pc_mint_address: offset 432
        
        let mut all_pools = Vec::new();

        // Search pattern 1: token_a as coin, token_b as pc
        let filters1 = vec![
            RpcFilterType::DataSize(AmmInfoLayoutV4::LEN as u64),
            RpcFilterType::Memcmp(Memcmp {
                offset: 400, // coin_mint_address offset
                bytes: MemcmpEncodedBytes::Base58(token_a.to_string()),
                encoding: None,
            }),
            RpcFilterType::Memcmp(Memcmp {
                offset: 432, // pc_mint_address offset
                bytes: MemcmpEncodedBytes::Base58(token_b.to_string()),
                encoding: None,
            }),
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
            .get_program_accounts_with_config(&AMM_V4_PROGRAM, config1)
            .await
        {
            Ok(accounts) => {
                debug!("Found {} AMM accounts with token_a as coin", accounts.len());
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
                warn!("Error searching AMM pools (pattern 1): {}", e);
            }
        }

        // Search pattern 2: token_b as coin, token_a as pc (reversed)
        let filters2 = vec![
            RpcFilterType::DataSize(AmmInfoLayoutV4::LEN as u64),
            RpcFilterType::Memcmp(Memcmp {
                offset: 400, // coin_mint_address offset
                bytes: MemcmpEncodedBytes::Base58(token_b.to_string()),
                encoding: None,
            }),
            RpcFilterType::Memcmp(Memcmp {
                offset: 432, // pc_mint_address offset
                bytes: MemcmpEncodedBytes::Base58(token_a.to_string()),
                encoding: None,
            }),
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
            .get_program_accounts_with_config(&AMM_V4_PROGRAM, config2)
            .await
        {
            Ok(accounts) => {
                debug!("Found {} AMM accounts with token_b as coin", accounts.len());
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
                warn!("Error searching AMM pools (pattern 2): {}", e);
            }
        }

        debug!("Found {} total AMM pools for pair", all_pools.len());
        Ok(all_pools)
    }

    /// Get token balance for an account
    async fn get_token_balance(&self, token_account: &Pubkey) -> SwapResult<u64> {
        match self.rpc_client.get_account(token_account).await {
            Ok(account) => {
                // Parse SPL token account (first 8 bytes is amount)
                if account.data.len() >= 165 {
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
            "DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263" => ("BONK", "Bonk", 5),
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

    /// Estimate liquidity in USD (simplified)
    fn estimate_liquidity_usd(
        &self,
        reserve_a: u64,
        reserve_b: u64,
        token_a: &TokenInfo,
        token_b: &TokenInfo,
    ) -> f64 {
        // In production, use price oracle
        // For now, use simple heuristics
        
        let value_a = match token_a.symbol.as_str() {
            "SOL" => (reserve_a as f64 / 10f64.powi(token_a.decimals as i32)) * 40.0,
            "USDC" | "USDT" => reserve_a as f64 / 10f64.powi(token_a.decimals as i32),
            "BONK" => (reserve_a as f64 / 10f64.powi(token_a.decimals as i32)) * 0.00002,
            _ => 0.0,
        };

        let value_b = match token_b.symbol.as_str() {
            "SOL" => (reserve_b as f64 / 10f64.powi(token_b.decimals as i32)) * 40.0,
            "USDC" | "USDT" => reserve_b as f64 / 10f64.powi(token_b.decimals as i32),
            "BONK" => (reserve_b as f64 / 10f64.powi(token_b.decimals as i32)) * 0.00002,
            _ => 0.0,
        };

        value_a + value_b
    }
    
    /// Get AMM vault addresses using PDA
    fn get_amm_vault_addresses(&self, pool_address: &Pubkey, mint: &Pubkey) -> (Pubkey, u8) {
        // For Raydium AMM V4, vaults are associated token accounts
        // owned by the pool's authority PDA
        let (authority, _nonce) = Pubkey::find_program_address(
            &[pool_address.as_ref()],
            &AMM_V4_PROGRAM,
        );
        
        // Get associated token account for the authority
        let vault = spl_associated_token_account::get_associated_token_address(
            &authority,
            mint,
        );
        
        (vault, _nonce)
    }
}