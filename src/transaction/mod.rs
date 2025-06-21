pub mod amm_swap;

use crate::core::{
    constants::AMM_V4_PROGRAM,
    PoolType, SwapError, SwapParams, SwapResult, TransactionResult,
};
use chrono::Utc;
use log::{debug, info};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    instruction::Instruction,
    pubkey::Pubkey,
    signature::{Keypair, Signature},
    signer::Signer,
    transaction::Transaction,
};

/// Transaction executor for different pool types
pub struct TransactionExecutor {
    rpc_client: RpcClient,
    keypair: Keypair,
}

impl TransactionExecutor {
    pub fn new(rpc_url: String, keypair: Keypair) -> Self {
        let rpc_client = RpcClient::new_with_commitment(
            rpc_url,
            CommitmentConfig::confirmed(),
        );

        Self {
            rpc_client,
            keypair,
        }
    }

    /// Execute a swap transaction
    pub async fn execute_swap(&self, params: SwapParams) -> SwapResult<TransactionResult> {
        info!(
            "Executing swap on {:?} pool {}",
            params.quote.pool_info.pool_type, params.quote.pool_info.address
        );

        // Build pool-specific instruction
        let swap_instruction = self.build_swap_instruction(&params).await?;

        // Get recent blockhash
        let recent_blockhash = self.rpc_client.get_latest_blockhash().await?;

        // Create transaction
        let mut transaction = Transaction::new_with_payer(
            &[swap_instruction],
            Some(&self.keypair.pubkey()),
        );
        transaction.sign(&[&self.keypair], recent_blockhash);

        // Simulate transaction first
        debug!("Simulating transaction...");
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

        // Send transaction
        info!("Sending transaction...");
        let signature = self
            .rpc_client
            .send_and_confirm_transaction(&transaction)
            .await?;

        info!("Transaction confirmed: {}", signature);

        // Get transaction details to calculate actual slippage
        let actual_amount_out = self.get_actual_output_amount(&signature).await?;
        let actual_slippage = calculate_actual_slippage(
            params.quote.amount_out,
            actual_amount_out,
        );

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
        // Get pool account data to extract all necessary accounts
        let pool_data = self.rpc_client
            .get_account_data(&params.quote.pool_info.address)
            .await?;
        
        // Build the swap instruction using the pool state
        amm_swap::build_amm_swap_instruction_with_state(
            params,
            &self.keypair.pubkey(),
            &AMM_V4_PROGRAM,
            &pool_data,
        ).await
    }

    /// Build Stable swap instruction
    async fn build_stable_swap_instruction(&self, params: &SwapParams) -> SwapResult<Instruction> {
        // TODO: Implement stable pool swap instruction
        
        Err(SwapError::Other(
            "Stable swap instruction building not yet implemented".to_string(),
        ))
    }

    /// Build CLMM swap instruction
    async fn build_clmm_swap_instruction(&self, params: &SwapParams) -> SwapResult<Instruction> {
        // TODO: Implement CLMM swap instruction
        
        Err(SwapError::Other(
            "CLMM swap instruction building not yet implemented".to_string(),
        ))
    }

    /// Build Standard swap instruction
    async fn build_standard_swap_instruction(&self, params: &SwapParams) -> SwapResult<Instruction> {
        // TODO: Implement standard pool swap instruction
        
        Err(SwapError::Other(
            "Standard swap instruction building not yet implemented".to_string(),
        ))
    }

    /// Get actual output amount from transaction
    async fn get_actual_output_amount(&self, signature: &Signature) -> SwapResult<u64> {
        // TODO: Parse transaction to get actual output amount
        // This would involve:
        // 1. Getting transaction details
        // 2. Parsing token balance changes
        // 3. Calculating actual output
        
        // For now, return a placeholder
        Ok(0)
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