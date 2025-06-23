use crate::core::{SwapError, SwapParams, SwapResult};
use borsh::{BorshDeserialize, BorshSerialize};
use log::{debug, info, warn};
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
    // Parse Serum market accounts using proper parser
    let (serum_bids, serum_asks, serum_event_queue, serum_coin_vault, serum_pc_vault, serum_vault_signer) = 
        if crate::core::is_placeholder_market(&pool_state.serum_market) {
            info!("Pool has placeholder serum market, using dummy accounts");
            let dummy = Pubkey::default();
            (dummy, dummy, dummy, dummy, dummy, dummy)
        } else {
            parse_serum_market_accounts(
                market_account_data,
                &pool_state.serum_market,
                &pool_state.serum_program_id
            )?
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
        // 8-13. Serum market accounts (placeholders for pools without real Serum market)
        AccountMeta::new(Pubkey::default(), false), // bids
        AccountMeta::new(Pubkey::default(), false), // asks
        AccountMeta::new(Pubkey::default(), false), // event queue
        AccountMeta::new(Pubkey::default(), false), // coin vault
        AccountMeta::new(Pubkey::default(), false), // pc vault
        AccountMeta::new_readonly(Pubkey::default(), false), // vault signer
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

// Serum market account helpers
fn parse_serum_market_accounts(
    market_data: &[u8], 
    market_address: &Pubkey,
    dex_program: &Pubkey
) -> SwapResult<(Pubkey, Pubkey, Pubkey, Pubkey, Pubkey, Pubkey)> {
    use crate::core::MarketState;
    
    // If market data is too small, it's likely a placeholder
    if market_data.len() < 400 {
        debug!("Market data too small, using placeholder accounts");
        // Return dummy accounts for placeholder market
        let dummy = Pubkey::default();
        return Ok((dummy, dummy, dummy, dummy, dummy, dummy));
    }
    
    // Parse market state
    match MarketState::parse(market_data) {
        Ok(market) => {
            // Get accounts from market state
            let bids = market.bids();
            let asks = market.asks();
            let event_queue = market.event_queue();
            let coin_vault = market.base_vault();
            let pc_vault = market.quote_vault();
            let vault_signer = market.vault_signer(market_address, dex_program)?;
            
            info!("Parsed Serum market accounts - bids: {}, asks: {}", bids, asks);
            Ok((bids, asks, event_queue, coin_vault, pc_vault, vault_signer))
        }
        Err(e) => {
            warn!("Failed to parse Serum market: {}, using placeholder accounts", e);
            // Return dummy accounts if parsing fails
            let dummy = Pubkey::default();
            Ok((dummy, dummy, dummy, dummy, dummy, dummy))
        }
    }
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
    /// Create placeholder accounts for markets without real Serum integration
    pub fn placeholder(market: &Pubkey) -> Self {
        Self {
            market: *market,
            bids: Pubkey::default(),
            asks: Pubkey::default(),
            event_queue: Pubkey::default(),
            coin_vault: Pubkey::default(),
            pc_vault: Pubkey::default(),
            vault_signer: Pubkey::default(),
        }
    }
}