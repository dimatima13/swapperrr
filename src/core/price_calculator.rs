use crate::core::{PoolInfo, PoolState};
use log::debug;
use solana_sdk::pubkey::Pubkey;
use std::collections::HashMap;
use std::str::FromStr;

// Known stable coin mints
lazy_static::lazy_static! {
    static ref STABLE_COINS: HashMap<Pubkey, f64> = {
        let mut m = HashMap::new();
        // USDC
        m.insert(
            Pubkey::from_str("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v").unwrap(),
            1.0
        );
        // USDT
        m.insert(
            Pubkey::from_str("Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB").unwrap(),
            1.0
        );
        // USDH
        m.insert(
            Pubkey::from_str("USDH1SM1ojwWUga67PGrgFWUHibbjqMvuMaDkRJTgkX").unwrap(),
            1.0
        );
        // UXD
        m.insert(
            Pubkey::from_str("7kbnvuGBxxj8AG9qp8Scn56muWGaRaFqxg1FsRp3PaFT").unwrap(),
            1.0
        );
        m
    };
}

/// Price calculator that derives prices from pool reserves
pub struct OnchainPriceCalculator;

impl OnchainPriceCalculator {
    /// Calculate token price in USD based on pool reserves
    /// Returns None if price cannot be determined
    pub fn calculate_token_price(
        token_mint: &Pubkey,
        pools: &[PoolInfo],
    ) -> Option<f64> {
        // If it's a known stablecoin, return its pegged price
        if let Some(&price) = STABLE_COINS.get(token_mint) {
            return Some(price);
        }
        
        // Find pools that contain this token paired with a stablecoin
        let stable_pools: Vec<_> = pools.iter()
            .filter(|pool| {
                (pool.token_a.mint == *token_mint && STABLE_COINS.contains_key(&pool.token_b.mint)) ||
                (pool.token_b.mint == *token_mint && STABLE_COINS.contains_key(&pool.token_a.mint))
            })
            .collect();
        
        if stable_pools.is_empty() {
            // Try to find price through intermediate tokens (e.g., token -> SOL -> USDC)
            return Self::calculate_indirect_price(token_mint, pools);
        }
        
        // Calculate weighted average price from stable pools
        let mut total_liquidity = 0.0;
        let mut weighted_price = 0.0;
        
        for pool in stable_pools {
            if let Some(price) = Self::calculate_price_from_pool(token_mint, pool) {
                let liquidity = pool.liquidity_usd.max(1.0); // Avoid zero weight
                weighted_price += price * liquidity;
                total_liquidity += liquidity;
                
                debug!(
                    "Pool {} price: ${:.6} (liquidity: ${:.2})",
                    pool.address, price, liquidity
                );
            }
        }
        
        if total_liquidity > 0.0 {
            Some(weighted_price / total_liquidity)
        } else {
            None
        }
    }
    
    /// Calculate price from a single pool
    fn calculate_price_from_pool(token_mint: &Pubkey, pool: &PoolInfo) -> Option<f64> {
        match &pool.pool_state {
            PoolState::AMM { reserve_a, reserve_b, .. } |
            PoolState::Standard { reserve_a, reserve_b } => {
                Self::calculate_price_from_reserves(
                    token_mint,
                    &pool.token_a,
                    &pool.token_b,
                    *reserve_a,
                    *reserve_b,
                )
            }
            PoolState::Stable { reserves, .. } => {
                if reserves.len() >= 2 {
                    Self::calculate_price_from_reserves(
                        token_mint,
                        &pool.token_a,
                        &pool.token_b,
                        reserves[0],
                        reserves[1],
                    )
                } else {
                    None
                }
            }
            PoolState::CLMM { .. } => {
                // CLMM pools have dynamic prices based on tick
                // For now, use the pool's reported liquidity
                None // TODO: Implement CLMM price calculation
            }
        }
    }
    
    /// Calculate price from reserves
    fn calculate_price_from_reserves(
        token_mint: &Pubkey,
        token_a: &crate::core::TokenInfo,
        token_b: &crate::core::TokenInfo,
        reserve_a: u64,
        reserve_b: u64,
    ) -> Option<f64> {
        if reserve_a == 0 || reserve_b == 0 {
            return None;
        }
        
        let amount_a = reserve_a as f64 / 10f64.powi(token_a.decimals as i32);
        let amount_b = reserve_b as f64 / 10f64.powi(token_b.decimals as i32);
        
        if token_a.mint == *token_mint {
            // token_a is our target token
            if let Some(&stable_price) = STABLE_COINS.get(&token_b.mint) {
                // token_b is a stablecoin
                Some((amount_b / amount_a) * stable_price)
            } else {
                None
            }
        } else if token_b.mint == *token_mint {
            // token_b is our target token
            if let Some(&stable_price) = STABLE_COINS.get(&token_a.mint) {
                // token_a is a stablecoin
                Some((amount_a / amount_b) * stable_price)
            } else {
                None
            }
        } else {
            None
        }
    }
    
