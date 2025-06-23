use borsh::{BorshDeserialize, BorshSerialize};
use solana_sdk::pubkey::Pubkey;

/// Raydium AMM V4 Pool State Layout
#[derive(Debug, Clone)]
pub struct AmmInfoLayoutV4 {
    pub status: u64,
    pub nonce: u64,
    pub order_num: u64,
    pub depth: u64,
    pub coin_decimals: u64,
    pub pc_decimals: u64,
    pub state: u64,
    pub reset_flag: u64,
    pub min_size: u64,
    pub vol_max_cut_ratio: u64,
    pub amount_wave: u64,
    pub coin_lot_size: u64,
    pub pc_lot_size: u64,
    pub min_price_multiplier: u64,
    pub max_price_multiplier: u64,
    pub sys_decimal_value: u64,
    // Fees
    pub min_separate_numerator: u64,
    pub min_separate_denominator: u64,
    pub trade_fee_numerator: u64,
    pub trade_fee_denominator: u64,
    pub pnl_numerator: u64,
    pub pnl_denominator: u64,
    pub swap_fee_numerator: u64,
    pub swap_fee_denominator: u64,
    // System accounts
    pub need_take_pnl_coin: u64,
    pub need_take_pnl_pc: u64,
    pub total_pnl_pc: u64,
    pub total_pnl_coin: u64,
    pub pool_total_deposit_pc: u128,
    pub pool_total_deposit_coin: u128,
    pub system_decimals_value: u64,
    // Accounts
    pub pool_coin_token_account: Pubkey,
    pub pool_pc_token_account: Pubkey,
    pub coin_mint_address: Pubkey,
    pub pc_mint_address: Pubkey,
    pub lp_mint_address: Pubkey,
    pub amm_open_orders: Pubkey,
    pub serum_market: Pubkey,
    pub serum_program_id: Pubkey,
    pub amm_target_orders: Pubkey,
    pub pool_withdraw_queue: Pubkey,
    pub pool_temp_lp_token_account: Pubkey,
    pub amm_owner: Pubkey,
    pub pnl_owner: Pubkey,
}

impl AmmInfoLayoutV4 {
    pub const LEN: usize = 752;
    
    /// Parse from raw bytes (packed layout)
    pub fn from_bytes(data: &[u8]) -> Result<Self, String> {
        if data.len() != Self::LEN {
            return Err(format!("Invalid data length: {} (expected {})", data.len(), Self::LEN));
        }
        
        // Helper to read u64
        let read_u64 = |offset: usize| -> u64 {
            u64::from_le_bytes(data[offset..offset + 8].try_into().unwrap())
        };
        
        // Helper to read u128
        let read_u128 = |offset: usize| -> u128 {
            u128::from_le_bytes(data[offset..offset + 16].try_into().unwrap())
        };
        
        // Helper to read Pubkey
        let read_pubkey = |offset: usize| -> Pubkey {
            Pubkey::new_from_array(data[offset..offset + 32].try_into().unwrap())
        };
        
        // Parse all fields at their specific offsets
        Ok(Self {
            // u64 fields (0-239)
            status: read_u64(0),
            nonce: read_u64(8),
            order_num: read_u64(16),
            depth: read_u64(24),
            coin_decimals: read_u64(32),
            pc_decimals: read_u64(40),
            state: read_u64(48),
            reset_flag: read_u64(56),
            min_size: read_u64(64),
            vol_max_cut_ratio: read_u64(72),
            amount_wave: read_u64(80),
            coin_lot_size: read_u64(88),
            pc_lot_size: read_u64(96),
            min_price_multiplier: read_u64(104),
            max_price_multiplier: read_u64(112),
            sys_decimal_value: read_u64(120),
            min_separate_numerator: read_u64(128),
            min_separate_denominator: read_u64(136),
            trade_fee_numerator: read_u64(144),
            trade_fee_denominator: read_u64(152),
            pnl_numerator: read_u64(160),
            pnl_denominator: read_u64(168),
            swap_fee_numerator: read_u64(176),
            swap_fee_denominator: read_u64(184),
            need_take_pnl_coin: read_u64(192),
            need_take_pnl_pc: read_u64(200),
            total_pnl_pc: read_u64(208),
            total_pnl_coin: read_u64(216),
            
            // u128 fields (224-255)
            pool_total_deposit_pc: read_u128(224),
            pool_total_deposit_coin: read_u128(240),
            
            // u64 field (256-263)
            system_decimals_value: read_u64(256),
            
            // Pubkey fields - CORRECTED offsets based on actual pool data
            // IMPORTANT: Some pools have swapped vault positions!
            // Standard layout has coin at 336 and pc at 368
            pool_coin_token_account: read_pubkey(336), // Coin vault at offset 336
            pool_pc_token_account: read_pubkey(368),   // PC vault at offset 368
            
            // Mint addresses are at 400 and 432
            coin_mint_address: read_pubkey(400),
            pc_mint_address: read_pubkey(432),
            
            // Other account addresses
            lp_mint_address: read_pubkey(256),
            amm_open_orders: read_pubkey(288),
            serum_market: read_pubkey(320),
            serum_program_id: read_pubkey(352),
            amm_target_orders: read_pubkey(384),
            pool_withdraw_queue: read_pubkey(416),
            pool_temp_lp_token_account: read_pubkey(448),
            amm_owner: read_pubkey(480),
            pnl_owner: read_pubkey(512)
        })
    }

