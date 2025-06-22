use crate::core::{
    PoolInfo, PoolState, QuoteRequest, QuoteResult, SwapError,
    SwapResult,
};
use log::debug;
use rust_decimal::prelude::*;
use rust_decimal::Decimal;

/// Stable pool quote calculator
/// Uses StableSwap invariant for minimal slippage on correlated assets
pub struct StableQuoteCalculator;

impl StableQuoteCalculator {
    pub fn new() -> Self {
        Self
    }

    /// Calculate D invariant for StableSwap
    /// D^(n+1) + D = An^n * sum(x_i) + n^n * prod(x_i)
    fn calculate_d(&self, reserves: &[u64], amp_factor: u64) -> SwapResult<Decimal> {
        let n = reserves.len() as u64;
        let sum_reserves: u64 = reserves.iter().sum();
        
        if sum_reserves == 0 {
            return Ok(Decimal::ZERO);
        }

        let ann = Decimal::from(amp_factor * n.pow(n as u32));
        let sum_dec = Decimal::from(sum_reserves);
        
        // Initial guess for D
        let mut d = sum_dec;
        let mut d_prev: Decimal;

        // Newton's method iteration
        for _ in 0..255 {
            let mut d_product = d;
            for &reserve in reserves {
                let reserve_dec = Decimal::from(reserve);
                d_product = d_product * d / (reserve_dec * Decimal::from(n));
            }
            
            d_prev = d;
            let numerator = d * (ann * sum_dec + d_product * Decimal::from(n));
            let denominator = d * (ann - Decimal::ONE) + d_product * Decimal::from(n + 1);
            
            if denominator.is_zero() {
                return Err(SwapError::MathOverflow);
            }
            
            d = numerator / denominator;
            
            // Check convergence
            let diff = if d > d_prev { d - d_prev } else { d_prev - d };
            if diff < Decimal::from_str("0.0001").unwrap() {
                break;
            }
        }
        
        Ok(d)
    }

    /// Calculate output amount for StableSwap
    fn calculate_stable_output(
        &self,
        amount_in: u64,
        reserve_in: u64,
        reserve_out: u64,
        reserves: &[u64],
        amp_factor: u64,
        fee_rate: f64,
    ) -> SwapResult<u64> {
        if reserves.len() != 2 {
            return Err(SwapError::InvalidPoolState(
                "Stable pool must have exactly 2 tokens".to_string(),
            ));
        }

        // Apply fee to input
        let amount_in_with_fee = (amount_in as f64 * (1.0 - fee_rate)) as u64;
        
        // Calculate D before swap
        let d = self.calculate_d(reserves, amp_factor)?;
        
        // Calculate new reserve_in after swap
        let new_reserve_in = reserve_in + amount_in_with_fee;
        
        // Calculate new reserve_out to maintain invariant
        let _n = Decimal::from(2u64);
        let ann = Decimal::from(amp_factor * 4); // A * n^n for n=2
        
        let new_reserve_in_dec = Decimal::from(new_reserve_in);
        let mut new_reserve_out = Decimal::from(reserve_out);
        
        // Newton's method to find new_reserve_out
        for _ in 0..255 {
            let old = new_reserve_out;
            
            let s = new_reserve_in_dec + new_reserve_out;
            let prod = new_reserve_in_dec * new_reserve_out;
            
            let numerator = new_reserve_out * (ann * s + d * d);
            let denominator = new_reserve_out * (ann - Decimal::ONE) + d * d * d / prod;
            
            if denominator.is_zero() {
                return Err(SwapError::MathOverflow);
            }
            
            new_reserve_out = numerator / denominator;
            
            let diff = if new_reserve_out > old {
                new_reserve_out - old
            } else {
                old - new_reserve_out
            };
            
            if diff < Decimal::from_str("0.0001").unwrap() {
                break;
            }
        }
        
        // Calculate output amount
        let new_reserve_out_u64 = new_reserve_out
            .to_u64()
            .ok_or(SwapError::MathOverflow)?;
            
        if new_reserve_out_u64 >= reserve_out {
            return Err(SwapError::InvalidPoolState(
                "Invalid calculation: output reserve increased".to_string(),
            ));
        }
        
        let amount_out = reserve_out - new_reserve_out_u64;
        
        Ok(amount_out)
    }

    /// Calculate price impact for stable pools
    fn calculate_price_impact(
        &self,
        amount_in: u64,
        amount_out: u64,
        reserve_in: u64,
        reserve_out: u64,
    ) -> f64 {
        if amount_in == 0 || amount_out == 0 {
            return 0.0;
        }

        // For stable pools, impact is typically much lower
        let ideal_rate = reserve_out as f64 / reserve_in as f64;
        let actual_rate = amount_out as f64 / amount_in as f64;
        
        let impact = ((ideal_rate - actual_rate) / ideal_rate).abs() * 100.0;
        
        // Stable pools should have minimal impact
        impact.min(1.0) // Cap at 1% for display
    }
}

