use crate::core::{
    constants::*, layouts::ClmmPoolState, PoolInfo, PoolState, PoolType, SwapError, SwapResult,
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
use solana_sdk::{commitment_config::CommitmentConfig, pubkey::Pubkey};
use std::sync::Arc;

pub struct ClmmPoolParser {
    rpc_client: Arc<RpcClient>,
}

impl ClmmPoolParser {
    pub fn new(rpc_client: Arc<RpcClient>) -> Self {
        Self { rpc_client }
    }

    /// Parse CLMM pool from account data
    pub async fn parse_pool(
        &self,
        address: Pubkey,
        data: &[u8],
    ) -> SwapResult<Option<PoolInfo>> {
        debug!(
            "Parsing CLMM pool {} with data length {}",
            address,
            data.len()
        );

        // CLMM uses Borsh serialization
        let pool_state = match ClmmPoolState::try_from_slice(data) {
            Ok(state) => state,
            Err(e) => {
                debug!("Failed to deserialize CLMM pool {}: {}", address, e);
                return Ok(None);
            }
        };

        // Check if pool has liquidity
        if pool_state.liquidity == 0 {
            debug!("CLMM pool {} has no liquidity", address);
            return Ok(None);
        }

        // Get token vault balances
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

        let token_0_balance = self.get_token_balance(&token_vault_0).await?;
        let token_1_balance = self.get_token_balance(&token_vault_1).await?;

        // Get token metadata
        let token_0_info = self.get_token_info(&pool_state.token_mint_0).await?;
        let token_1_info = self.get_token_info(&pool_state.token_mint_1).await?;

        // Calculate liquidity in USD
        let liquidity_usd = self.estimate_liquidity_usd(
            token_0_balance,
            token_1_balance,
            &token_0_info,
            &token_1_info,
        );

        // Convert fee rate from CLMM format to percentage
        let fee_rate = pool_state.fee_rate as f64 / 1_000_000.0;

        Ok(Some(PoolInfo {
            pool_type: PoolType::CLMM,
            address,
            token_a: token_0_info,
            token_b: token_1_info,
            liquidity_usd,
            volume_24h_usd: 0.0, // Would need to track swaps or use API
            fee_rate,
            program_id: *CLMM_PROGRAM,
            pool_state: PoolState::CLMM {
                current_tick: pool_state.current_tick,
                tick_spacing: pool_state.tick_spacing,
                liquidity: pool_state.liquidity,
                fee_tier: pool_state.get_fee_rate_bps(),
            },
        }))
    }

    /// Find all CLMM pools for a token pair
    pub async fn find_pools_for_pair(
        &self,
        token_a: Pubkey,
        token_b: Pubkey,
    ) -> SwapResult<Vec<PoolInfo>> {
        debug!("Searching for CLMM pools: {}/{}", token_a, token_b);

        let mut all_pools = Vec::new();

        // CLMM pool state offsets (after Borsh deserialization):
        // bump: 1 byte (offset 0)
        // token_mint_0: 32 bytes (offset 1)
        // token_mint_1: 32 bytes (offset 33)
        
        // Search pattern 1: token_a as token_mint_0, token_b as token_mint_1
        let filters1 = vec![
            RpcFilterType::DataSize(ClmmPoolState::LEN as u64),
            RpcFilterType::Memcmp(Memcmp {
                offset: 1, // token_mint_0 offset (after bump byte)
                bytes: MemcmpEncodedBytes::Base58(token_a.to_string()),
                encoding: None,
            }),
            RpcFilterType::Memcmp(Memcmp {
                offset: 33, // token_mint_1 offset
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
            .get_program_accounts_with_config(&CLMM_PROGRAM, config1)
            .await
        {
            Ok(accounts) => {
                debug!("Found {} CLMM accounts with token_a as token_0", accounts.len());
                for (address, account) in accounts {
                    debug!(
                        "Processing CLMM account {} with data length {}",
                        address,
                        account.data.len()
                    );
                    
                    if let Some(pool) = self.parse_pool(address, &account.data).await? {
                        all_pools.push(pool);
                    }
                }
            }
            Err(e) => {
                warn!("Error searching CLMM pools (pattern 1): {}", e);
            }
        }

        // Search pattern 2: token_b as token_mint_0, token_a as token_mint_1 (reversed)
        let filters2 = vec![
            RpcFilterType::DataSize(ClmmPoolState::LEN as u64),
            RpcFilterType::Memcmp(Memcmp {
                offset: 1, // token_mint_0 offset
                bytes: MemcmpEncodedBytes::Base58(token_b.to_string()),
                encoding: None,
            }),
            RpcFilterType::Memcmp(Memcmp {
                offset: 33, // token_mint_1 offset
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
            .get_program_accounts_with_config(&CLMM_PROGRAM, config2)
            .await
        {
            Ok(accounts) => {
                debug!("Found {} CLMM accounts with token_b as token_0", accounts.len());
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
                warn!("Error searching CLMM pools (pattern 2): {}", e);
            }
        }

        debug!("Found {} total CLMM pools for pair", all_pools.len());
        Ok(all_pools)
    }

    /// Get token balance for an account
    async fn get_token_balance(&self, token_account: &Pubkey) -> SwapResult<u64> {
        match self.rpc_client.get_account(token_account).await {
            Ok(account) => {
                // Parse SPL token account (first 8 bytes after discriminator is amount)
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
            "DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263" => ("BONK", "Bonk", 5),
            "mSoLzYCxHdYgdzU16g5QSh3i5K3z3KZK7ytfqcJm7So" => ("mSOL", "Marinade staked SOL", 9),
            "7vfCXTUXx5WJV5JADk17DUJ4ksgau7utNKj4b963voxs" => ("ETH", "Ether (Portal)", 8),
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
        reserve_0: u64,
        reserve_1: u64,
        token_0: &TokenInfo,
        token_1: &TokenInfo,
    ) -> f64 {
        // In production, use price oracle
        // For now, use simple heuristics
        
        let value_0 = match token_0.symbol.as_str() {
            "SOL" => (reserve_0 as f64 / 10f64.powi(token_0.decimals as i32)) * 40.0,
            "USDC" | "USDT" => reserve_0 as f64 / 10f64.powi(token_0.decimals as i32),
            "ETH" => (reserve_0 as f64 / 10f64.powi(token_0.decimals as i32)) * 2400.0,
            _ => 0.0,
        };

        let value_1 = match token_1.symbol.as_str() {
            "SOL" => (reserve_1 as f64 / 10f64.powi(token_1.decimals as i32)) * 40.0,
            "USDC" | "USDT" => reserve_1 as f64 / 10f64.powi(token_1.decimals as i32),
            "ETH" => (reserve_1 as f64 / 10f64.powi(token_1.decimals as i32)) * 2400.0,
            _ => 0.0,
        };

        value_0 + value_1
    }
}