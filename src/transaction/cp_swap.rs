use crate::core::{SwapError, SwapParams, SwapResult};
use borsh::{BorshDeserialize, BorshSerialize};
use log::{debug, info};
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    sysvar,
};
use spl_associated_token_account::get_associated_token_address;
use spl_token::ID as TOKEN_PROGRAM_ID;

/// CP Swap instruction discriminator
const CP_SWAP_BASE_IN: u8 = 0;
const CP_SWAP_BASE_OUT: u8 = 1;

/// CP Swap instruction data layout
#[derive(Debug, Clone, BorshSerialize, BorshDeserialize)]
pub struct CpSwapInstructionData {
    /// Instruction discriminator
    pub instruction: u8,
    /// Amount (input or output depending on instruction)
    pub amount: u64,
    /// Other side amount (minimum out or maximum in)
    pub other_amount: u64,
}

impl CpSwapInstructionData {
    pub fn new_base_in(amount_in: u64, min_amount_out: u64) -> Self {
        Self {
            instruction: CP_SWAP_BASE_IN,
            amount: amount_in,
            other_amount: min_amount_out,
        }
    }

    pub fn new_base_out(amount_out: u64, max_amount_in: u64) -> Self {
        Self {
            instruction: CP_SWAP_BASE_OUT,
            amount: amount_out,
            other_amount: max_amount_in,
        }
    }
}

/// Build CP/Standard swap instruction for Raydium CP Swap
pub async fn build_cp_swap_instruction(
    params: &SwapParams,
    user_pubkey: &Pubkey,
    cp_program_id: &Pubkey,
    pool_account_data: &[u8],
) -> SwapResult<Instruction> {
    // Extract pool info
    let pool_address = params.quote.pool_info.address;
    
    // Parse pool state to get vault addresses
    let pool_state = crate::core::layouts::CpSwapPoolState::from_bytes(pool_account_data)
        .map_err(|e| SwapError::ParseError(e))?;
    
    info!("Building CP swap instruction for pool {}", pool_address);
    info!("Pool token mints - 0: {}, 1: {}", pool_state.token_0_mint, pool_state.token_1_mint);
    
    // Determine swap direction
    let (is_base_input, token_0_to_1) = if params.token_in == pool_state.token_0_mint {
        (true, true)
    } else if params.token_in == pool_state.token_1_mint {
        (true, false)
    } else {
        return Err(SwapError::InvalidTokenMint("Input token not found in pool".to_string()));
    };
    
    info!("Swap direction: token_0_to_1 = {}", token_0_to_1);
    
    // Get user token accounts
    let user_input_token = get_associated_token_address(user_pubkey, &params.token_in);
    let user_output_token = get_associated_token_address(user_pubkey, &params.token_out);
    
    info!("User input token account: {}", user_input_token);
    info!("User output token account: {}", user_output_token);
    
    // Create instruction data (always use base_in for simplicity)
    let data = CpSwapInstructionData::new_base_in(
        params.quote.amount_in,
        params.quote.min_amount_out,
    );
    
    // Serialize instruction data
    let mut instruction_data = Vec::new();
    data.serialize(&mut instruction_data)?;
    
    // Derive authority PDA
    let (authority, _bump) = Pubkey::find_program_address(
        &[b"cp_swap_program", &pool_address.to_bytes()],
        cp_program_id,
    );
    
    info!("Pool authority: {}", authority);
    
    // Build accounts list
    // The order is critical for CP swap instruction
    let accounts = vec![
        // 0. Pool config account (if exists, otherwise system program)
        AccountMeta::new_readonly(sysvar::ID, false), // Using system program as placeholder
        // 1. Pool state account
        AccountMeta::new(pool_address, false),
        // 2. Authority
        AccountMeta::new_readonly(authority, false),
        // 3. User input token account
        AccountMeta::new(user_input_token, false),
        // 4. User output token account  
        AccountMeta::new(user_output_token, false),
        // 5. Pool vault 0
        AccountMeta::new(pool_state.token_0_vault, false),
        // 6. Pool vault 1
        AccountMeta::new(pool_state.token_1_vault, false),
        // 7. Token program
        AccountMeta::new_readonly(TOKEN_PROGRAM_ID, false),
        // 8. User owner (signer)
        AccountMeta::new_readonly(*user_pubkey, true),
    ];
    
    Ok(Instruction {
        program_id: *cp_program_id,
        accounts,
        data: instruction_data,
    })
}