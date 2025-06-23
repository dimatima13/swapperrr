pub mod amm_swap;
pub mod stable_swap;
pub mod clmm_swap;
pub mod cp_swap;
pub mod monitor;
pub mod wsol;
pub mod alt;

use crate::core::{
    constants::{AMM_V4_PROGRAM, STABLE_PROGRAM, CLMM_PROGRAM},
    PoolType, SwapError, SwapParams, SwapResult, TransactionResult,
};
use chrono::Utc;
use log::{debug, info, warn};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    instruction::Instruction,
    pubkey::Pubkey,
    signature::{Keypair, Signature},
    signer::Signer,
    transaction::{Transaction, VersionedTransaction},
    message::{VersionedMessage, v0},
    compute_budget::ComputeBudgetInstruction,
};
use solana_client::rpc_config::RpcTransactionConfig;
use solana_transaction_status::UiTransactionEncoding;
use alt::AltManager;
use std::str::FromStr;
use spl_associated_token_account::{get_associated_token_address, instruction::create_associated_token_account};

// Token-2022 program ID
const TOKEN_2022_PROGRAM_ID: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";

pub use monitor::{TransactionMonitor, MonitorConfig, RetryConfig, BalanceChange};

/// Transaction version preference
#[derive(Debug, Clone, Copy)]
pub enum TransactionVersion {
    Legacy,
    V0,
}

impl Default for TransactionVersion {
    fn default() -> Self {
        TransactionVersion::V0
    }
}

/// Transaction executor for different pool types
pub struct TransactionExecutor {
    rpc_client: RpcClient,
    keypair: Keypair,
    monitor: TransactionMonitor,
    transaction_version: TransactionVersion,
    alt_manager: Option<AltManager>,
    use_alts: bool,
}

impl TransactionExecutor {
    pub fn new(rpc_url: String, keypair: Keypair) -> Self {
        let rpc_client = RpcClient::new_with_commitment(
            rpc_url.clone(),
            CommitmentConfig::confirmed(),
        );

        let monitor = TransactionMonitor::new(
            rpc_url,
            None, // Use default monitor config
            None, // Use default retry config
        );

        Self {
            rpc_client,
            keypair,
            monitor,
            transaction_version: TransactionVersion::default(),
            alt_manager: None,
            use_alts: false,
        }
    }

    pub fn new_with_config(
        rpc_url: String, 
        keypair: Keypair, 
        monitor_config: Option<MonitorConfig>,
        retry_config: Option<RetryConfig>,
    ) -> Self {
        let rpc_client = RpcClient::new_with_commitment(
            rpc_url.clone(),
            CommitmentConfig::confirmed(),
        );

        let monitor = TransactionMonitor::new(
            rpc_url,
            monitor_config,
            retry_config,
        );

        Self {
            rpc_client,
            keypair,
            monitor,
            transaction_version: TransactionVersion::default(),
            alt_manager: None,
            use_alts: false,
        }
    }

    /// Set transaction version preference
    pub fn set_transaction_version(&mut self, version: TransactionVersion) {
        self.transaction_version = version;
    }

    /// Enable ALT usage
    pub async fn enable_alts(&mut self, rpc_url: String) {
        let rpc_client = std::sync::Arc::new(RpcClient::new_with_commitment(
            rpc_url,
            CommitmentConfig::confirmed(),
        ));
        self.alt_manager = Some(AltManager::new(rpc_client));
        self.use_alts = true;
        info!("Address Lookup Tables enabled");
    }

    /// Set ALT usage preference
    pub fn set_use_alts(&mut self, use_alts: bool) {
        self.use_alts = use_alts;
    }

    /// Create compute budget instructions
    fn create_compute_budget_instructions(compute_units: Option<u32>, priority_fee: Option<u64>) -> Vec<Instruction> {
        let mut instructions = vec![];
        
        // Add compute unit limit if specified
        if let Some(units) = compute_units {
            instructions.push(ComputeBudgetInstruction::set_compute_unit_limit(units));
        }
        
        // Add priority fee if specified
        if let Some(fee) = priority_fee {
            instructions.push(ComputeBudgetInstruction::set_compute_unit_price(fee));
        }
        
        instructions
    }