#[async_trait::async_trait]
impl crate::quotes::QuoteCalculator for StableQuoteCalculator {
    async fn calculate_quote(
        &self,
        pool: &PoolInfo,
        request: &QuoteRequest,
    ) -> SwapResult<QuoteResult> {
        // Extract reserves and amp factor from pool state
        let (reserves, amp_factor) = match &pool.pool_state {
            PoolState::Stable { reserves, amp_factor } => (reserves, *amp_factor),
            _ => {
                return Err(SwapError::InvalidPoolState(
                    "Expected Stable pool state".to_string(),
                ));
            }
        };

        if reserves.len() != 2 {
            return Err(SwapError::InvalidPoolState(
                "Stable pool must have exactly 2 tokens".to_string(),
            ));
        }

        // Determine which token is input/output
        let (reserve_in, reserve_out, _in_index) = if pool.token_a.mint == request.token_in {
            (reserves[0], reserves[1], 0)
        } else if pool.token_b.mint == request.token_in {
            (reserves[1], reserves[0], 1)
        } else {
            return Err(SwapError::InvalidTokenMint(
                "Input token not found in pool".to_string(),
            ));
        };

        debug!(
            "Stable Quote: amount_in={}, reserves={:?}, amp={}, fee={}",
            request.amount_in, reserves, amp_factor, pool.fee_rate
        );

        // Calculate output amount
        let amount_out = self.calculate_stable_output(
            request.amount_in,
            reserve_in,
            reserve_out,
            reserves,
            amp_factor,
            pool.fee_rate,
        )?;

        // Calculate price impact
        let price_impact = self.calculate_price_impact(
            request.amount_in,
            amount_out,
            reserve_in,
            reserve_out,
        );

        // Calculate minimum output with slippage
        let slippage_multiplier = 1.0 - (request.slippage_bps as f64 / 10000.0);
        let min_amount_out = (amount_out as f64 * slippage_multiplier) as u64;

        // Calculate fee
        let fee = (request.amount_in as f64 * pool.fee_rate) as u64;

        Ok(QuoteResult {
            pool_info: pool.clone(),
            amount_in: request.amount_in,
            amount_out,
            min_amount_out,
            price_impact,
            fee,
            route: vec![pool.address],
            token_in: request.token_in,
            token_out: request.token_out,
        })
    }
}

impl Default for StableQuoteCalculator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{PoolType, TokenInfo, STABLE_FEE_RATE};
    use crate::quotes::QuoteCalculator;
    use solana_sdk::pubkey::Pubkey;

    fn create_test_stable_pool(reserves: Vec<u64>, amp_factor: u64) -> PoolInfo {
        let token_a = TokenInfo {
            mint: Pubkey::new_unique(),
            symbol: "USDC".to_string(),
            decimals: 6,
            name: "USD Coin".to_string(),
        };

        let token_b = TokenInfo {
            mint: Pubkey::new_unique(),
            symbol: "USDT".to_string(),
            decimals: 6,
            name: "Tether".to_string(),
        };

        PoolInfo {
            pool_type: PoolType::Stable,
            address: Pubkey::new_unique(),
            token_a,
            token_b,
            liquidity_usd: 1000000.0,
            volume_24h_usd: 500000.0,
            fee_rate: STABLE_FEE_RATE,
            program_id: Pubkey::new_unique(),
            pool_state: PoolState::Stable {
                reserves,
                amp_factor,
            },
        }
    }

    #[test]
    fn test_calculate_d_invariant() {
        let calculator = StableQuoteCalculator::new();
        
        // Test with equal reserves
        let d = calculator.calculate_d(&[1000000, 1000000], 100).unwrap();
        assert!(d > Decimal::from(2000000));
        
        // Test with imbalanced reserves
        let d = calculator.calculate_d(&[1500000, 500000], 100).unwrap();
        assert!(d > Decimal::from(1000000));
    }

    #[test]
    fn test_stable_swap_minimal_slippage() {
        let calculator = StableQuoteCalculator::new();
        
        // For stable pairs with high amp factor, slippage should be minimal
        let amount_out = calculator
            .calculate_stable_output(
                1000,
                1_000_000,
                1_000_000,
                &[1_000_000, 1_000_000],
                1000, // High amp factor
                0.0004,
            )
            .unwrap();
        
        // Output should be very close to input for stable pairs
        assert!(amount_out >= 999);
        assert!(amount_out <= 1000);
    }

    #[tokio::test]
    async fn test_calculate_stable_quote() {
        let calculator = StableQuoteCalculator::new();
        let pool = create_test_stable_pool(vec![1_000_000, 1_000_000], 1000);
        
        let request = QuoteRequest {
            token_in: pool.token_a.mint,
            token_out: pool.token_b.mint,
            amount_in: 10000,
            slippage_bps: 10, // 0.1% slippage for stable
        };

        let quote = calculator.calculate_quote(&pool, &request).await.unwrap();
        
        assert_eq!(quote.amount_in, 10000);
        assert!(quote.amount_out >= 9990); // Very minimal slippage
        assert!(quote.price_impact < 0.1); // Less than 0.1% impact
        assert_eq!(quote.fee, 4); // 0.04% of 10000
    }

    #[test]
    fn test_amp_factor_effect() {
        let calculator = StableQuoteCalculator::new();
        
        // Low amp factor = more slippage
        let amount_out_low_amp = calculator
            .calculate_stable_output(
                10000,
                1_000_000,
                1_000_000,
                &[1_000_000, 1_000_000],
                10, // Low amp
                0.0004,
            )
            .unwrap();
        
        // High amp factor = less slippage
        let amount_out_high_amp = calculator
            .calculate_stable_output(
                10000,
                1_000_000,
                1_000_000,
                &[1_000_000, 1_000_000],
                1000, // High amp
                0.0004,
            )
            .unwrap();
        
        // High amp should give better rate
        assert!(amount_out_high_amp > amount_out_low_amp);
    }
}