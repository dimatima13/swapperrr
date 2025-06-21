use crate::core::{SwapError, SwapParams, SwapResult};
use borsh::{BorshDeserialize, BorshSerialize};
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    program_pack::Pack,
    pubkey::Pubkey,
    system_program,
    sysvar,
};
use spl_associated_token_account::get_associated_token_address;
use spl_token::ID as TOKEN_PROGRAM_ID;

/// AMM Swap instruction discriminator
const AMM_SWAP_INSTRUCTION: u8 = 9;

/// AMM Swap instruction data layout
#[derive(Debug, Clone, BorshSerialize, BorshDeserialize)]
pub struct AmmSwapInstructionData {
    /// Instruction discriminator
    pub instruction: u8,
    /// Amount of tokens to swap in
    pub amount_in: u64,
    /// Minimum amount of tokens to receive
    pub min_amount_out: u64,
}

impl AmmSwapInstructionData {
    pub fn new(amount_in: u64, min_amount_out: u64) -> Self {
        Self {
            instruction: AMM_SWAP_INSTRUCTION,
            amount_in,
            min_amount_out,
        }
    }
}

/// Build AMM swap instruction for Raydium AMM V4
pub async fn build_amm_swap_instruction(
    params: &SwapParams,
    user_pubkey: &Pubkey,
    amm_program_id: &Pubkey,
) -> SwapResult<Instruction> {
    // Extract pool info
    let pool_address = params.quote.pool_info.address;
    let pool_state = match &params.quote.pool_info.pool_state {
        crate::core::types::PoolState::AMM { .. } => &params.quote.pool_info.pool_state,
        _ => return Err(SwapError::InvalidPoolType("Expected AMM pool".to_string())),
    };

    // Get pool authority (PDA)
    let (authority, _nonce) = Pubkey::find_program_address(
        &[&pool_address.to_bytes()],
        amm_program_id,
    );

    // Determine swap direction
    let (source_mint, destination_mint) = if params.quote.pool_info.token_a.mint == params.quote.pool_info.token_a.mint {
        (
            &params.quote.pool_info.token_a.mint,
            &params.quote.pool_info.token_b.mint,
        )
    } else {
        (
            &params.quote.pool_info.token_b.mint,
            &params.quote.pool_info.token_a.mint,
        )
    };

    // Get user token accounts
    let user_source_token = get_associated_token_address(user_pubkey, source_mint);
    let user_destination_token = get_associated_token_address(user_pubkey, destination_mint);

    // Create instruction data
    let data = AmmSwapInstructionData::new(
        params.quote.amount_in,
        params.quote.min_amount_out,
    );

    // Serialize instruction data
    let mut instruction_data = Vec::new();
    data.serialize(&mut instruction_data)?;

    // Build accounts list in the order required by Raydium AMM V4
    let accounts = vec![
        // 0. Token program
        AccountMeta::new_readonly(TOKEN_PROGRAM_ID, false),
        // 1. AMM pool account
        AccountMeta::new(pool_address, false),
        // 2. AMM authority
        AccountMeta::new_readonly(authority, false),
        // 3. AMM open orders account
        AccountMeta::new(get_amm_open_orders(&pool_address, amm_program_id)?, false),
        // 4. AMM target orders account (can be same as AMM account in some versions)
        AccountMeta::new(pool_address, false),
        // 5. Pool coin vault
        AccountMeta::new(get_pool_coin_vault(&pool_address)?, false),
        // 6. Pool pc vault
        AccountMeta::new(get_pool_pc_vault(&pool_address)?, false),
        // 7. Serum market account
        AccountMeta::new(get_serum_market(&pool_address)?, false),
        // 8. Serum bids
        AccountMeta::new(get_serum_bids(&pool_address)?, false),
        // 9. Serum asks
        AccountMeta::new(get_serum_asks(&pool_address)?, false),
        // 10. Serum event queue
        AccountMeta::new(get_serum_event_queue(&pool_address)?, false),
        // 11. Serum coin vault
        AccountMeta::new(get_serum_coin_vault(&pool_address)?, false),
        // 12. Serum pc vault
        AccountMeta::new(get_serum_pc_vault(&pool_address)?, false),
        // 13. Serum vault signer
        AccountMeta::new_readonly(get_serum_vault_signer(&pool_address)?, false),
        // 14. User source token account
        AccountMeta::new(user_source_token, false),
        // 15. User destination token account
        AccountMeta::new(user_destination_token, false),
        // 16. User owner (signer)
        AccountMeta::new_readonly(*user_pubkey, true),
        // 17. Serum DEX program
        AccountMeta::new_readonly(get_serum_program_id(), false),
    ];

    Ok(Instruction {
        program_id: *amm_program_id,
        accounts,
        data: instruction_data,
    })
}

// Helper functions to derive associated accounts
// These would need to be implemented based on actual Raydium account derivation logic

