use crate::core::{SwapError, SwapResult};
use log::{debug, info, warn, error};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    signature::Signature,
    transaction::{Transaction, VersionedTransaction},
    pubkey::Pubkey,
};
use solana_client::rpc_config::RpcTransactionConfig;
use solana_transaction_status::UiTransactionEncoding;
use std::time::{Duration, Instant};
use std::str::FromStr;
use tokio::time::sleep;

/// Balance change information for an account
#[derive(Debug, Clone)]
pub struct BalanceChange {
    /// Account address
    pub account: Pubkey,
    /// Token mint (None for native SOL)
    pub mint: Option<Pubkey>,
    /// Balance before transaction
    pub pre_balance: u64,
    /// Balance after transaction
    pub post_balance: u64,
    /// Change amount (can be negative)
    pub change: i64,
    /// Token decimals
    pub decimals: u8,
}

/// Transaction status monitoring configuration
#[derive(Debug, Clone)]
pub struct MonitorConfig {
    /// Maximum number of confirmation attempts
    pub max_confirmation_attempts: u32,
    /// Timeout for each confirmation attempt (seconds)
    pub confirmation_timeout_secs: u64,
    /// Interval between confirmation checks (milliseconds)
    pub check_interval_ms: u64,
    /// Maximum time to wait for finalization (seconds)
    pub finalization_timeout_secs: u64,
}

impl Default for MonitorConfig {
    fn default() -> Self {
        Self {
            max_confirmation_attempts: 30,
            confirmation_timeout_secs: 60,
            check_interval_ms: 1000,
            finalization_timeout_secs: 120,
        }
    }
}

/// Transaction retry configuration
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts
    pub max_retries: u32,
    /// Base delay between retries (milliseconds)
    pub base_delay_ms: u64,
    /// Maximum delay between retries (milliseconds)
    pub max_delay_ms: u64,
    /// Exponential backoff multiplier
    pub backoff_multiplier: f64,
    /// Whether to refresh blockhash on retry
    pub refresh_blockhash: bool,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_delay_ms: 1000,
            max_delay_ms: 10000,
            backoff_multiplier: 2.0,
            refresh_blockhash: true,
        }
    }
}

/// Transaction monitoring and retry handler
pub struct TransactionMonitor {
    rpc_client: RpcClient,
    monitor_config: MonitorConfig,
    retry_config: RetryConfig,
}

impl TransactionMonitor {
    pub fn new(
        rpc_url: String,
        monitor_config: Option<MonitorConfig>,
        retry_config: Option<RetryConfig>,
    ) -> Self {
        let rpc_client = RpcClient::new_with_commitment(
            rpc_url,
            CommitmentConfig::confirmed(),
        );

        Self {
            rpc_client,
            monitor_config: monitor_config.unwrap_or_default(),
            retry_config: retry_config.unwrap_or_default(),
        }
    }

    /// Send versioned transaction with retry logic and monitoring
    /// Returns (signature, retry_attempts)
    pub async fn send_and_confirm_versioned_with_retry(
        &self,
        mut transaction: VersionedTransaction,
        signer_keypairs: &[&dyn solana_sdk::signer::Signer],
    ) -> SwapResult<(Signature, u32)> {
        let mut attempt = 0;
        let mut last_error = None;

        while attempt <= self.retry_config.max_retries {
            if attempt > 0 {
                // Calculate delay with exponential backoff
                let delay = self.calculate_retry_delay(attempt);
                info!("Retrying transaction in {}ms (attempt {}/{})", 
                      delay, attempt, self.retry_config.max_retries);
                sleep(Duration::from_millis(delay)).await;

                // Refresh blockhash if configured
                if self.retry_config.refresh_blockhash {
                    match self.refresh_versioned_transaction_blockhash(&mut transaction, signer_keypairs).await {
                        Ok(_) => debug!("Refreshed transaction blockhash"),
                        Err(e) => {
                            warn!("Failed to refresh blockhash: {}", e);
                            last_error = Some(e);
                            attempt += 1;
                            continue;
                        }
                    }
                }
            }

            match self.send_and_monitor_versioned_transaction(&transaction).await {
                Ok(signature) => {
                    if attempt > 0 {
                        info!("Transaction succeeded on retry attempt {}", attempt);
                    }
                    return Ok((signature, attempt));
                }
                Err(e) => {
                    error!("Transaction attempt {} failed: {}", attempt + 1, e);
                    last_error = Some(e);
                    
                    // Check if error is retryable
                    if !self.is_retryable_error(&last_error.as_ref().unwrap()) {
                        break;
                    }
                }
            }

            attempt += 1;
        }

        Err(last_error.unwrap_or_else(|| {
            SwapError::Other("Transaction failed after all retry attempts".to_string())
        }))
    }