    /// Calculate price through intermediate tokens
    fn calculate_indirect_price(token_mint: &Pubkey, pools: &[PoolInfo]) -> Option<f64> {
        // Common intermediate tokens
        let sol_mint = Pubkey::from_str("So11111111111111111111111111111111111111112").ok()?;
        
        // Try token -> SOL -> USDC path
        let sol_price = Self::calculate_token_price(&sol_mint, pools)?;
        
        // Find pools with our token and SOL
        let token_sol_pools: Vec<_> = pools.iter()
            .filter(|pool| {
                (pool.token_a.mint == *token_mint && pool.token_b.mint == sol_mint) ||
                (pool.token_b.mint == *token_mint && pool.token_a.mint == sol_mint)
            })
            .collect();
        
        if !token_sol_pools.is_empty() {
            // Calculate token price in SOL
            let mut total_liquidity = 0.0;
            let mut weighted_price_in_sol = 0.0;
            
            for pool in token_sol_pools {
                if let Some(price_in_sol) = Self::calculate_price_from_pool(token_mint, pool) {
                    let liquidity = pool.liquidity_usd.max(1.0);
                    weighted_price_in_sol += price_in_sol * liquidity;
                    total_liquidity += liquidity;
                }
            }
            
            if total_liquidity > 0.0 {
                let price_in_sol = weighted_price_in_sol / total_liquidity;
                return Some(price_in_sol * sol_price);
            }
        }
        
        None
    }
    
    /// Estimate pool liquidity in USD
    pub fn estimate_pool_liquidity_usd(pool: &PoolInfo, token_prices: &HashMap<Pubkey, f64>) -> f64 {
        let price_a = token_prices.get(&pool.token_a.mint)
            .or_else(|| STABLE_COINS.get(&pool.token_a.mint))
            .copied()
            .unwrap_or(0.0);
            
        let price_b = token_prices.get(&pool.token_b.mint)
            .or_else(|| STABLE_COINS.get(&pool.token_b.mint))
            .copied()
            .unwrap_or(0.0);
        
        match &pool.pool_state {
            PoolState::AMM { reserve_a, reserve_b, .. } |
            PoolState::Standard { reserve_a, reserve_b } => {
                let amount_a = *reserve_a as f64 / 10f64.powi(pool.token_a.decimals as i32);
                let amount_b = *reserve_b as f64 / 10f64.powi(pool.token_b.decimals as i32);
                
                amount_a * price_a + amount_b * price_b
            }
            PoolState::Stable { reserves, .. } => {
                if reserves.len() >= 2 {
                    let amount_a = reserves[0] as f64 / 10f64.powi(pool.token_a.decimals as i32);
                    let amount_b = reserves[1] as f64 / 10f64.powi(pool.token_b.decimals as i32);
                    
                    amount_a * price_a + amount_b * price_b
                } else {
                    0.0
                }
            }
            PoolState::CLMM { liquidity, .. } => {
                // Rough estimate based on concentrated liquidity
                // This is a simplification
                *liquidity as f64 / 1_000_000.0
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::TokenInfo;
    
    #[test]
    fn test_stablecoin_price() {
        let usdc = Pubkey::from_str("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v").unwrap();
        let price = OnchainPriceCalculator::calculate_token_price(&usdc, &[]);
        assert_eq!(price, Some(1.0));
    }
    
    #[test]
    fn test_price_from_reserves() {
        let token_a = TokenInfo {
            mint: Pubkey::new_unique(),
            symbol: "TOKEN".to_string(),
            name: "Test Token".to_string(),
            decimals: 9,
        };
        
        let usdc = TokenInfo {
            mint: Pubkey::from_str("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v").unwrap(),
            symbol: "USDC".to_string(),
            name: "USD Coin".to_string(),
            decimals: 6,
        };
        
        // 1000 TOKEN = 100 USDC, so 1 TOKEN = 0.1 USDC
        let price = OnchainPriceCalculator::calculate_price_from_reserves(
            &token_a.mint,
            &token_a,
            &usdc,
            1000 * 10u64.pow(9), // 1000 TOKEN
            100 * 10u64.pow(6),  // 100 USDC
        );
        
        assert_eq!(price, Some(0.1));
    }
}