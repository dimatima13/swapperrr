use crate::core::{SwapError, SwapResult};
use solana_sdk::pubkey::Pubkey;
use std::mem::size_of;

/// Serum/OpenBook market layout
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct MarketState {
    // First 5 bytes are padding
    pub account_flags: u64,      // 8 bytes
    pub own_address: [u64; 4],   // 32 bytes
    pub vault_signer_nonce: u64, // 8 bytes
    pub base_mint: [u64; 4],     // 32 bytes
    pub quote_mint: [u64; 4],    // 32 bytes
    pub base_vault: [u64; 4],    // 32 bytes
    pub base_deposits_total: u64, // 8 bytes
    pub base_fees_accrued: u64,  // 8 bytes
    pub quote_vault: [u64; 4],   // 32 bytes
    pub quote_deposits_total: u64, // 8 bytes
    pub quote_fees_accrued: u64, // 8 bytes
    pub quote_dust_threshold: u64, // 8 bytes
    pub request_queue: [u64; 4], // 32 bytes
    pub event_queue: [u64; 4],   // 32 bytes
    pub bids: [u64; 4],          // 32 bytes
    pub asks: [u64; 4],          // 32 bytes
    pub base_lot_size: u64,      // 8 bytes
    pub quote_lot_size: u64,     // 8 bytes
    pub fee_rate_bps: u64,       // 8 bytes
    pub referrer_rebate_accrued: u64, // 8 bytes
    pub _padding: [u8; 7],       // 7 bytes padding
}

impl MarketState {
    /// Parse market state from raw account data
    pub fn parse(data: &[u8]) -> SwapResult<Self> {
        if data.len() < size_of::<MarketState>() + 5 {
            return Err(SwapError::Other(format!(
                "Market account data too small: {} bytes, expected at least {}",
                data.len(),
                size_of::<MarketState>() + 5
            )));
        }

        // Skip first 5 bytes (padding)
        let market_data = &data[5..5 + size_of::<MarketState>()];
        
        // Safety: We've checked the size above
        let market = unsafe { 
            std::ptr::read_unaligned(market_data.as_ptr() as *const MarketState)
        };

        Ok(market)
    }

    /// Convert [u64; 4] to Pubkey
    fn bytes_to_pubkey(bytes: &[u64; 4]) -> Pubkey {
        let mut pubkey_bytes = [0u8; 32];
        for i in 0..4 {
            let bytes_array = bytes[i].to_le_bytes();
            pubkey_bytes[i * 8..(i + 1) * 8].copy_from_slice(&bytes_array);
        }
        Pubkey::new_from_array(pubkey_bytes)
    }

    /// Get bids account
    pub fn bids(&self) -> Pubkey {
        Self::bytes_to_pubkey(&self.bids)
    }

    /// Get asks account
    pub fn asks(&self) -> Pubkey {
        Self::bytes_to_pubkey(&self.asks)
    }

    /// Get event queue account
    pub fn event_queue(&self) -> Pubkey {
        Self::bytes_to_pubkey(&self.event_queue)
    }

    /// Get base (coin) vault
    pub fn base_vault(&self) -> Pubkey {
        Self::bytes_to_pubkey(&self.base_vault)
    }

    /// Get quote (pc) vault
    pub fn quote_vault(&self) -> Pubkey {
        Self::bytes_to_pubkey(&self.quote_vault)
    }

    /// Get request queue
    pub fn request_queue(&self) -> Pubkey {
        Self::bytes_to_pubkey(&self.request_queue)
    }

    /// Get base mint
    pub fn base_mint(&self) -> Pubkey {
        Self::bytes_to_pubkey(&self.base_mint)
    }

    /// Get quote mint
    pub fn quote_mint(&self) -> Pubkey {
        Self::bytes_to_pubkey(&self.quote_mint)
    }

    /// Get vault signer PDA
    pub fn vault_signer(&self, market_address: &Pubkey, dex_program: &Pubkey) -> SwapResult<Pubkey> {
        let (pda, nonce) = Pubkey::find_program_address(
            &[&market_address.to_bytes()],
            dex_program,
        );
        
        if nonce != self.vault_signer_nonce as u8 {
            return Err(SwapError::Other(format!(
                "Vault signer nonce mismatch: expected {}, got {}",
                self.vault_signer_nonce, nonce
            )));
        }
        
        Ok(pda)
    }
}

/// Check if a market is valid placeholder (all 1s)
pub fn is_placeholder_market(market: &Pubkey) -> bool {
    market.to_string().chars().all(|c| c == '1')
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_placeholder_market() {
        let placeholder = Pubkey::from_str("11111111111111111111111111111111").unwrap();
        assert!(is_placeholder_market(&placeholder));

        let real_market = Pubkey::new_unique();
        assert!(!is_placeholder_market(&real_market));
    }

    #[test]
    fn test_bytes_to_pubkey() {
        let bytes: [u64; 4] = [
            u64::from_le_bytes([1, 2, 3, 4, 5, 6, 7, 8]),
            u64::from_le_bytes([9, 10, 11, 12, 13, 14, 15, 16]),
            u64::from_le_bytes([17, 18, 19, 20, 21, 22, 23, 24]),
            u64::from_le_bytes([25, 26, 27, 28, 29, 30, 31, 32]),
        ];
        
        let pubkey = MarketState::bytes_to_pubkey(&bytes);
        let expected_bytes = [
            1, 2, 3, 4, 5, 6, 7, 8,
            9, 10, 11, 12, 13, 14, 15, 16,
            17, 18, 19, 20, 21, 22, 23, 24,
            25, 26, 27, 28, 29, 30, 31, 32,
        ];
        
        assert_eq!(pubkey.to_bytes(), expected_bytes);
    }
}