    /// Send transaction with retry logic and monitoring
    /// Returns (signature, retry_attempts)
    pub async fn send_and_confirm_with_retry(
        &self,
        transaction: &mut Transaction,
        signer_keypairs: &[&dyn solana_sdk::signer::Signer],
    ) -> SwapResult<(Signature, u32)> {
        let mut attempt = 0;
        let mut last_error = None;

        while attempt <= self.retry_config.max_retries {
            if attempt > 0 {
                // Calculate delay with exponential backoff
                let delay = self.calculate_retry_delay(attempt);
                info!("Retrying transaction in {}ms (attempt {}/{})", 
                      delay, attempt, self.retry_config.max_retries);
                sleep(Duration::from_millis(delay)).await;

                // Refresh blockhash if configured
                if self.retry_config.refresh_blockhash {
                    match self.refresh_transaction_blockhash(transaction, signer_keypairs).await {
                        Ok(_) => debug!("Refreshed transaction blockhash"),
                        Err(e) => {
                            warn!("Failed to refresh blockhash: {}", e);
                            last_error = Some(e);
                            attempt += 1;
                            continue;
                        }
                    }
                }
            }

            match self.send_and_monitor_transaction(transaction).await {
                Ok(signature) => {
                    if attempt > 0 {
                        info!("Transaction succeeded on retry attempt {}", attempt);
                    }
                    return Ok((signature, attempt));
                }
                Err(e) => {
                    error!("Transaction attempt {} failed: {}", attempt + 1, e);
                    last_error = Some(e);
                    
                    // Check if error is retryable
                    if !self.is_retryable_error(&last_error.as_ref().unwrap()) {
                        break;
                    }
                }
            }

            attempt += 1;
        }

        Err(last_error.unwrap_or_else(|| {
            SwapError::Other("Transaction failed after all retry attempts".to_string())
        }))
    }

    /// Send transaction and monitor its status
    async fn send_and_monitor_transaction(&self, transaction: &Transaction) -> SwapResult<Signature> {
        debug!("Sending transaction...");
        
        // Send transaction
        let signature = self.rpc_client
            .send_transaction(transaction)
            .await
            .map_err(SwapError::RpcError)?;

        info!("Transaction sent: {}", signature);

        // Monitor confirmation
        self.monitor_confirmation(&signature).await?;

        Ok(signature)
    }

    /// Monitor transaction confirmation status
    async fn monitor_confirmation(&self, signature: &Signature) -> SwapResult<()> {
        let start_time = Instant::now();
        let timeout = Duration::from_secs(self.monitor_config.confirmation_timeout_secs);
        let check_interval = Duration::from_millis(self.monitor_config.check_interval_ms);
        
        let mut confirmation_attempts = 0;

        loop {
            if start_time.elapsed() > timeout {
                return Err(SwapError::Other(
                    format!("Transaction confirmation timeout after {}s", 
                            self.monitor_config.confirmation_timeout_secs)
                ));
            }

            if confirmation_attempts >= self.monitor_config.max_confirmation_attempts {
                return Err(SwapError::Other(
                    format!("Maximum confirmation attempts ({}) reached", 
                            self.monitor_config.max_confirmation_attempts)
                ));
            }

            // Check transaction status
            match self.rpc_client.get_signature_status(signature).await {
                Ok(Some(status)) => {
                    match status {
                        Ok(_) => {
                            info!("Transaction confirmed: {}", signature);
                            
                            // Optionally wait for finalization
                            if self.monitor_config.finalization_timeout_secs > 0 {
                                return self.wait_for_finalization(signature).await;
                            }
                            
                            return Ok(());
                        }
                        Err(err) => {
                            return Err(SwapError::TransactionFailed(
                                format!("Transaction failed: {:?}", err)
                            ));
                        }
                    }
                }
                Ok(None) => {
                    debug!("Transaction not yet processed, checking again...");
                }
                Err(e) => {
                    warn!("Error checking transaction status: {}", e);
                }
            }

            confirmation_attempts += 1;
            sleep(check_interval).await;
        }
    }