fn get_amm_open_orders(pool_address: &Pubkey, program_id: &Pubkey) -> SwapResult<Pubkey> {
    // In real implementation, this would fetch from pool state or derive
    // For now, returning a placeholder
    let (pda, _) = Pubkey::find_program_address(
        &[
            b"amm_open_orders",
            &pool_address.to_bytes(),
        ],
        program_id,
    );
    Ok(pda)
}

fn get_pool_coin_vault(pool_address: &Pubkey) -> SwapResult<Pubkey> {
    // This should be fetched from the pool state
    // Placeholder implementation
    Err(SwapError::Other("Pool coin vault needs to be fetched from pool state".to_string()))
}

fn get_pool_pc_vault(pool_address: &Pubkey) -> SwapResult<Pubkey> {
    // This should be fetched from the pool state
    // Placeholder implementation
    Err(SwapError::Other("Pool pc vault needs to be fetched from pool state".to_string()))
}

fn get_serum_market(pool_address: &Pubkey) -> SwapResult<Pubkey> {
    // This should be fetched from the pool state
    // Placeholder implementation
    Err(SwapError::Other("Serum market needs to be fetched from pool state".to_string()))
}

fn get_serum_bids(pool_address: &Pubkey) -> SwapResult<Pubkey> {
    // This should be derived from serum market
    // Placeholder implementation
    Err(SwapError::Other("Serum bids needs to be derived from market".to_string()))
}

fn get_serum_asks(pool_address: &Pubkey) -> SwapResult<Pubkey> {
    // This should be derived from serum market
    // Placeholder implementation
    Err(SwapError::Other("Serum asks needs to be derived from market".to_string()))
}

fn get_serum_event_queue(pool_address: &Pubkey) -> SwapResult<Pubkey> {
    // This should be derived from serum market
    // Placeholder implementation
    Err(SwapError::Other("Serum event queue needs to be derived from market".to_string()))
}

fn get_serum_coin_vault(pool_address: &Pubkey) -> SwapResult<Pubkey> {
    // This should be derived from serum market
    // Placeholder implementation
    Err(SwapError::Other("Serum coin vault needs to be derived from market".to_string()))
}

fn get_serum_pc_vault(pool_address: &Pubkey) -> SwapResult<Pubkey> {
    // This should be derived from serum market
    // Placeholder implementation
    Err(SwapError::Other("Serum pc vault needs to be derived from market".to_string()))
}

fn get_serum_vault_signer(pool_address: &Pubkey) -> SwapResult<Pubkey> {
    // This should be derived from serum market
    // Placeholder implementation
    Err(SwapError::Other("Serum vault signer needs to be derived from market".to_string()))
}

fn get_serum_program_id() -> Pubkey {
    // Mainnet Serum DEX V3 program
    "9xQeWvG816bUx9EPjHmaT23yvVM2ZWbrrpZb9PusVFin".parse().unwrap()
}

/// Enhanced version that uses actual pool state data
pub async fn build_amm_swap_instruction_with_state(
    params: &SwapParams,
    user_pubkey: &Pubkey,
    amm_program_id: &Pubkey,
    pool_state_data: &[u8],
) -> SwapResult<Instruction> {
    use crate::core::layouts::AmmInfoLayoutV4;
    
    // Parse pool state
    let pool_state = AmmInfoLayoutV4::from_bytes(pool_state_data)
        .map_err(|e| SwapError::ParseError(e))?;
    
    // Check if pool is enabled
    if !pool_state.is_enabled() {
        return Err(SwapError::PoolNotActive);
    }
    
    // Determine swap direction based on mints
    let (is_coin_to_pc, source_mint, destination_mint) = 
        if params.quote.pool_info.token_a.mint == pool_state.coin_mint_address {
            (true, pool_state.coin_mint_address, pool_state.pc_mint_address)
        } else {
            (false, pool_state.pc_mint_address, pool_state.coin_mint_address)
        };

    // Get pool authority (PDA)
    let (authority, _nonce) = Pubkey::find_program_address(
        &[&params.quote.pool_info.address.to_bytes()],
        amm_program_id,
    );

    // Get user token accounts
    let user_source_token = get_associated_token_address(user_pubkey, &source_mint);
    let user_destination_token = get_associated_token_address(user_pubkey, &destination_mint);

    // Create instruction data
    let data = AmmSwapInstructionData::new(
        params.quote.amount_in,
        params.quote.min_amount_out,
    );

    // Serialize instruction data
    let mut instruction_data = Vec::new();
    data.serialize(&mut instruction_data)?;

    // Build accounts list using actual pool state data
    let accounts = vec![
        // 0. Token program
        AccountMeta::new_readonly(TOKEN_PROGRAM_ID, false),
        // 1. AMM pool account
        AccountMeta::new(params.quote.pool_info.address, false),
        // 2. AMM authority
        AccountMeta::new_readonly(authority, false),
        // 3. AMM open orders account
        AccountMeta::new(pool_state.amm_open_orders, false),
        // 4. AMM target orders account
        AccountMeta::new(pool_state.amm_target_orders, false),
        // 5. Pool coin vault
        AccountMeta::new(pool_state.pool_coin_token_account, false),
        // 6. Pool pc vault
        AccountMeta::new(pool_state.pool_pc_token_account, false),
        // 7. Serum market account
        AccountMeta::new(pool_state.serum_market, false),
        // 8. Serum bids (derived from market - placeholder)
        AccountMeta::new(derive_serum_bids(&pool_state.serum_market, &pool_state.serum_program_id), false),
        // 9. Serum asks (derived from market - placeholder)
        AccountMeta::new(derive_serum_asks(&pool_state.serum_market, &pool_state.serum_program_id), false),
        // 10. Serum event queue (derived from market - placeholder)
        AccountMeta::new(derive_serum_event_queue(&pool_state.serum_market, &pool_state.serum_program_id), false),
        // 11. Serum coin vault (derived from market - placeholder)
        AccountMeta::new(derive_serum_coin_vault(&pool_state.serum_market, &pool_state.serum_program_id), false),
        // 12. Serum pc vault (derived from market - placeholder)
        AccountMeta::new(derive_serum_pc_vault(&pool_state.serum_market, &pool_state.serum_program_id), false),
        // 13. Serum vault signer (derived from market - placeholder)
        AccountMeta::new_readonly(derive_serum_vault_signer(&pool_state.serum_market, &pool_state.serum_program_id), false),
        // 14. User source token account
        AccountMeta::new(user_source_token, false),
        // 15. User destination token account
        AccountMeta::new(user_destination_token, false),
        // 16. User owner (signer)
        AccountMeta::new_readonly(*user_pubkey, true),
        // 17. Serum DEX program
        AccountMeta::new_readonly(pool_state.serum_program_id, false),
    ];

    Ok(Instruction {
        program_id: *amm_program_id,
        accounts,
        data: instruction_data,
    })
}