    /// Build a versioned transaction (v0)
    async fn build_versioned_transaction(
        &self,
        instructions: Vec<Instruction>,
        payer: &Pubkey,
        signers: &[&Keypair],
    ) -> SwapResult<VersionedTransaction> {
        // Get recent blockhash
        let recent_blockhash = self.rpc_client.get_latest_blockhash().await
            .map_err(SwapError::RpcError)?;

        // Add compute budget instructions at the beginning
        let mut all_instructions = Self::create_compute_budget_instructions(
            Some(400_000), // Default compute units
            Some(1_000),   // Default priority fee (1000 microlamports)
        );
        all_instructions.extend(instructions);

        // Prepare ALT lookups if enabled
        let alt_accounts = if self.use_alts && self.alt_manager.is_some() {
            // Extract all unique accounts from instructions
            let mut accounts = Vec::new();
            for ix in &all_instructions {
                accounts.push(ix.program_id);
                for meta in &ix.accounts {
                    accounts.push(meta.pubkey);
                }
            }
            accounts.sort();
            accounts.dedup();

            // Check if ALTs would be beneficial
            if AltManager::should_use_alts(accounts.len()) {
                info!("Using ALTs for {} accounts", accounts.len());
                if let Some(ref alt_manager) = self.alt_manager {
                    match alt_manager.find_optimal_alts(&accounts).await {
                        Ok(alts) => {
                            info!("Found {} ALT accounts", alts.len());
                            alts
                        }
                        Err(e) => {
                            warn!("Failed to find ALTs: {}, continuing without them", e);
                            vec![]
                        }
                    }
                } else {
                    vec![]
                }
            } else {
                vec![]
            }
        } else {
            vec![]
        };

        // Create v0 message
        let message = v0::Message::try_compile(
            payer,
            &all_instructions,
            &alt_accounts,
            recent_blockhash,
        ).map_err(|e| SwapError::Other(format!("Failed to compile v0 message: {}", e)))?;

        // Create versioned message
        let versioned_message = VersionedMessage::V0(message);

        // Create versioned transaction
        let transaction = VersionedTransaction::try_new(versioned_message, signers)
            .map_err(|e| SwapError::Other(format!("Failed to create versioned transaction: {}", e)))?;

        Ok(transaction)
    }

    /// Determine which token program owns a mint
    async fn get_token_program_for_mint(&self, mint: &Pubkey) -> SwapResult<Pubkey> {
        match self.rpc_client.get_account(mint).await {
            Ok(account) => {
                // The owner of the mint account is the token program
                Ok(account.owner)
            }
            Err(e) => {
                warn!("Failed to get mint account for {}: {}, defaulting to SPL Token", mint, e);
                // Default to regular SPL Token program
                Ok(spl_token::ID)
            }
        }
    }