    /// Wait for transaction finalization
    async fn wait_for_finalization(&self, signature: &Signature) -> SwapResult<()> {
        debug!("Waiting for transaction finalization...");
        
        let start_time = Instant::now();
        let timeout = Duration::from_secs(self.monitor_config.finalization_timeout_secs);
        let check_interval = Duration::from_millis(self.monitor_config.check_interval_ms * 2);

        loop {
            if start_time.elapsed() > timeout {
                warn!("Transaction finalization timeout, but transaction was confirmed");
                return Ok(());
            }

            // Check if transaction is finalized
            match self.rpc_client
                .get_signature_status_with_commitment(
                    signature,
                    CommitmentConfig::finalized(),
                )
                .await
            {
                Ok(Some(status)) => {
                    match status {
                        Ok(_) => {
                            info!("Transaction finalized: {}", signature);
                            return Ok(());
                        }
                        Err(err) => {
                            return Err(SwapError::TransactionFailed(
                                format!("Transaction failed during finalization: {:?}", err)
                            ));
                        }
                    }
                }
                Ok(None) => {
                    debug!("Transaction not yet finalized, checking again...");
                }
                Err(e) => {
                    warn!("Error checking finalization status: {}", e);
                }
            }

            sleep(check_interval).await;
        }
    }

    /// Refresh transaction blockhash and re-sign
    async fn refresh_transaction_blockhash(
        &self,
        transaction: &mut Transaction,
        signer_keypairs: &[&dyn solana_sdk::signer::Signer],
    ) -> SwapResult<()> {
        let recent_blockhash = self.rpc_client
            .get_latest_blockhash()
            .await
            .map_err(SwapError::RpcError)?;

        transaction.message.recent_blockhash = recent_blockhash;
        transaction.signatures.clear();
        transaction.sign(signer_keypairs, recent_blockhash);

        debug!("Transaction blockhash refreshed: {}", recent_blockhash);
        Ok(())
    }

    /// Calculate retry delay with exponential backoff
    fn calculate_retry_delay(&self, attempt: u32) -> u64 {
        let delay = (self.retry_config.base_delay_ms as f64 
            * self.retry_config.backoff_multiplier.powi(attempt as i32 - 1)) as u64;
        
        delay.min(self.retry_config.max_delay_ms)
    }

    /// Analyze balance changes for a confirmed transaction
    pub async fn analyze_balance_changes(
        &self,
        signature: &Signature,
    ) -> SwapResult<Vec<BalanceChange>> {
        utils::get_all_balance_changes(&self.rpc_client, signature).await
    }

    /// Get balance change for a specific account
    pub async fn get_balance_change_for_account(
        &self,
        signature: &Signature,
        account: &Pubkey,
    ) -> SwapResult<i64> {
        utils::get_account_balance_change(&self.rpc_client, signature, account).await
    }

