use crate::core::{SwapError, SwapParams, SwapResult};
use borsh::{BorshDeserialize, BorshSerialize};
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
};
use std::str::FromStr;
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
    pool_account_data: &[u8],
    market_account_data: &[u8],
) -> SwapResult<Instruction> {
    use log::info;
    // Extract pool info
    let pool_address = params.quote.pool_info.address;
    
    // Parse pool state to get all accounts
    let pool_state = crate::core::layouts::AmmInfoLayoutV4::from_bytes(pool_account_data)
        .map_err(|e| SwapError::ParseError(e))?;
    
    // Raydium AMM V4 uses a hardcoded authority for all pools
    // This is the actual authority that owns all pool vaults
    let authority = Pubkey::from_str("5Q544fKrFoe6tsEbD7S8EmxGTJYAKtTVhAW5Q5pge4j1")
        .map_err(|_| SwapError::ParseError("Invalid authority pubkey".to_string()))?;

    // Determine swap direction based on token_in
    let (source_mint, destination_mint) = if params.token_in == params.quote.pool_info.token_a.mint {
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
    
    info!("Swap direction: {} -> {}", source_mint, destination_mint);
    info!("Pool mints - coin: {}, pc: {}", pool_state.coin_mint_address, pool_state.pc_mint_address);

    // Get user token accounts
    let user_source_token = get_associated_token_address(user_pubkey, source_mint);
    let user_destination_token = get_associated_token_address(user_pubkey, destination_mint);
    
    info!("User source token account: {}", user_source_token);
    info!("User destination token account: {}", user_destination_token);

    // Create instruction data
    let data = AmmSwapInstructionData::new(
        params.quote.amount_in,
        params.quote.min_amount_out,
    );

    // Serialize instruction data
    let mut instruction_data = Vec::new();
    data.serialize(&mut instruction_data)?;

    // Check if serum market is a placeholder
    let serum_market_str = pool_state.serum_market.to_string();
    let is_placeholder_market = serum_market_str.starts_with("11111111");
    
    let (serum_coin_vault, serum_pc_vault, serum_event_queue, serum_bids, serum_asks, serum_vault_signer) = 
        if is_placeholder_market {
            info!("Pool has placeholder serum market, using dummy accounts");
            // For pools without real serum market, use dummy accounts
            let dummy = Pubkey::default();
            (dummy, dummy, dummy, dummy, dummy, dummy)
        } else if market_account_data.len() >= 245 {
            // Parse serum market - basic parsing for essential fields
            let serum_coin_vault = Pubkey::from(<[u8; 32]>::try_from(&market_account_data[53..85]).unwrap());
            let serum_pc_vault = Pubkey::from(<[u8; 32]>::try_from(&market_account_data[85..117]).unwrap());
            let serum_event_queue = Pubkey::from(<[u8; 32]>::try_from(&market_account_data[149..181]).unwrap());
            let serum_bids = Pubkey::from(<[u8; 32]>::try_from(&market_account_data[181..213]).unwrap());
            let serum_asks = Pubkey::from(<[u8; 32]>::try_from(&market_account_data[213..245]).unwrap());
            
            // Get serum vault signer
            let vault_signer_nonce = u64::from_le_bytes(market_account_data[45..53].try_into().unwrap());
            let (serum_vault_signer, _) = Pubkey::find_program_address(
                &[
                    pool_state.serum_market.as_ref(),
                    &vault_signer_nonce.to_le_bytes(),
                ],
                &pool_state.serum_program_id,
            );
            
            (serum_coin_vault, serum_pc_vault, serum_event_queue, serum_bids, serum_asks, serum_vault_signer)
        } else {
            return Err(SwapError::ParseError("Invalid serum market data".to_string()));
        };

    info!("Building AMM swap with {} accounts", 18);
    info!("Authority: {}", authority);
    info!("Pool vaults - coin: {}, pc: {}", pool_state.pool_coin_token_account, pool_state.pool_pc_token_account);
    
    // Build accounts list in the order required by Raydium AMM V4
    // CRITICAL: The exact order matters! Serum program must be at position 7
    let accounts = vec![
        // 0. Token program
        AccountMeta::new_readonly(TOKEN_PROGRAM_ID, false),
        // 1. AMM pool account (writable)
        AccountMeta::new(pool_address, false),
        // 2. AMM authority (read-only)
        AccountMeta::new_readonly(authority, false),
        // 3. AMM open orders account (writable)
        AccountMeta::new(pool_state.amm_open_orders, false),
        // 4. AMM target orders account (writable)
        AccountMeta::new(pool_state.amm_target_orders, false),
        // 5. Pool coin vault (writable)
        AccountMeta::new(pool_state.pool_coin_token_account, false),
        // 6. Pool pc vault (writable)
        AccountMeta::new(pool_state.pool_pc_token_account, false),
        // 7. Serum DEX program (read-only) - MUST BE HERE!
        AccountMeta::new_readonly(pool_state.serum_program_id, false),
        // 8. Serum market (writable)
        AccountMeta::new(pool_state.serum_market, false),
        // 9. Serum bids (writable)
        AccountMeta::new(serum_bids, false),
        // 10. Serum asks (writable)
        AccountMeta::new(serum_asks, false),
        // 11. Serum event queue (writable)
        AccountMeta::new(serum_event_queue, false),
        // 12. Serum coin vault (writable)
        AccountMeta::new(serum_coin_vault, false),
        // 13. Serum pc vault (writable)
        AccountMeta::new(serum_pc_vault, false),
        // 14. Serum vault signer (read-only)
        AccountMeta::new_readonly(serum_vault_signer, false),
        // 15. User source token account (writable)
        AccountMeta::new(user_source_token, false),
        // 16. User destination token account (writable)
        AccountMeta::new(user_destination_token, false),
        // 17. User owner (signer)
        AccountMeta::new_readonly(*user_pubkey, true),
    ];

    Ok(Instruction {
        program_id: *amm_program_id,
        accounts,
        data: instruction_data,
    })
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
    let (_is_coin_to_pc, source_mint, destination_mint) = 
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
// TODO: need to fetch the actual Serum market state and extract these accounts

fn derive_serum_bids(market: &Pubkey, serum_program: &Pubkey) -> Pubkey {
    // TODO
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