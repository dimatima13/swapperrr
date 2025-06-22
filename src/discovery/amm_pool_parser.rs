use crate::core::{
    constants::*, layouts::AmmInfoLayoutV4, PoolInfo, PoolState, PoolType, SwapError, SwapResult,
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

const TOKEN_2022_PROGRAM_ID: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";

pub struct AmmPoolParser {
    rpc_client: Arc<RpcClient>,
    metadata_fetcher: Arc<AsyncTokenMetadataFetcher>,
}

impl AmmPoolParser {
    pub fn new(rpc_client: Arc<RpcClient>, _rpc_url: String) -> Self {
        let metadata_fetcher = Arc::new(AsyncTokenMetadataFetcher::new(rpc_client.clone()));
        Self { 
            rpc_client,
            metadata_fetcher,
        }
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
            debug!("AMM pool {} is not enabled (status != 6)", address);
            return Ok(None);
        }
        
        // Warn about pools with placeholder serum markets
        let serum_market_str = pool_state.serum_market.to_string();
        if serum_market_str.starts_with("11111111") {
            debug!("AMM pool {} has placeholder serum market: {} (may not work for swaps)", address, pool_state.serum_market);
            // Don't filter out - some pools might still work
        }

        // Get token reserves
        // Use vault addresses from the pool state
        debug!("Coin vault address: {}", pool_state.pool_coin_token_account);
        debug!("PC vault address: {}", pool_state.pool_pc_token_account);
        let coin_vault_balance = self.get_token_balance(&pool_state.pool_coin_token_account).await?;
        let pc_vault_balance = self.get_token_balance(&pool_state.pool_pc_token_account).await?;
        debug!("Coin vault balance: {}, PC vault balance: {}", coin_vault_balance, pc_vault_balance);

        // Get token metadata
        let coin_token_info = self.get_token_info(&pool_state.coin_mint_address).await?;
        let pc_token_info = self.get_token_info(&pool_state.pc_mint_address).await?;
        
        // Verify vault mints match pool mints
        // Sometimes vaults can be swapped, so we need to check
        let coin_vault_mint = self.get_vault_mint(&pool_state.pool_coin_token_account).await.ok();
        let pc_vault_mint = self.get_vault_mint(&pool_state.pool_pc_token_account).await.ok();
        
        debug!("Pool coin mint: {}, vault mint: {:?}", pool_state.coin_mint_address, coin_vault_mint);
        debug!("Pool pc mint: {}, vault mint: {:?}", pool_state.pc_mint_address, pc_vault_mint);
        
        // Keep the original pool token order for correct calculations
        // coin is token_a, pc is token_b
        let (token_a_info, token_b_info, token_a_balance, token_b_balance) = 
            (coin_token_info, pc_token_info, coin_vault_balance, pc_vault_balance);

        // Calculate liquidity in USD (simplified - would need price oracle in production)
        let liquidity_usd = self.estimate_liquidity_usd(
            token_a_balance,
            token_b_balance,
            &token_a_info,
            &token_b_info,
        );
        
        // Sanity check reserves
        if token_a_balance == 0 || token_b_balance == 0 {
            debug!("AMM pool {} has zero reserves: coin={}, pc={}", address, token_a_balance, token_b_balance);
            return Ok(None);
        }
        
        // Filter out pools with unrealistic prices - DISABLED for debugging
        if token_a_info.symbol == "SOL" && token_b_info.symbol == "USDC" {
            let sol_balance = token_a_balance as f64 / 10f64.powi(token_a_info.decimals as i32);
            let usdc_balance = token_b_balance as f64 / 10f64.powi(token_b_info.decimals as i32);
            if sol_balance > 0.0 {
                let price = usdc_balance / sol_balance;
                // Log warning but don't filter out for now
                if price > 1000.0 || price < 10.0 {
                    debug!("WARNING: Pool {} has unusual SOL/USDC price: {}", address, price);
                    // return Ok(None);
                }
            }
        }

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
                nonce: pool_state.nonce as u8,
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
            RpcFilterType::Memcmp(Memcmp::new_raw_bytes(
                400, // coin_mint_address offset
                token_a.to_bytes().to_vec(),
            )),
            RpcFilterType::Memcmp(Memcmp::new_raw_bytes(
                432, // pc_mint_address offset
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
            RpcFilterType::Memcmp(Memcmp::new_raw_bytes(
                400, // coin_mint_address offset
                token_b.to_bytes().to_vec(),
            )),
            RpcFilterType::Memcmp(Memcmp::new_raw_bytes(
                432, // pc_mint_address offset
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

    /// Find all AMM pools containing a specific token
    pub async fn find_pools_by_token(&self, token: Pubkey) -> SwapResult<Vec<PoolInfo>> {
        debug!("Searching for all AMM pools containing token {}", token);
        let mut all_pools = Vec::new();

        // Search for pools where token is coin_mint (token A)
        let filters1 = vec![
            RpcFilterType::DataSize(AmmInfoLayoutV4::LEN as u64),
            RpcFilterType::Memcmp(Memcmp::new_raw_bytes(
                400, // coin_mint_address offset
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
            .get_program_accounts_with_config(&AMM_V4_PROGRAM, config1)
            .await
        {
            Ok(accounts) => {
                debug!("Found {} AMM accounts with token as coin_mint", accounts.len());
                for (address, account) in accounts {
                    if let Some(pool) = self.parse_pool(address, &account.data).await? {
                        all_pools.push(pool);
                    }
                }
            }
            Err(e) => {
                warn!("Error searching AMM pools with token as coin: {}", e);
            }
        }

        // Search for pools where token is pc_mint (token B)
        let filters2 = vec![
            RpcFilterType::DataSize(AmmInfoLayoutV4::LEN as u64),
            RpcFilterType::Memcmp(Memcmp::new_raw_bytes(
                432, // pc_mint_address offset
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
            .get_program_accounts_with_config(&AMM_V4_PROGRAM, config2)
            .await
        {
            Ok(accounts) => {
                debug!("Found {} AMM accounts with token as pc_mint", accounts.len());
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
                warn!("Error searching AMM pools with token as pc: {}", e);
            }
        }

        debug!("Found {} total AMM pools containing token", all_pools.len());
        Ok(all_pools)
    }

    /// Get vault mint address
    async fn get_vault_mint(&self, vault_account: &Pubkey) -> SwapResult<Pubkey> {
        match self.rpc_client.get_account(vault_account).await {
            Ok(account) => {
                if account.owner == spl_token::ID || account.owner.to_string() == TOKEN_2022_PROGRAM_ID {
                    // Token account mint is at offset 0
                    if account.data.len() >= 32 {
                        Ok(Pubkey::new_from_array(account.data[0..32].try_into().unwrap()))
                    } else {
                        Err(SwapError::ParseError("Invalid token account data".to_string()))
                    }
                } else {
                    Err(SwapError::ParseError("Not a token account".to_string()))
                }
            }
            Err(e) => Err(SwapError::RpcError(e)),
        }
    }
    
    /// Get token balance for an account
    async fn get_token_balance(&self, token_account: &Pubkey) -> SwapResult<u64> {
        debug!("Getting token balance for account: {}", token_account);
        match self.rpc_client.get_account(token_account).await {
            Ok(account) => {
                debug!("Account found, owner: {}, data len: {}", account.owner, account.data.len());
                // Parse SPL token account (first 8 bytes is amount)
                if account.data.len() >= 165 {
                    // SPL Token account layout: amount is at offset 64
                    let balance = u64::from_le_bytes(
                        account.data[64..72].try_into().unwrap(),
                    );
                    debug!("Token balance: {}", balance);
                    Ok(balance)
                } else if account.data.len() == 82 {
                    // Token-2022 account without extensions - balance at offset 32
                    let balance = u64::from_le_bytes(
                        account.data[32..40].try_into().unwrap(),
                    );
                    debug!("Token balance (82 byte Token-2022 account): {}", balance);
                    Ok(balance)
                } else {
                    debug!("Invalid token account data length: {} (expected 165 or 82)", account.data.len());
                    // Try to read balance anyway if data is long enough
                    if account.data.len() >= 72 {
                        let balance = u64::from_le_bytes(
                            account.data[64..72].try_into().unwrap(),
                        );
                        debug!("Token balance (non-standard account): {}", balance);
                        Ok(balance)
                    } else {
                        Ok(0)
                    }
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
        // Use our metadata fetcher to get token info
        self.metadata_fetcher.get_token_metadata(mint).await
    }

    /// Get token decimals from mint account
    #[allow(dead_code)]
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
            "SOL" => (reserve_a as f64 / 10f64.powi(token_a.decimals as i32)) * 140.0, // Updated to ~$140
            "USDC" | "USDT" => reserve_a as f64 / 10f64.powi(token_a.decimals as i32),
            "BONK" => (reserve_a as f64 / 10f64.powi(token_a.decimals as i32)) * 0.00002,
            _ => 0.0,
        };

        let value_b = match token_b.symbol.as_str() {
            "SOL" => (reserve_b as f64 / 10f64.powi(token_b.decimals as i32)) * 140.0, // Updated to ~$140
            "USDC" | "USDT" => reserve_b as f64 / 10f64.powi(token_b.decimals as i32),
            "BONK" => (reserve_b as f64 / 10f64.powi(token_b.decimals as i32)) * 0.00002,
            _ => 0.0,
        };

        value_a + value_b
    }
    
    /// Get AMM vault addresses using PDA
    #[allow(dead_code)]
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