    /// Execute a swap transaction
    pub async fn execute_swap(&self, params: SwapParams) -> SwapResult<TransactionResult> {
        info!(
            "Executing swap on {:?} pool {}",
            params.quote.pool_info.pool_type, params.quote.pool_info.address
        );
        info!("Swap details: {} {} -> {} (amount: {})", 
            params.token_in, 
            params.quote.pool_info.token_a.symbol,
            params.quote.pool_info.token_b.symbol,
            params.quote.amount_in
        );

        // Check and create associated token accounts if needed
        let mut instructions = vec![];
        
        // Get token mints
        let token_in_mint = params.token_in;
        let token_out_mint = params.token_out;
        let user_pubkey = self.keypair.pubkey();
        
        // Native SOL mint
        let native_sol_mint = spl_token::native_mint::ID;
        
        // Handle native SOL wrapping
        if token_in_mint == native_sol_mint {
            info!("Input token is native SOL, checking if wrapping is needed");
            let (needs_wrapping, _amount_to_wrap, wrap_instructions) = 
                wsol::check_and_prepare_wsol_wrapping(
                    &self.rpc_client,
                    &user_pubkey,
                    params.quote.amount_in,
                ).await?;
            
            if needs_wrapping {
                info!("Adding SOL wrapping instructions to transaction");
                instructions.extend(wrap_instructions);
            }
        }
        
        // Check if user has associated token account for output token
        let user_out_ata = get_associated_token_address(&user_pubkey, &token_out_mint);
        let out_ata_exists = self.rpc_client.get_account(&user_out_ata).await.is_ok();
        info!("Output ATA {} exists: {}", user_out_ata, out_ata_exists);
        
        if !out_ata_exists {
            debug!("Creating associated token account for output token");
            
            // Determine the correct token program for the output mint
            let token_program = self.get_token_program_for_mint(&token_out_mint).await?;
            info!("Output token {} uses program: {} (Token-2022: {})", 
                token_out_mint, 
                token_program,
                token_program == Pubkey::from_str(TOKEN_2022_PROGRAM_ID).unwrap()
            );
            
            let create_ata_ix = create_associated_token_account(
                &user_pubkey,
                &user_pubkey,
                &token_out_mint,
                &token_program,
            );
            instructions.push(create_ata_ix);
        }

        // Build pool-specific instruction
        let swap_instruction = self.build_swap_instruction(&params).await?;
        instructions.push(swap_instruction);
        
        // Handle native SOL unwrapping if output is SOL
        if token_out_mint == native_sol_mint {
            info!("Output token is native SOL, adding unwrap instruction");
            let unwrap_ix = wsol::create_unwrap_sol_instruction(&user_pubkey)?;
            instructions.push(unwrap_ix);
        }

        // Create transaction based on version preference
        let start_time = std::time::Instant::now();
        let (signature, retry_attempts) = match self.transaction_version {
            TransactionVersion::V0 => {
                info!("Creating v0 transaction");
                let transaction = self.build_versioned_transaction(
                    instructions,
                    &self.keypair.pubkey(),
                    &[&self.keypair],
                ).await?;

                // Simulate versioned transaction first
                debug!("Simulating v0 transaction...");
                match self.rpc_client.simulate_transaction(&transaction).await {
                    Ok(result) => {
                        if let Some(err) = result.value.err {
                            return Err(SwapError::SimulationFailed(format!("{:?}", err)));
                        }
                        debug!("Simulation successful");
                    }
                    Err(e) => {
                        return Err(SwapError::SimulationFailed(e.to_string()));
                    }
                }

                // Send versioned transaction with monitoring and retry logic
                info!("Sending v0 transaction with monitoring and retry...");
                
                self.monitor
                    .send_and_confirm_versioned_with_retry(transaction, &[&self.keypair])
                    .await?
            }
            TransactionVersion::Legacy => {
                info!("Creating legacy transaction");
                // Get recent blockhash
                let recent_blockhash = self.rpc_client.get_latest_blockhash().await?;

                // Create legacy transaction
                let mut transaction = Transaction::new_with_payer(
                    &instructions,
                    Some(&self.keypair.pubkey()),
                );
                transaction.sign(&[&self.keypair], recent_blockhash);

                // Simulate transaction first
                debug!("Simulating legacy transaction...");
                match self.rpc_client.simulate_transaction(&transaction).await {
                    Ok(result) => {
                        if let Some(err) = result.value.err {
                            return Err(SwapError::SimulationFailed(format!("{:?}", err)));
                        }
                        debug!("Simulation successful");
                    }
                    Err(e) => {
                        return Err(SwapError::SimulationFailed(e.to_string()));
                    }
                }

                // Send transaction with monitoring and retry logic
                info!("Sending legacy transaction with monitoring and retry...");
                
                self.monitor
                    .send_and_confirm_with_retry(&mut transaction, &[&self.keypair])
                    .await?
            }
        };

        let confirmation_time = start_time.elapsed().as_millis() as u64;
        info!("Transaction confirmed in {}ms: {}", confirmation_time, signature);

        // Get transaction details to calculate actual slippage
        let actual_amount_out = self.get_actual_output_amount(&signature).await?;
        info!("Got actual_amount_out from transaction: {}", actual_amount_out);
        
        // If we couldn't parse the actual output, use the expected amount as fallback
        let actual_amount_out = if actual_amount_out == 0 {
            warn!("Could not parse actual output amount, using expected amount");
            params.quote.amount_out
        } else {
            actual_amount_out
        };
        
        let actual_slippage = calculate_actual_slippage(
            params.quote.amount_out,
            actual_amount_out,
        );

        // Get transaction fee
        let transaction_fee = match monitor::utils::calculate_transaction_fee(&self.rpc_client, &signature).await {
            Ok(fee) => Some(fee),
            Err(e) => {
                warn!("Could not get transaction fee: {}", e);
                None
            }
        };

        // Check if transaction is finalized
        let finalized = match monitor::utils::is_transaction_successful(&self.rpc_client, &signature).await {
            Ok(success) => success,
            Err(_) => false,
        };

        Ok(TransactionResult {
            signature: signature.to_string(),
            pool_type: params.quote.pool_info.pool_type,
            pool_address: params.quote.pool_info.address,
            amount_in: params.quote.amount_in,
            amount_out: actual_amount_out,
            expected_amount_out: params.quote.amount_out,
            actual_slippage,
            fee_paid: params.quote.fee,
            timestamp: Utc::now().timestamp(),
            retry_attempts,
            confirmation_time_ms: confirmation_time,
            finalized,
            transaction_fee,
        })
    }