    /// Check if error is retryable
    fn is_retryable_error(&self, error: &SwapError) -> bool {
        match error {
            SwapError::RpcError(e) => {
                let error_str = e.to_string().to_lowercase();
                
                // Retryable network/RPC errors
                error_str.contains("timeout") ||
                error_str.contains("connection") ||
                error_str.contains("network") ||
                error_str.contains("service unavailable") ||
                error_str.contains("too many requests") ||
                error_str.contains("blockhash not found") ||
                error_str.contains("transaction was not confirmed")
            }
            SwapError::TransactionFailed(msg) => {
                let msg_lower = msg.to_lowercase();
                
                // Retryable transaction errors
                msg_lower.contains("blockhash not found") ||
                msg_lower.contains("transaction was not confirmed") ||
                msg_lower.contains("block height exceeded")
            }
            SwapError::Other(msg) => {
                let msg_lower = msg.to_lowercase();
                
                // Retryable generic errors
                msg_lower.contains("timeout") ||
                msg_lower.contains("network") ||
                msg_lower.contains("connection")
            }
            SwapError::Timeout(_) => true,
            SwapError::NetworkError(_) => true,
            // Don't retry simulation, parsing, or config errors
            SwapError::SimulationFailed(_) => false,
            SwapError::ParseError(_) => false,
            SwapError::ConfigError(_) => false,
            SwapError::InsufficientBalance(_) => false,
            SwapError::InsufficientLiquidity { .. } => false,
            SwapError::SlippageExceeded { .. } => false,
            SwapError::NoPoolsFound(_, _) => false,
            SwapError::UnsupportedPoolType(_) => false,
            SwapError::InvalidTokenMint(_) => false,
            SwapError::InvalidAmount(_) => false,
            SwapError::SerializationError(_) => false,
            SwapError::MathOverflow => false,
            SwapError::InvalidPoolState(_) => false,
            SwapError::CacheError(_) => false,
            SwapError::ParsePubkeyError(_) => false,
            SwapError::PoolNotFound(_) => false,
            SwapError::TokenNotFound(_) => false,
            SwapError::InvalidSlippage(_) => false,
            SwapError::InvalidPoolType(_) => false,
            SwapError::PoolNotActive => false,
            SwapError::InvalidInput(_) => false,
        }
    }

    /// Get transaction details for analysis
    pub async fn get_transaction_details(
        &self,
        signature: &Signature,
    ) -> SwapResult<solana_transaction_status::UiTransactionEncoding> {
        let config = RpcTransactionConfig {
            encoding: Some(UiTransactionEncoding::Json),
            commitment: Some(CommitmentConfig::confirmed()),
            max_supported_transaction_version: Some(0),
        };
        
        match self.rpc_client
            .get_transaction_with_config(signature, config)
            .await
        {
            Ok(_transaction) => Ok(UiTransactionEncoding::Json),
            Err(e) => Err(SwapError::RpcError(e)),
        }
    }

    /// Extract token balance changes from transaction
    pub async fn get_token_balance_changes(
        &self,
        signature: &Signature,
    ) -> SwapResult<Vec<(String, i64)>> {
        // This would parse transaction logs to find token balance changes
        // For now, return empty vector as placeholder
        // TODO
        debug!("Getting token balance changes for transaction: {}", signature);
        Ok(vec![])
    }

    /// Send versioned transaction and monitor its status
    async fn send_and_monitor_versioned_transaction(&self, transaction: &VersionedTransaction) -> SwapResult<Signature> {
        debug!("Sending versioned transaction...");
        
        // Send transaction
        let signature = self.rpc_client
            .send_transaction(transaction)
            .await
            .map_err(SwapError::RpcError)?;

        info!("Versioned transaction sent: {}", signature);

        // Monitor confirmation
        self.monitor_confirmation(&signature).await?;

        Ok(signature)
    }

    /// Refresh versioned transaction blockhash and re-sign
    async fn refresh_versioned_transaction_blockhash(
        &self,
        transaction: &mut VersionedTransaction,
        signer_keypairs: &[&dyn solana_sdk::signer::Signer],
    ) -> SwapResult<()> {
        let recent_blockhash = self.rpc_client
            .get_latest_blockhash()
            .await
            .map_err(SwapError::RpcError)?;

        // Update blockhash in the versioned message
        match &mut transaction.message {
            solana_sdk::message::VersionedMessage::Legacy(message) => {
                message.recent_blockhash = recent_blockhash;
            }
            solana_sdk::message::VersionedMessage::V0(_message) => {
                // For v0 messages, we need to rebuild the transaction
                // as the blockhash is part of the compiled message
                return Err(SwapError::Other(
                    "Cannot refresh blockhash for v0 transactions directly. Rebuild required.".to_string()
                ));
            }
        }

        // Clear and re-sign
        transaction.signatures.clear();
        transaction.signatures.resize(signer_keypairs.len(), Default::default());
        
        // Sign the transaction
        for (i, signer) in signer_keypairs.iter().enumerate() {
            transaction.signatures[i] = signer.sign_message(&transaction.message.serialize());
        }

        debug!("Versioned transaction blockhash refreshed: {}", recent_blockhash);
        Ok(())
    }
}