    /// Check if pool is enabled for trading
    pub fn is_enabled(&self) -> bool {
        // Status 6 = Normal pool ready for trading
        // Status 1 = Pool being initialized
        // Status 7 = Pool disabled
        self.status == 6
    }

    /// Get swap fee rate
    pub fn get_swap_fee_rate(&self) -> f64 {
        if self.swap_fee_denominator == 0 {
            return 0.0;
        }
        self.swap_fee_numerator as f64 / self.swap_fee_denominator as f64
    }
}

/// Raydium CLMM Pool State Layout
#[derive(Debug, Clone, BorshSerialize, BorshDeserialize)]
pub struct ClmmPoolState {
    pub bump: [u8; 1],
    pub token_mint_0: Pubkey,
    pub token_mint_1: Pubkey,
    pub tick_spacing: u16,
    pub liquidity: u128,
    pub current_price_sqrt: u128,
    pub current_tick: i32,
    pub fee_growth_global_0: u128,
    pub fee_growth_global_1: u128,
    pub fee_rate: u64,
    pub protocol_fee_rate: u64,
    pub protocol_fee_owed_0: u64,
    pub protocol_fee_owed_1: u64,
    pub fund_fee_owed_0: u64,
    pub fund_fee_owed_1: u64,
    pub padding: [u64; 32],
}

impl ClmmPoolState {
    pub const LEN: usize = 1544;

    /// Get fee rate in basis points
    pub fn get_fee_rate_bps(&self) -> u32 {
        (self.fee_rate * 10000 / 1_000_000) as u32
    }
}

/// Raydium Stable Pool State Layout (simplified)
#[derive(Debug, Clone, BorshSerialize, BorshDeserialize)]
pub struct StablePoolState {
    pub is_initialized: bool,
    pub is_paused: bool,
    pub nonce: u8,
    pub initial_amp_factor: u64,
    pub target_amp_factor: u64,
    pub start_ramp_timestamp: i64,
    pub stop_ramp_timestamp: i64,
    pub future_admin_deadline: i64,
    pub future_admin_account: Pubkey,
    pub admin_account: Pubkey,
    pub token_mint_a: Pubkey,
    pub token_mint_b: Pubkey,
    pub token_a_account: Pubkey,
    pub token_b_account: Pubkey,
    pub pool_mint: Pubkey,
    pub token_a_fees: u64,
    pub token_b_fees: u64,
    pub admin_trade_fee_numerator: u64,
    pub admin_trade_fee_denominator: u64,
    pub trade_fee_numerator: u64,
    pub trade_fee_denominator: u64,
}

impl StablePoolState {
    /// Get current amplification factor
    pub fn get_current_amp(&self, current_timestamp: i64) -> u64 {
        if current_timestamp >= self.stop_ramp_timestamp {
            self.target_amp_factor
        } else if current_timestamp <= self.start_ramp_timestamp {
            self.initial_amp_factor
        } else {
            // Linear interpolation during ramping
            let time_range = self.stop_ramp_timestamp - self.start_ramp_timestamp;
            let time_elapsed = current_timestamp - self.start_ramp_timestamp;
            let amp_range = if self.target_amp_factor > self.initial_amp_factor {
                self.target_amp_factor - self.initial_amp_factor
            } else {
                self.initial_amp_factor - self.target_amp_factor
            };
            
            if self.target_amp_factor > self.initial_amp_factor {
                self.initial_amp_factor + (amp_range * time_elapsed as u64 / time_range as u64)
            } else {
                self.initial_amp_factor - (amp_range * time_elapsed as u64 / time_range as u64)
            }
        }
    }

    /// Get trade fee rate
    pub fn get_trade_fee_rate(&self) -> f64 {
        if self.trade_fee_denominator == 0 {
            return 0.0;
        }
        self.trade_fee_numerator as f64 / self.trade_fee_denominator as f64
    }
}

/// CP-Swap (Constant Product Swap) Pool State for Token-2022
#[derive(Debug, Clone, BorshSerialize, BorshDeserialize)]
pub struct CpSwapPoolState {
    pub discriminator: [u8; 8],
    pub amm_config: Pubkey,
    pub pool_creator: Pubkey,
    pub token_0_vault: Pubkey,
    pub token_1_vault: Pubkey,
    pub lp_mint: Pubkey,
    pub token_0_mint: Pubkey,
    pub token_1_mint: Pubkey,
    pub token_0_program: Pubkey,
    pub token_1_program: Pubkey,
    pub observation_key: Pubkey,
    pub auth_bump: u8,
    pub status: u8,
    pub lp_mint_decimals: u8,
    pub mint_0_decimals: u8,
    pub mint_1_decimals: u8,
    pub lp_supply: u64,
    pub protocol_fees_token_0: u64,
    pub protocol_fees_token_1: u64,
    pub fund_fees_token_0: u64,
    pub fund_fees_token_1: u64,
    pub open_time: u64,
    pub padding: [u64; 32],
}

impl CpSwapPoolState {
    pub const LEN: usize = 653;
    
    /// Check if pool is active
    pub fn is_active(&self) -> bool {
        self.status == 1
    }
    
    /// Parse from raw bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self, String> {
        use borsh::BorshDeserialize;
        
        if data.len() != Self::LEN {
            return Err(format!("Invalid CP pool data length: {} (expected {})", data.len(), Self::LEN));
        }
        
        Self::try_from_slice(data)
            .map_err(|e| format!("Failed to deserialize CP pool state: {}", e))
    }
}