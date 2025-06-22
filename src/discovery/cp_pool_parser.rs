use crate::core::{
    constants::*, layouts::CpSwapPoolState, PoolInfo, PoolState, PoolType, SwapResult,
    TokenInfo, AsyncTokenMetadataFetcher,
};
use log::{debug, warn};
use solana_account_decoder::UiAccountEncoding;
use solana_client::{
    nonblocking::rpc_client::RpcClient,
    rpc_config::{RpcAccountInfoConfig, RpcProgramAccountsConfig},
    rpc_filter::{Memcmp, RpcFilterType},
};
use solana_sdk::{commitment_config::CommitmentConfig, pubkey::Pubkey};
use std::sync::Arc;

pub struct CpPoolParser {
    rpc_client: Arc<RpcClient>,
    metadata_fetcher: Arc<AsyncTokenMetadataFetcher>,
}

impl CpPoolParser {
    pub fn new(rpc_client: Arc<RpcClient>) -> Self {
        let metadata_fetcher = Arc::new(AsyncTokenMetadataFetcher::new(rpc_client.clone()));
        Self { 
            rpc_client,
            metadata_fetcher,
        }
    }

    /// Parse CP pool from account data
    pub async fn parse_pool(
        &self,
        address: Pubkey,
        data: &[u8],
    ) -> SwapResult<Option<PoolInfo>> {
        debug!(
            "Parsing CP pool {} with data length {}",
            address,
            data.len()
        );
        
        // Check data length
        if data.len() != CpSwapPoolState::LEN {
            debug!(
                "Invalid CP pool data length: {} (expected {})",
                data.len(),
                CpSwapPoolState::LEN
            );
            return Ok(None);
        }

        // Parse pool state according to CpSwapPoolState layout
        let pool_state = CpSwapPoolState {
            discriminator: data[0..8].try_into().unwrap(),
            amm_config: Pubkey::from(<[u8; 32]>::try_from(&data[8..40]).unwrap()),
            pool_creator: Pubkey::from(<[u8; 32]>::try_from(&data[40..72]).unwrap()),
            token_0_vault: Pubkey::from(<[u8; 32]>::try_from(&data[72..104]).unwrap()),
            token_1_vault: Pubkey::from(<[u8; 32]>::try_from(&data[104..136]).unwrap()),
            lp_mint: Pubkey::from(<[u8; 32]>::try_from(&data[136..168]).unwrap()),
            token_0_mint: Pubkey::from(<[u8; 32]>::try_from(&data[168..200]).unwrap()),
            token_1_mint: Pubkey::from(<[u8; 32]>::try_from(&data[200..232]).unwrap()),
            token_0_program: Pubkey::from(<[u8; 32]>::try_from(&data[232..264]).unwrap()),
            token_1_program: Pubkey::from(<[u8; 32]>::try_from(&data[264..296]).unwrap()),
            observation_key: Pubkey::from(<[u8; 32]>::try_from(&data[296..328]).unwrap()),
            auth_bump: data[328],
            status: data[329],
            lp_mint_decimals: data[330],
            mint_0_decimals: data[331],
            mint_1_decimals: data[332],
            lp_supply: u64::from_le_bytes(data[333..341].try_into().unwrap()),
            protocol_fees_token_0: u64::from_le_bytes(data[341..349].try_into().unwrap()),
            protocol_fees_token_1: u64::from_le_bytes(data[349..357].try_into().unwrap()),
            fund_fees_token_0: u64::from_le_bytes(data[357..365].try_into().unwrap()),
            fund_fees_token_1: u64::from_le_bytes(data[365..373].try_into().unwrap()),
            open_time: u64::from_le_bytes(data[373..381].try_into().unwrap()),
            padding: [0u64; 32], // Skip padding
        };

        // Check if pool is active
        if !pool_state.is_active() {
            debug!("CP pool {} is not active (status != 1)", address);
            return Ok(None);
        }

        // Get token metadata
        let token_0_info = self.get_token_info(&pool_state.token_0_mint).await?;
        let token_1_info = self.get_token_info(&pool_state.token_1_mint).await?;

        // Get actual vault balances from token accounts
        let token_0_balance = self.get_token_balance(&pool_state.token_0_vault).await?;
        let token_1_balance = self.get_token_balance(&pool_state.token_1_vault).await?;

        // Calculate liquidity in USD (simplified)
        let liquidity_usd = self.estimate_liquidity_usd(
            token_0_balance,
            token_1_balance,
            &token_0_info,
            &token_1_info,
        );

        Ok(Some(PoolInfo {
            pool_type: PoolType::Standard, // Use Standard type for CP pools
            address,
            token_a: token_0_info,
            token_b: token_1_info,
            liquidity_usd,
            volume_24h_usd: 0.0, // Would need to track swaps
            fee_rate: 0.003, // Default 0.3% fee for CP pools
            program_id: *RAYDIUM_CP_SWAP_PROGRAM,
            pool_state: PoolState::Standard {
                reserve_a: token_0_balance,
                reserve_b: token_1_balance,
            },
        }))
    }