    /// Build pool-specific swap instruction
    async fn build_swap_instruction(&self, params: &SwapParams) -> SwapResult<Instruction> {
        match params.quote.pool_info.pool_type {
            PoolType::AMM => self.build_amm_swap_instruction(params).await,
            PoolType::Stable => self.build_stable_swap_instruction(params).await,
            PoolType::CLMM => self.build_clmm_swap_instruction(params).await,
            PoolType::Standard => self.build_standard_swap_instruction(params).await,
        }
    }

    /// Build AMM swap instruction
    async fn build_amm_swap_instruction(&self, params: &SwapParams) -> SwapResult<Instruction> {
        info!("Building AMM swap instruction for pool {}", params.quote.pool_info.address);
        
        // Get pool account data
        let pool_data = self.rpc_client
            .get_account_data(&params.quote.pool_info.address)
            .await?;
            
        // Get serum market address from pool state
        let pool_state = crate::core::layouts::AmmInfoLayoutV4::from_bytes(&pool_data)
            .map_err(|e| SwapError::ParseError(e))?;
        info!("Pool state - coin_mint: {}, pc_mint: {}, serum_market: {}",
            pool_state.coin_mint_address,
            pool_state.pc_mint_address,
            pool_state.serum_market
        );
        
        // Check if serum market is a placeholder
        let serum_market_str = pool_state.serum_market.to_string();
        let market_data = if serum_market_str.starts_with("11111111") {
            info!("Pool has placeholder serum market, using empty data");
            vec![0u8; 388] // Minimum size for serum market parsing
        } else {
            self.rpc_client
                .get_account_data(&pool_state.serum_market)
                .await?
        };
            
        // Build the swap instruction
        amm_swap::build_amm_swap_instruction(
            params,
            &self.keypair.pubkey(),
            &AMM_V4_PROGRAM,
            &pool_data,
            &market_data,
        ).await
    }

    /// Build Stable swap instruction
    async fn build_stable_swap_instruction(&self, params: &SwapParams) -> SwapResult<Instruction> {
        // Get pool account data for parsing
        let pool_data = self
            .rpc_client
            .get_account(&params.quote.pool_info.address)
            .await
            .map_err(SwapError::RpcError)?
            .data;
        
        stable_swap::build_stable_swap_instruction(
            params,
            &self.keypair.pubkey(),
            &STABLE_PROGRAM,
            &pool_data,
        ).await
    }