/// Transaction monitoring utilities
pub mod utils {
    use super::*;
    use solana_sdk::pubkey::Pubkey;

    /// Calculate transaction fee
    pub async fn calculate_transaction_fee(
        rpc_client: &RpcClient,
        signature: &Signature,
    ) -> SwapResult<u64> {
        let config = RpcTransactionConfig {
            encoding: Some(UiTransactionEncoding::Json),
            commitment: Some(CommitmentConfig::confirmed()),
            max_supported_transaction_version: Some(0),
        };
        
        match rpc_client.get_transaction_with_config(signature, config).await {
            Ok(transaction) => {
                if let Some(meta) = transaction.transaction.meta {
                    Ok(meta.fee)
                } else {
                    Err(SwapError::Other("No transaction metadata available".to_string()))
                }
            }
            Err(e) => Err(SwapError::RpcError(e)),
        }
    }

    /// Get account balance change for specific token
    pub async fn get_account_balance_change(
        rpc_client: &RpcClient,
        signature: &Signature,
        account: &Pubkey,
    ) -> SwapResult<i64> {
        let config = RpcTransactionConfig {
            encoding: Some(UiTransactionEncoding::Json),
            commitment: Some(CommitmentConfig::confirmed()),
            max_supported_transaction_version: Some(0),
        };
        
        match rpc_client.get_transaction_with_config(signature, config).await {
            Ok(transaction) => {
                if let Some(meta) = transaction.transaction.meta {
                    // Parse balance changes from transaction metadata
                    use solana_transaction_status::option_serializer::OptionSerializer;
                    
                    // Check token balance changes
                    let (pre_balances, post_balances) = match (&meta.pre_token_balances, &meta.post_token_balances) {
                        (OptionSerializer::Some(pre), OptionSerializer::Some(post)) => (pre, post),
                        _ => {
                            debug!("No token balance data available for transaction");
                            return Ok(0);
                        }
                    };
                    
                    // Find the account in the token balances
                    for post_balance in post_balances {
                        // Check if this is the account we're looking for
                        if let OptionSerializer::Some(owner) = &post_balance.owner {
                            if owner == &account.to_string() {
                                // Find corresponding pre-balance
                                if let Some(pre_balance) = pre_balances.iter()
                                    .find(|pb| pb.account_index == post_balance.account_index) {
                                    
                                    let pre_amount = pre_balance.ui_token_amount.amount.parse::<i64>().unwrap_or(0);
                                    let post_amount = post_balance.ui_token_amount.amount.parse::<i64>().unwrap_or(0);
                                    let change = post_amount - pre_amount;
                                    
                                    debug!("Account {} balance change: {} -> {} (change: {})", 
                                        account, pre_amount, post_amount, change);
                                    
                                    return Ok(change);
                                } else {
                                    // New token account created
                                    let post_amount = post_balance.ui_token_amount.amount.parse::<i64>().unwrap_or(0);
                                    debug!("New token account {} created with balance: {}", account, post_amount);
                                    return Ok(post_amount);
                                }
                            }
                        }
                    }
                    
                    // Also check SOL balance changes if no token balance found
                    let pre_sol = &meta.pre_balances;
                    let post_sol = &meta.post_balances;
                    
                    // Find account index
                    match &transaction.transaction.transaction {
                            solana_transaction_status::EncodedTransaction::Json(tx) => {
                                match &tx.message {
                                    solana_transaction_status::UiMessage::Raw(raw_msg) => {
                                        let account_keys = &raw_msg.account_keys;
                                        for (idx, key) in account_keys.iter().enumerate() {
                                            if key == &account.to_string() {
                                                if idx < pre_sol.len() && idx < post_sol.len() {
                                                    let change = post_sol[idx] as i64 - pre_sol[idx] as i64;
                                                    debug!("Account {} SOL balance change: {}", account, change);
                                                    return Ok(change);
                                                }
                                            }
                                        }
                                    }
                                    _ => {}
                                }
                            }
                            _ => {}
                        }
                    
                    debug!("No balance change found for account: {}", account);
                    Ok(0)
                } else {
                    Err(SwapError::Other("No transaction metadata available".to_string()))
                }
            }
            Err(e) => Err(SwapError::RpcError(e)),
        }
    }

