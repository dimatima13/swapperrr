use crate::core::{SwapError, SwapResult};
use log::{debug, info};
use solana_sdk::{
    instruction::Instruction,
    native_token::LAMPORTS_PER_SOL,
    program_pack::Pack,
    pubkey::Pubkey,
    system_instruction,
};
use spl_associated_token_account::get_associated_token_address;
use spl_token::instruction as token_instruction;

/// Create instructions to wrap native SOL into wSOL
pub fn create_wrap_sol_instructions(
    user_pubkey: &Pubkey,
    amount: u64,
    create_ata_if_needed: bool,
) -> SwapResult<Vec<Instruction>> {
    let mut instructions = vec![];
    let wsol_mint = spl_token::native_mint::ID;
    let user_wsol_ata = get_associated_token_address(user_pubkey, &wsol_mint);
    
    // Create wSOL ATA if needed
    if create_ata_if_needed {
        info!("Creating wSOL associated token account");
        let create_ata_ix = spl_associated_token_account::instruction::create_associated_token_account(
            user_pubkey,
            user_pubkey,
            &wsol_mint,
            &spl_token::ID,
        );
        instructions.push(create_ata_ix);
    }
    
    // Transfer SOL to wSOL account
    info!("Transferring {} SOL to wSOL account", amount as f64 / LAMPORTS_PER_SOL as f64);
    let transfer_ix = system_instruction::transfer(
        user_pubkey,
        &user_wsol_ata,
        amount,
    );
    instructions.push(transfer_ix);
    
    // Sync native account to convert SOL to wSOL
    info!("Syncing native account to wrap SOL");
    let sync_native_ix = token_instruction::sync_native(&spl_token::ID, &user_wsol_ata)
        .map_err(|e| SwapError::Other(format!("Failed to create sync native instruction: {:?}", e)))?;
    instructions.push(sync_native_ix);
    
    Ok(instructions)
}

/// Create instruction to unwrap wSOL back to native SOL
pub fn create_unwrap_sol_instruction(
    user_pubkey: &Pubkey,
) -> SwapResult<Instruction> {
    let wsol_mint = spl_token::native_mint::ID;
    let user_wsol_ata = get_associated_token_address(user_pubkey, &wsol_mint);
    
    info!("Creating instruction to close wSOL account and unwrap to SOL");
    
    // Close wSOL account to unwrap all wSOL back to SOL
    let close_account_ix = token_instruction::close_account(
        &spl_token::ID,
        &user_wsol_ata,
        user_pubkey,
        user_pubkey,
        &[],
    ).map_err(|e| SwapError::Other(format!("Failed to create close account instruction: {:?}", e)))?;
    
    Ok(close_account_ix)
}

/// Check if we need to wrap SOL for a swap
pub async fn check_and_prepare_wsol_wrapping(
    rpc_client: &solana_client::nonblocking::rpc_client::RpcClient,
    user_pubkey: &Pubkey,
    amount_needed: u64,
) -> SwapResult<(bool, u64, Vec<Instruction>)> {
    let wsol_mint = spl_token::native_mint::ID;
    let user_wsol_ata = get_associated_token_address(user_pubkey, &wsol_mint);
    
    // Check if wSOL ATA exists and get balance
    let (wsol_exists, current_wsol_balance) = match rpc_client.get_account(&user_wsol_ata).await {
        Ok(account) => {
            match spl_token::state::Account::unpack(&account.data) {
                Ok(token_account) => {
                    debug!("Found existing wSOL account with balance: {}", token_account.amount);
                    (true, token_account.amount)
                }
                Err(_) => {
                    debug!("wSOL account exists but couldn't parse data");
                    (true, 0)
                }
            }
        }
        Err(_) => {
            debug!("wSOL account does not exist");
            (false, 0)
        }
    };
    
    // Check if we need to wrap more SOL
    if current_wsol_balance >= amount_needed {
        info!("Sufficient wSOL balance: {} >= {}", current_wsol_balance, amount_needed);
        return Ok((false, 0, vec![]));
    }
    
    let amount_to_wrap = amount_needed - current_wsol_balance;
    info!("Need to wrap {} more SOL (current wSOL: {}, needed: {})", 
        amount_to_wrap as f64 / LAMPORTS_PER_SOL as f64,
        current_wsol_balance as f64 / LAMPORTS_PER_SOL as f64,
        amount_needed as f64 / LAMPORTS_PER_SOL as f64
    );
    
    // Check native SOL balance
    let sol_balance = rpc_client.get_balance(user_pubkey).await?;
    
    // Reserve some SOL for transaction fees (0.01 SOL)
    let fee_reserve = LAMPORTS_PER_SOL / 100;
    let available_sol = sol_balance.saturating_sub(fee_reserve);
    
    if available_sol < amount_to_wrap {
        return Err(SwapError::InsufficientBalance(format!(
            "Insufficient SOL balance. Need {} SOL to wrap, but only have {} SOL available (after fees)",
            amount_to_wrap as f64 / LAMPORTS_PER_SOL as f64,
            available_sol as f64 / LAMPORTS_PER_SOL as f64
        )));
    }
    
    // Create wrapping instructions
    let wrap_instructions = create_wrap_sol_instructions(
        user_pubkey,
        amount_to_wrap,
        !wsol_exists,
    )?;
    
    Ok((true, amount_to_wrap, wrap_instructions))
}