    /// Build CLMM swap instruction
    async fn build_clmm_swap_instruction(&self, params: &SwapParams) -> SwapResult<Instruction> {
        // Get pool account data for parsing
        let pool_data = self
            .rpc_client
            .get_account(&params.quote.pool_info.address)
            .await
            .map_err(SwapError::RpcError)?
            .data;
        
        clmm_swap::build_clmm_swap_instruction(
            params,
            &self.keypair.pubkey(),
            &CLMM_PROGRAM,
            &pool_data,
        ).await
    }

    /// Build Standard swap instruction
    async fn build_standard_swap_instruction(&self, params: &SwapParams) -> SwapResult<Instruction> {
        info!("Building Standard (CP) swap instruction for pool {}", params.quote.pool_info.address);
        
        // Get pool account data
        let pool_data = self.rpc_client
            .get_account_data(&params.quote.pool_info.address)
            .await?;
        
        // Build the swap instruction using CP swap builder
        cp_swap::build_cp_swap_instruction(
            params,
            &self.keypair.pubkey(),
            &crate::core::constants::RAYDIUM_CP_SWAP_PROGRAM,
            &pool_data,
        ).await
    }

    /// Get transaction with v0 support
    async fn get_transaction_with_config(
        &self,
        signature: &Signature,
    ) -> Result<solana_transaction_status::EncodedConfirmedTransactionWithStatusMeta, solana_client::client_error::ClientError> {
        let config = RpcTransactionConfig {
            encoding: Some(UiTransactionEncoding::Json),
            commitment: Some(CommitmentConfig::confirmed()),
            max_supported_transaction_version: Some(0),
        };
        
        self.rpc_client.get_transaction_with_config(signature, config).await
    }