    /// Get all balance changes in a transaction
    pub async fn get_all_balance_changes(
        rpc_client: &RpcClient,
        signature: &Signature,
    ) -> SwapResult<Vec<BalanceChange>> {
        let config = RpcTransactionConfig {
            encoding: Some(UiTransactionEncoding::Json),
            commitment: Some(CommitmentConfig::confirmed()),
            max_supported_transaction_version: Some(0),
        };
        
        let mut balance_changes = Vec::new();
        
        match rpc_client.get_transaction_with_config(signature, config).await {
            Ok(transaction) => {
                if let Some(meta) = transaction.transaction.meta {
                    use solana_transaction_status::option_serializer::OptionSerializer;
                    
                    // Get account keys
                    let account_keys: Vec<String> = match &transaction.transaction.transaction {
                            solana_transaction_status::EncodedTransaction::Json(json_tx) => {
                                match &json_tx.message {
                                    solana_transaction_status::UiMessage::Parsed(_) => vec![],
                                    solana_transaction_status::UiMessage::Raw(raw_msg) => {
                                        raw_msg.account_keys.clone()
                                    }
                                }
                            }
                            _ => vec![]
                        };
                    
                    // Parse token balance changes
                    if let (OptionSerializer::Some(pre_token), OptionSerializer::Some(post_token)) = 
                        (&meta.pre_token_balances, &meta.post_token_balances) {
                        
                        for post_balance in post_token {
                            let pre_balance = pre_token.iter()
                                .find(|pb| pb.account_index == post_balance.account_index);
                            
                            let pre_amount = pre_balance
                                .map(|pb| pb.ui_token_amount.amount.parse::<i64>().unwrap_or(0))
                                .unwrap_or(0);
                            let post_amount = post_balance.ui_token_amount.amount.parse::<i64>().unwrap_or(0);
                            let change = post_amount - pre_amount;
                            
                            if change != 0 {
                                let account_pubkey = account_keys.get(post_balance.account_index as usize)
                                    .and_then(|s| Pubkey::from_str(s).ok())
                                    .unwrap_or_default();
                                
                                balance_changes.push(BalanceChange {
                                    account: account_pubkey,
                                    mint: Pubkey::from_str(&post_balance.mint).ok(),
                                    pre_balance: pre_amount as u64,
                                    post_balance: post_amount as u64,
                                    change,
                                    decimals: post_balance.ui_token_amount.decimals,
                                });
                            }
                        }
                    }
                    
                    // Parse SOL balance changes
                    let pre_sol = &meta.pre_balances;
                    let post_sol = &meta.post_balances;
                    
                    for (idx, (pre, post)) in pre_sol.iter().zip(post_sol.iter()).enumerate() {
                        let change = *post as i64 - *pre as i64;
                        if change != 0 && idx < account_keys.len() {
                            if let Ok(account) = Pubkey::from_str(&account_keys[idx]) {
                                balance_changes.push(BalanceChange {
                                    account,
                                    mint: None, // Native SOL
                                    pre_balance: *pre,
                                    post_balance: *post,
                                    change,
                                    decimals: 9, // SOL has 9 decimals
                                });
                            }
                        }
                    }
                    
                    Ok(balance_changes)
                } else {
                    Err(SwapError::Other("No transaction metadata available".to_string()))
                }
            }
            Err(e) => Err(SwapError::RpcError(e)),
        }
    }

    /// Check if transaction was successful
    pub async fn is_transaction_successful(
        rpc_client: &RpcClient,
        signature: &Signature,
    ) -> SwapResult<bool> {
        match rpc_client.get_signature_status(signature).await {
            Ok(Some(status)) => Ok(status.is_ok()),
            Ok(None) => Ok(false),
            Err(e) => Err(SwapError::RpcError(e)),
        }
    }
}