    /// Find all CP pools for a token pair
    pub async fn find_pools_for_pair(
        &self,
        token_a: Pubkey,
        token_b: Pubkey,
    ) -> SwapResult<Vec<PoolInfo>> {
        debug!("Searching for CP pools: {}/{}", token_a, token_b);

        let mut all_pools = Vec::new();

        // Search for pools where token_a is token_0
        let filters1 = vec![
            RpcFilterType::DataSize(CpSwapPoolState::LEN as u64),
            RpcFilterType::Memcmp(Memcmp::new_raw_bytes(
                168, // token_0_mint offset
                token_a.to_bytes().to_vec(),
            )),
            RpcFilterType::Memcmp(Memcmp::new_raw_bytes(
                200, // token_1_mint offset
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
            .get_program_accounts_with_config(&RAYDIUM_CP_SWAP_PROGRAM, config1)
            .await
        {
            Ok(accounts) => {
                debug!("Found {} CP accounts with token_a as token_0", accounts.len());
                for (address, account) in accounts {
                    if let Some(pool) = self.parse_pool(address, &account.data).await? {
                        all_pools.push(pool);
                    }
                }
            }
            Err(e) => {
                warn!("Error searching CP pools with token_a as token_0: {}", e);
            }
        }

        // Search for pools where tokens are swapped
        let filters2 = vec![
            RpcFilterType::DataSize(CpSwapPoolState::LEN as u64),
            RpcFilterType::Memcmp(Memcmp::new_raw_bytes(
                168, // token_0_mint offset
                token_b.to_bytes().to_vec(),
            )),
            RpcFilterType::Memcmp(Memcmp::new_raw_bytes(
                200, // token_1_mint offset
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
            .get_program_accounts_with_config(&RAYDIUM_CP_SWAP_PROGRAM, config2)
            .await
        {
            Ok(accounts) => {
                debug!("Found {} CP accounts with token_b as token_0", accounts.len());
                for (address, account) in accounts {
                    if let Some(pool) = self.parse_pool(address, &account.data).await? {
                        all_pools.push(pool);
                    }
                }
            }
            Err(e) => {
                warn!("Error searching CP pools with token_b as token_0: {}", e);
            }
        }

        debug!("Found {} total CP pools for pair", all_pools.len());
        Ok(all_pools)
    }

    /// Find all CP pools containing a specific token
    pub async fn find_pools_by_token(&self, token: Pubkey) -> SwapResult<Vec<PoolInfo>> {
        debug!("Searching for all CP pools containing token {}", token);
        
        let mut all_pools = Vec::new();

        // Search for pools where token is token_0
        let filters1 = vec![
            RpcFilterType::DataSize(CpSwapPoolState::LEN as u64),
            RpcFilterType::Memcmp(Memcmp::new_raw_bytes(
                168, // token_0_mint offset
                token.to_bytes().to_vec(),
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
            .get_program_accounts_with_config(&RAYDIUM_CP_SWAP_PROGRAM, config1)
            .await
        {
            Ok(accounts) => {
                debug!("Found {} CP accounts with token as token_0", accounts.len());
                for (address, account) in accounts {
                    if let Some(pool) = self.parse_pool(address, &account.data).await? {
                        all_pools.push(pool);
                    }
                }
            }
            Err(e) => {
                warn!("Error searching CP pools with token as token_0: {}", e);
            }
        }

        // Search for pools where token is token_1
        let filters2 = vec![
            RpcFilterType::DataSize(CpSwapPoolState::LEN as u64),
            RpcFilterType::Memcmp(Memcmp::new_raw_bytes(
                200, // token_1_mint offset
                token.to_bytes().to_vec(),
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
            .get_program_accounts_with_config(&RAYDIUM_CP_SWAP_PROGRAM, config2)
            .await
        {
            Ok(accounts) => {
                debug!("Found {} CP accounts with token as token_1", accounts.len());
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
                warn!("Error searching CP pools with token as token_1: {}", e);
            }
        }

        debug!("Found {} total CP pools containing token", all_pools.len());
        Ok(all_pools)
    }

    /// Get token balance for an account
    async fn get_token_balance(&self, token_account: &Pubkey) -> SwapResult<u64> {
        match self.rpc_client.get_account(token_account).await {
            Ok(account) => {
                // SPL Token account layout: amount is at offset 64
                if account.data.len() >= 72 {
                    let amount_bytes = &account.data[64..72];
                    let amount = u64::from_le_bytes(amount_bytes.try_into().unwrap());
                    Ok(amount)
                } else {
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
        self.metadata_fetcher.get_token_metadata(mint).await
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
        let value_a = match token_a.symbol.as_str() {
            "SOL" => (reserve_a as f64 / 10f64.powi(token_a.decimals as i32)) * 140.0,
            "USDC" | "USDT" => reserve_a as f64 / 10f64.powi(token_a.decimals as i32),
            _ => 0.0,
        };

        let value_b = match token_b.symbol.as_str() {
            "SOL" => (reserve_b as f64 / 10f64.powi(token_b.decimals as i32)) * 140.0,
            "USDC" | "USDT" => reserve_b as f64 / 10f64.powi(token_b.decimals as i32),
            _ => 0.0,
        };

        value_a + value_b
    }
}