    /// Get actual output amount from transaction
    async fn get_actual_output_amount(&self, signature: &Signature) -> SwapResult<u64> {
        info!("Analyzing transaction output for signature: {}", signature);
        
        // Wait a bit for transaction to be fully processed
        tokio::time::sleep(std::time::Duration::from_millis(2000)).await;
        
        // Get transaction details with full metadata
        match self.get_transaction_with_config(signature).await {
            Ok(transaction) => {
                if let Some(meta) = transaction.transaction.meta {
                    // Parse token balance changes from pre/post token balances
                    use solana_transaction_status::option_serializer::OptionSerializer;
                    
                    let (pre_balances, post_balances) = match (&meta.pre_token_balances, &meta.post_token_balances) {
                        (OptionSerializer::Some(pre), OptionSerializer::Some(post)) => (pre, post),
                        _ => {
                            debug!("Token balance data not available");
                            return Ok(0);
                        }
                    };
                    
                    info!("Pre token balances: {} accounts", pre_balances.len());
                    info!("Post token balances: {} accounts", post_balances.len());
                    
                    // Find balance changes for each account
                    for post_balance in post_balances {
                        if let Some(pre_balance) = pre_balances.iter()
                            .find(|pb| pb.account_index == post_balance.account_index) {
                            
                            // Parse amounts
                            info!("Account {} - Pre amount string: '{}', Post amount string: '{}'", 
                                post_balance.account_index,
                                pre_balance.ui_token_amount.amount,
                                post_balance.ui_token_amount.amount
                            );
                            let pre_amount = pre_balance.ui_token_amount.amount.parse::<u64>().unwrap_or(0);
                            let post_amount = post_balance.ui_token_amount.amount.parse::<u64>().unwrap_or(0);
                            
                            if post_amount > pre_amount {
                                let amount_received = post_amount - pre_amount;
                                info!("Found token balance increase: {} tokens (account {})", 
                                    amount_received, post_balance.account_index);
                                
                                info!("Token mint: {}", post_balance.mint);
                                info!("Decimals: {}", post_balance.ui_token_amount.decimals);
                                info!("UI Amount String: {}", post_balance.ui_token_amount.ui_amount_string);
                                info!("Amount received: {} units", amount_received);
                                
                                return Ok(amount_received);
                            }
                        } else {
                            // New token account created during swap
                            info!("New account {} - Amount string: '{}'", 
                                post_balance.account_index,
                                post_balance.ui_token_amount.amount
                            );
                            let amount_received = post_balance.ui_token_amount.amount.parse::<u64>().unwrap_or(0);
                            if amount_received > 0 {
                                info!("Found new token account with balance: {} tokens (account {})", 
                                    amount_received, post_balance.account_index);
                                
                                info!("Token mint: {}", post_balance.mint);
                                info!("Decimals: {}", post_balance.ui_token_amount.decimals);
                                info!("UI Amount String: {}", post_balance.ui_token_amount.ui_amount_string);
                                info!("Amount received: {} units", amount_received);
                                
                                return Ok(amount_received);
                            }
                        }
                    }
                    
                    // Alternative: Parse from instruction logs
                    match &meta.log_messages {
                        solana_transaction_status::option_serializer::OptionSerializer::Some(log_messages) => {
                            for log in log_messages {
                                // Raydium AMM swap logs contain transfer information
                                if log.contains("Program log: ray_log:") {
                                    debug!("Raydium log: {}", log);
                                    
                                    // Try to extract amounts from Raydium logs
                                    if let Some(amount) = self.parse_swap_amount_from_logs(log) {
                                        info!("Parsed swap amount from logs: {}", amount);
                                        return Ok(amount);
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                    
                    debug!("Could not determine actual output amount from transaction data");
                } else {
                    warn!("Transaction metadata not available");
                }
            }
            Err(e) => {
                warn!("Could not get transaction details: {}", e);
            }
        }
        
        // Return 0 if we couldn't parse the actual amount
        Ok(0)
    }
    
    /// Parse swap amount from Raydium log messages
    fn parse_swap_amount_from_logs(&self, log: &str) -> Option<u64> {
        // Raydium logs contain swap information in different formats for each pool type
        
        // AMM pool logs format: "ray_log: ... swap_base_out:123456, swap_quote_out:789012"
        if log.contains("swap_base_out:") || log.contains("swap_quote_out:") {
            for pattern in ["swap_base_out:", "swap_quote_out:"] {
                if let Some(start) = log.find(pattern) {
                    let remaining = &log[start + pattern.len()..];
                    // Find the number - it ends at comma, space, or end of string
                    let end = remaining.find(|c: char| !c.is_numeric())
                        .unwrap_or(remaining.len());
                    if let Ok(amount) = remaining[..end].trim().parse::<u64>() {
                        if amount > 0 {
                            debug!("Parsed {} amount: {}", pattern, amount);
                            return Some(amount);
                        }
                    }
                }
            }
        }
        
        // CLMM pool logs format: "amount_0:123456, amount_1:789012"
        if log.contains("amount_0:") || log.contains("amount_1:") {
            for pattern in ["amount_0:", "amount_1:"] {
                if let Some(start) = log.find(pattern) {
                    let remaining = &log[start + pattern.len()..];
                    let end = remaining.find(|c: char| !c.is_numeric())
                        .unwrap_or(remaining.len());
                    if let Ok(amount) = remaining[..end].trim().parse::<u64>() {
                        if amount > 0 {
                            debug!("Parsed CLMM {} amount: {}", pattern, amount);
                            return Some(amount);
                        }
                    }
                }
            }
        }
        
        // Stable pool logs format: "amount_out:123456"
        if log.contains("amount_out:") {
            if let Some(start) = log.find("amount_out:") {
                let remaining = &log[start + "amount_out:".len()..];
                let end = remaining.find(|c: char| !c.is_numeric())
                    .unwrap_or(remaining.len());
                if let Ok(amount) = remaining[..end].trim().parse::<u64>() {
                    debug!("Parsed Stable amount_out: {}", amount);
                    return Some(amount);
                }
            }
        }
        
        None
    }
}

/// Calculate actual slippage percentage
fn calculate_actual_slippage(expected: u64, actual: u64) -> f64 {
    if expected == 0 {
        return 0.0;
    }
    
    let diff = if expected > actual {
        expected - actual
    } else {
        actual - expected
    };
    
    (diff as f64 / expected as f64) * 100.0
}