use crate::core::{error::SwapError, SwapParams, SwapResult, PoolState, constants::STABLE_PROGRAM};
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    sysvar,
};
use borsh::{BorshDeserialize, BorshSerialize};
use log::debug;

/// Raydium Stable swap instruction data
#[derive(Debug, BorshSerialize, BorshDeserialize)]
struct StableSwapInstructionData {
    /// Instruction discriminator for Stable swap
    instruction: u8,
    /// Amount of input token
    amount_in: u64,
    /// Minimum amount of output token
    minimum_amount_out: u64,
}

/// Build Raydium Stable swap instruction
pub async fn build_stable_swap_instruction(
    params: &SwapParams,
    user_pubkey: &Pubkey,
    pool_program: &Pubkey,
    _pool_data: &[u8],
) -> SwapResult<Instruction> {
    debug!("Building Stable swap instruction");
    
    let pool_info = &params.quote.pool_info;
    
    // Extract pool state
    let (_reserves, _amp_factor) = match &pool_info.pool_state {
        PoolState::Stable { reserves, amp_factor } => (reserves, *amp_factor),
        _ => return Err(SwapError::InvalidPoolState("Expected Stable pool state".to_string())),
    };
    
    // Use token mints from params
    let token_in = params.token_in;
    let token_out = params.token_out;
    
    // Determine token indices  
    let (token_in_index, _token_out_index) = if pool_info.token_a.mint == token_in {
        (0, 1)
    } else {
        (1, 0)
    };
    
    debug!(
        "Stable swap: {} -> {}, amount: {}, min_out: {}",
        if token_in_index == 0 { &pool_info.token_a.symbol } else { &pool_info.token_b.symbol },
        if token_in_index == 0 { &pool_info.token_b.symbol } else { &pool_info.token_a.symbol },
        params.quote.amount_in,
        params.quote.min_amount_out
    );
    
    // Get user token accounts
    let user_token_in = get_associated_token_address(user_pubkey, &token_in);
    let user_token_out = get_associated_token_address(user_pubkey, &token_out);
    
    // Derive pool accounts
    let pool_token_accounts = derive_stable_pool_token_accounts(&pool_info.address)?;
    let pool_authority = derive_stable_pool_authority(&pool_info.address)?;
    
    // Stable swap instruction discriminator is typically 1
    let instruction_data = StableSwapInstructionData {
        instruction: 1, // Swap instruction
        amount_in: params.quote.amount_in,
        minimum_amount_out: params.quote.min_amount_out,
    };
    
    let mut data = Vec::new();
    instruction_data.serialize(&mut data)
        .map_err(|e| SwapError::SerializationError(e.to_string()))?;
    
    // Build accounts for stable swap
    // Account order for Raydium Stable:
    // 0. [signer] User
    // 1. [] Pool state account
    // 2. [] Pool authority
    // 3. [writable] User token in account
    // 4. [writable] User token out account
    // 5. [writable] Pool token A vault
    // 6. [writable] Pool token B vault
    // 7. [] Token program
    // 8. [] Clock sysvar (for time-based checks)
    
    let accounts = vec![
        AccountMeta::new_readonly(*user_pubkey, true), // User (signer)
        AccountMeta::new_readonly(pool_info.address, false), // Pool state
        AccountMeta::new_readonly(pool_authority, false), // Pool authority
        AccountMeta::new(user_token_in, false), // User token in
        AccountMeta::new(user_token_out, false), // User token out
        AccountMeta::new(pool_token_accounts.0, false), // Pool token A vault
        AccountMeta::new(pool_token_accounts.1, false), // Pool token B vault
        AccountMeta::new_readonly(spl_token::id(), false), // Token program
        AccountMeta::new_readonly(sysvar::clock::id(), false), // Clock sysvar
    ];
    
    Ok(Instruction {
        program_id: *pool_program,
        accounts,
        data,
    })
}

/// Derive stable pool token vault accounts
fn derive_stable_pool_token_accounts(pool_address: &Pubkey) -> SwapResult<(Pubkey, Pubkey)> {
    // In Raydium Stable, token vaults are PDAs derived from pool address
    // Seeds: [pool_address, token_index]
    
    let (token_a_vault, _) = Pubkey::find_program_address(
        &[pool_address.as_ref(), &[0u8]],
        &STABLE_PROGRAM,
    );
    
    let (token_b_vault, _) = Pubkey::find_program_address(
        &[pool_address.as_ref(), &[1u8]],
        &STABLE_PROGRAM,
    );
    
    Ok((token_a_vault, token_b_vault))
}

/// Derive stable pool authority
fn derive_stable_pool_authority(pool_address: &Pubkey) -> SwapResult<Pubkey> {
    // Pool authority is a PDA
    let (authority, _) = Pubkey::find_program_address(
        &[pool_address.as_ref()],
        &STABLE_PROGRAM,
    );
    
    Ok(authority)
}

/// Get associated token address
fn get_associated_token_address(wallet: &Pubkey, mint: &Pubkey) -> Pubkey {
    spl_associated_token_account::get_associated_token_address(wallet, mint)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{QuoteResult, PoolInfo, PoolType, TokenInfo};
    
    #[test]
    fn test_stable_swap_instruction_serialization() {
        let instruction_data = StableSwapInstructionData {
            instruction: 1,
            amount_in: 1_000_000_000,
            minimum_amount_out: 900_000_000,
        };
        
        let mut serialized = Vec::new();
        instruction_data.serialize(&mut serialized).unwrap();
        assert!(!serialized.is_empty());
        
        // Verify we can deserialize
        let deserialized = StableSwapInstructionData::try_from_slice(&serialized).unwrap();
        assert_eq!(deserialized.instruction, 1);
        assert_eq!(deserialized.amount_in, 1_000_000_000);
        assert_eq!(deserialized.minimum_amount_out, 900_000_000);
    }
}