// Serum market account derivation helpers
// These are placeholder implementations - in production, you would need to fetch
// the actual Serum market state and extract these accounts

fn derive_serum_bids(market: &Pubkey, serum_program: &Pubkey) -> Pubkey {
    // In a real implementation, this would:
    // 1. Fetch the Serum market account data
    // 2. Parse the market state
    // 3. Extract the bids account
    // For now, we use a deterministic derivation as placeholder
    let (pda, _) = Pubkey::find_program_address(
        &[b"bids", &market.to_bytes()],
        serum_program,
    );
    pda
}

fn derive_serum_asks(market: &Pubkey, serum_program: &Pubkey) -> Pubkey {
    // Similar to bids
    let (pda, _) = Pubkey::find_program_address(
        &[b"asks", &market.to_bytes()],
        serum_program,
    );
    pda
}

fn derive_serum_event_queue(market: &Pubkey, serum_program: &Pubkey) -> Pubkey {
    // Similar to bids/asks
    let (pda, _) = Pubkey::find_program_address(
        &[b"event_queue", &market.to_bytes()],
        serum_program,
    );
    pda
}

fn derive_serum_coin_vault(market: &Pubkey, serum_program: &Pubkey) -> Pubkey {
    // Market's coin vault
    let (pda, _) = Pubkey::find_program_address(
        &[b"coin_vault", &market.to_bytes()],
        serum_program,
    );
    pda
}

fn derive_serum_pc_vault(market: &Pubkey, serum_program: &Pubkey) -> Pubkey {
    // Market's pc (price currency) vault
    let (pda, _) = Pubkey::find_program_address(
        &[b"pc_vault", &market.to_bytes()],
        serum_program,
    );
    pda
}

fn derive_serum_vault_signer(market: &Pubkey, serum_program: &Pubkey) -> Pubkey {
    // Vault signer PDA
    let (pda, _) = Pubkey::find_program_address(
        &[&market.to_bytes()],
        serum_program,
    );
    pda
}

/// Structure to hold Serum market accounts
#[derive(Debug, Clone)]
pub struct SerumMarketAccounts {
    pub market: Pubkey,
    pub bids: Pubkey,
    pub asks: Pubkey,
    pub event_queue: Pubkey,
    pub coin_vault: Pubkey,
    pub pc_vault: Pubkey,
    pub vault_signer: Pubkey,
}

impl SerumMarketAccounts {
    /// Create from market pubkey (placeholder implementation)
    pub fn from_market(market: &Pubkey, serum_program: &Pubkey) -> Self {
        Self {
            market: *market,
            bids: derive_serum_bids(market, serum_program),
            asks: derive_serum_asks(market, serum_program),
            event_queue: derive_serum_event_queue(market, serum_program),
            coin_vault: derive_serum_coin_vault(market, serum_program),
            pc_vault: derive_serum_pc_vault(market, serum_program),
            vault_signer: derive_serum_vault_signer(market, serum_program),
        }
    }
}