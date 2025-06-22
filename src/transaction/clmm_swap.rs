use crate::core::{error::SwapError, SwapParams, SwapResult, PoolState, constants::CLMM_PROGRAM};
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    sysvar,
};
use borsh::{BorshDeserialize, BorshSerialize};
use log::debug;

/// Raydium CLMM swap instruction data
#[derive(Debug, BorshSerialize, BorshDeserialize)]
struct ClmmSwapInstructionData {
    /// Instruction discriminator for CLMM swap
    instruction: [u8; 8], // Anchor discriminator
    /// Amount of input token
    amount: u64,
    /// Minimum amount of output token  
    other_amount_threshold: u64,
    /// Square root price limit
    sqrt_price_limit_x64: u128,
    /// Is base input (true if swapping base to quote)
    is_base_input: bool,
}

/// Build Raydium CLMM swap instruction
pub async fn build_clmm_swap_instruction(
    params: &SwapParams,
    user_pubkey: &Pubkey,
    pool_program: &Pubkey,
    _pool_data: &[u8],
) -> SwapResult<Instruction> {
    debug!("Building CLMM swap instruction");
    
    let pool_info = &params.quote.pool_info;
    
    // Extract pool state
    let (current_tick, tick_spacing, _liquidity) = match &pool_info.pool_state {
        PoolState::CLMM { current_tick, tick_spacing, liquidity, .. } => 
            (*current_tick, *tick_spacing, *liquidity),
        _ => return Err(SwapError::InvalidPoolState("Expected CLMM pool state".to_string())),
    };
    
    // Determine swap direction
    let is_base_input = pool_info.token_a.mint == params.token_in;
    
    debug!(
        "CLMM swap: {} -> {}, amount: {}, min_out: {}, is_base_input: {}",
        if is_base_input { &pool_info.token_a.symbol } else { &pool_info.token_b.symbol },
        if is_base_input { &pool_info.token_b.symbol } else { &pool_info.token_a.symbol },
        params.quote.amount_in,
        params.quote.min_amount_out,
        is_base_input
    );
    
    // Get user token accounts
    let user_token_in = get_associated_token_address(user_pubkey, &params.token_in);
    let user_token_out = get_associated_token_address(user_pubkey, &params.token_out);
    
    // Derive pool accounts
    let pool_token_vault_0 = derive_clmm_token_vault(&pool_info.address, 0)?;
    let pool_token_vault_1 = derive_clmm_token_vault(&pool_info.address, 1)?;
    let tick_array_0 = derive_tick_array_address(&pool_info.address, current_tick, tick_spacing)?;
    let tick_array_1 = derive_tick_array_address(&pool_info.address, current_tick + tick_spacing as i32, tick_spacing)?;
    let tick_array_2 = derive_tick_array_address(&pool_info.address, current_tick - tick_spacing as i32, tick_spacing)?;
    
    // CLMM swap instruction discriminator
    // This is the Anchor discriminator for "swap" instruction
    let discriminator: [u8; 8] = [248, 198, 158, 145, 225, 117, 135, 200]; // Anchor IDL hash for "swap"
    
    // No sqrt price limit (0 means no limit)
    let sqrt_price_limit_x64 = 0u128;
    
    let instruction_data = ClmmSwapInstructionData {
        instruction: discriminator,
        amount: params.quote.amount_in,
        other_amount_threshold: params.quote.min_amount_out,
        sqrt_price_limit_x64,
        is_base_input,
    };
    
    let mut data = Vec::new();
    instruction_data.serialize(&mut data)
        .map_err(|e| SwapError::SerializationError(e.to_string()))?;
    
    // Build accounts for CLMM swap
    // Account order for Raydium CLMM:
    // 0. [signer] Payer (user)
    // 1. [] Pool state
    // 2. [writable] Token vault 0
    // 3. [writable] Token vault 1  
    // 4. [writable] Tick array 0
    // 5. [writable] Tick array 1
    // 6. [writable] Tick array 2
    // 7. [writable] User token account A
    // 8. [writable] User token account B
    // 9. [] Token program
    // 10. [] Associated token program
    // 11. [] System program
    // 12. [] Rent sysvar
    
    let accounts = vec![
        AccountMeta::new(*user_pubkey, true), // Payer
        AccountMeta::new_readonly(pool_info.address, false), // Pool state
        AccountMeta::new(pool_token_vault_0, false), // Token vault 0
        AccountMeta::new(pool_token_vault_1, false), // Token vault 1
        AccountMeta::new(tick_array_0, false), // Tick array 0
        AccountMeta::new(tick_array_1, false), // Tick array 1
        AccountMeta::new(tick_array_2, false), // Tick array 2
        AccountMeta::new(user_token_in, false), // User token in
        AccountMeta::new(user_token_out, false), // User token out
        AccountMeta::new_readonly(spl_token::id(), false), // Token program
        AccountMeta::new_readonly(spl_associated_token_account::id(), false), // Associated token program
        AccountMeta::new_readonly(solana_sdk::system_program::id(), false), // System program
        AccountMeta::new_readonly(sysvar::rent::id(), false), // Rent sysvar
    ];
    
    Ok(Instruction {
        program_id: *pool_program,
        accounts,
        data,
    })
}

/// Derive CLMM token vault
fn derive_clmm_token_vault(pool_address: &Pubkey, index: u8) -> SwapResult<Pubkey> {
    let (vault, _) = Pubkey::find_program_address(
        &[b"pool_vault", pool_address.as_ref(), &[index]],
        &CLMM_PROGRAM,
    );
    Ok(vault)
}

/// Derive tick array address
fn derive_tick_array_address(pool_address: &Pubkey, tick: i32, tick_spacing: u16) -> SwapResult<Pubkey> {
    // Calculate start index for tick array
    let ticks_per_array = 60; // CLMM uses 60 ticks per array
    let array_index = tick / (ticks_per_array * tick_spacing as i32);
    let start_tick = array_index * ticks_per_array * tick_spacing as i32;
    
    let (tick_array, _) = Pubkey::find_program_address(
        &[
            b"tick_array",
            pool_address.as_ref(),
            &start_tick.to_le_bytes(),
        ],
        &CLMM_PROGRAM,
    );
    
    Ok(tick_array)
}

/// Get associated token address
fn get_associated_token_address(wallet: &Pubkey, mint: &Pubkey) -> Pubkey {
    spl_associated_token_account::get_associated_token_address(wallet, mint)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_clmm_swap_instruction_serialization() {
        let instruction_data = ClmmSwapInstructionData {
            instruction: [248, 198, 158, 145, 225, 117, 135, 200],
            amount: 1_000_000_000,
            other_amount_threshold: 900_000_000,
            sqrt_price_limit_x64: 0,
            is_base_input: true,
        };
        
        let mut serialized = Vec::new();
        instruction_data.serialize(&mut serialized).unwrap();
        assert!(!serialized.is_empty());
        
        // Verify discriminator is at the beginning
        assert_eq!(&serialized[0..8], &[248, 198, 158, 145, 225, 117, 135, 200]);
    }
    
    #[test]
    fn test_tick_array_calculation() {
        let pool = Pubkey::new_unique();
        let tick = 1234;
        let tick_spacing = 10;
        
        let tick_array = derive_tick_array_address(&pool, tick, tick_spacing).unwrap();
        assert_ne!(tick_array, Pubkey::default());
        
        // Test negative tick
        let tick_array_neg = derive_tick_array_address(&pool, -1234, tick_spacing).unwrap();
        assert_ne!(tick_array_neg, Pubkey::default());
    }
}