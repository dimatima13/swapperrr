use crate::core::{
    PoolInfo, PoolState, QuoteRequest, QuoteResult, SwapError, SwapResult,
};
use log::debug;
use rust_decimal::prelude::*;
use rust_decimal::Decimal;

/// CLMM (Concentrated Liquidity Market Maker) quote calculator
/// Uses tick-based pricing with concentrated liquidity
pub struct ClmmQuoteCalculator;

impl ClmmQuoteCalculator {
    pub fn new() -> Self {
        Self
    }

    /// Calculate price from tick
    /// price = 1.0001^tick
    fn tick_to_price(&self, tick: i32) -> SwapResult<Decimal> {
        let base = Decimal::from_str("1.0001").unwrap();
        let tick_dec = Decimal::from(tick.abs());
        
        // Calculate 1.0001^|tick|
        let mut price = Decimal::ONE;
        let mut temp_base = base;
        let mut temp_tick = tick.abs() as u32;
        
        // Binary exponentiation for efficiency
        while temp_tick > 0 {
            if temp_tick & 1 == 1 {
                price *= temp_base;
            }
            temp_base *= temp_base;
            temp_tick >>= 1;
        }
        
        // If tick is negative, invert the price
        if tick < 0 {
            if price.is_zero() {
                return Err(SwapError::MathOverflow);
            }
            price = Decimal::ONE / price;
        }
        
        Ok(price)
    }

    /// Calculate sqrt price from tick
    fn tick_to_sqrt_price(&self, tick: i32) -> SwapResult<Decimal> {
        let price = self.tick_to_price(tick)?;
        
        // Calculate square root using Newton's method
        let mut sqrt_price = price / Decimal::TWO;
        for _ in 0..10 {
            if sqrt_price.is_zero() {
                return Err(SwapError::MathOverflow);
            }
            sqrt_price = (sqrt_price + price / sqrt_price) / Decimal::TWO;
        }
        
        Ok(sqrt_price)
    }

    /// Calculate output amount for CLMM swap
    fn calculate_clmm_output(
        &self,
        amount_in: u64,
        current_tick: i32,
        tick_spacing: u16,
        liquidity: u128,
        fee_tier: u32,
        is_token_a_to_b: bool,
    ) -> SwapResult<u64> {
        if liquidity == 0 {
            return Err(SwapError::InsufficientLiquidity {
                pool_type: crate::core::PoolType::CLMM,
                available: 0,
                required: amount_in,
            });
        }

        let fee_rate = fee_tier as f64 / 1_000_000.0;
        let amount_in_after_fee = (amount_in as f64 * (1.0 - fee_rate)) as u64;
        
        // Get current sqrt price
        let sqrt_price_current = self.tick_to_sqrt_price(current_tick)?;
        
        // For simplicity, assume swap happens within current tick range
        // In reality, we would need to handle crossing multiple ticks
        let liquidity_dec = Decimal::from(liquidity);
        let amount_in_dec = Decimal::from(amount_in_after_fee);
        
        let amount_out = if is_token_a_to_b {
            // Token A to Token B: price decreases
            // amount_out = liquidity * (1/sqrt_price_new - 1/sqrt_price_current)
            // Simplified: assume small swap doesn't cross ticks
            let delta_sqrt_price = amount_in_dec / liquidity_dec;
            let new_sqrt_price = sqrt_price_current - delta_sqrt_price;
            
            if new_sqrt_price <= Decimal::ZERO {
                return Err(SwapError::InsufficientLiquidity {
                    pool_type: crate::core::PoolType::CLMM,
                    available: liquidity as u64,
                    required: amount_in,
                });
            }
            
            let amount_out_dec = liquidity_dec * (Decimal::ONE / new_sqrt_price - Decimal::ONE / sqrt_price_current);
            amount_out_dec.abs()
        } else {
            // Token B to Token A: price increases
            let delta_sqrt_price = amount_in_dec / liquidity_dec;
            let new_sqrt_price = sqrt_price_current + delta_sqrt_price;
            
            let amount_out_dec = liquidity_dec * (new_sqrt_price - sqrt_price_current);
            amount_out_dec
        };
        
        amount_out
            .to_u64()
            .ok_or(SwapError::MathOverflow)
    }

    /// Calculate price impact for CLMM
    fn calculate_price_impact(
        &self,
        amount_in: u64,
        amount_out: u64,
        current_tick: i32,
    ) -> SwapResult<f64> {
        let current_price = self.tick_to_price(current_tick)?;
        let execution_price = Decimal::from(amount_out) / Decimal::from(amount_in);
        
        let current_price_f64 = current_price
            .to_f64()
            .ok_or(SwapError::MathOverflow)?;
        let execution_price_f64 = execution_price
            .to_f64()
            .ok_or(SwapError::MathOverflow)?;
        
        let impact = ((current_price_f64 - execution_price_f64) / current_price_f64).abs() * 100.0;
        
        Ok(impact)
    }
}

#[async_trait::async_trait]
impl crate::quotes::QuoteCalculator for ClmmQuoteCalculator {
    async fn calculate_quote(
        &self,
        pool: &PoolInfo,
        request: &QuoteRequest,
    ) -> SwapResult<QuoteResult> {
        // Extract CLMM state
        let (current_tick, tick_spacing, liquidity, fee_tier) = match &pool.pool_state {
            PoolState::CLMM {
                current_tick,
                tick_spacing,
                liquidity,
                fee_tier,
            } => (*current_tick, *tick_spacing, *liquidity, *fee_tier),
            _ => {
                return Err(SwapError::InvalidPoolState(
                    "Expected CLMM pool state".to_string(),
                ));
            }
        };

        // Determine swap direction
        let is_token_a_to_b = pool.token_a.mint == request.token_in;
        if !is_token_a_to_b && pool.token_b.mint != request.token_in {
            return Err(SwapError::InvalidTokenMint(
                "Input token not found in pool".to_string(),
            ));
        }

        debug!(
            "CLMM Quote: amount_in={}, tick={}, liquidity={}, fee_tier={}",
            request.amount_in, current_tick, liquidity, fee_tier
        );

        // Calculate output amount
        let amount_out = self.calculate_clmm_output(
            request.amount_in,
            current_tick,
            tick_spacing,
            liquidity,
            fee_tier,
            is_token_a_to_b,
        )?;

        // Calculate price impact
        let price_impact = self.calculate_price_impact(
            request.amount_in,
            amount_out,
            current_tick,
        )?;

        // Calculate minimum output with slippage
        let slippage_multiplier = 1.0 - (request.slippage_bps as f64 / 10000.0);
        let min_amount_out = (amount_out as f64 * slippage_multiplier) as u64;

        // Calculate fee
        let fee_rate = fee_tier as f64 / 1_000_000.0;
        let fee = (request.amount_in as f64 * fee_rate) as u64;

        Ok(QuoteResult {
            pool_info: pool.clone(),
            amount_in: request.amount_in,
            amount_out,
            min_amount_out,
            price_impact,
            fee,
            route: vec![pool.address],
        })
    }
}

impl Default for ClmmQuoteCalculator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{PoolType, TokenInfo};
    use crate::quotes::QuoteCalculator;
    use solana_sdk::pubkey::Pubkey;

    fn create_test_clmm_pool(
        current_tick: i32,
        tick_spacing: u16,
        liquidity: u128,
        fee_tier: u32,
    ) -> PoolInfo {
        let token_a = TokenInfo {
            mint: Pubkey::new_unique(),
            symbol: "SOL".to_string(),
            decimals: 9,
            name: "Solana".to_string(),
        };

        let token_b = TokenInfo {
            mint: Pubkey::new_unique(),
            symbol: "USDC".to_string(),
            decimals: 6,
            name: "USD Coin".to_string(),
        };

        PoolInfo {
            pool_type: PoolType::CLMM,
            address: Pubkey::new_unique(),
            token_a,
            token_b,
            liquidity_usd: 500000.0,
            volume_24h_usd: 250000.0,
            fee_rate: fee_tier as f64 / 1_000_000.0,
            program_id: Pubkey::new_unique(),
            pool_state: PoolState::CLMM {
                current_tick,
                tick_spacing,
                liquidity,
                fee_tier,
            },
        }
    }

    #[test]
    fn test_tick_to_price() {
        let calculator = ClmmQuoteCalculator::new();
        
        // Test tick 0 = price 1
        let price = calculator.tick_to_price(0).unwrap();
        assert_eq!(price, Decimal::ONE);
        
        // Test positive tick
        let price = calculator.tick_to_price(1000).unwrap();
        assert!(price > Decimal::ONE);
        
        // Test negative tick
        let price = calculator.tick_to_price(-1000).unwrap();
        assert!(price < Decimal::ONE);
    }

    #[test]
    fn test_tick_to_sqrt_price() {
        let calculator = ClmmQuoteCalculator::new();
        
        // Test tick 0
        let sqrt_price = calculator.tick_to_sqrt_price(0).unwrap();
        assert!((sqrt_price - Decimal::ONE).abs() < Decimal::from_str("0.001").unwrap());
        
        // Test that sqrt(price) * sqrt(price) ≈ price
        let tick = 1000;
        let price = calculator.tick_to_price(tick).unwrap();
        let sqrt_price = calculator.tick_to_sqrt_price(tick).unwrap();
        let price_check = sqrt_price * sqrt_price;
        
        let diff = (price - price_check).abs() / price;
        assert!(diff < Decimal::from_str("0.01").unwrap()); // Less than 1% error
    }

    #[tokio::test]
    async fn test_calculate_clmm_quote() {
        let calculator = ClmmQuoteCalculator::new();
        let pool = create_test_clmm_pool(
            0,        // current tick
            1,        // tick spacing
            1_000_000_000_000, // liquidity
            500,      // 0.05% fee tier
        );
        
        let request = QuoteRequest {
            token_in: pool.token_a.mint,
            token_out: pool.token_b.mint,
            amount_in: 1000000, // 1M units
            slippage_bps: 50,
        };

        let quote = calculator.calculate_quote(&pool, &request).await.unwrap();
        
        assert_eq!(quote.amount_in, 1000000);
        assert!(quote.amount_out > 0);
        assert!(quote.price_impact >= 0.0);
        assert_eq!(quote.fee, 500); // 0.05% of 1M
    }

    #[test]
    fn test_fee_tiers() {
        let calculator = ClmmQuoteCalculator::new();
        
        // Test different fee tiers
        let fee_tiers = vec![100, 500, 3000, 10000]; // 0.01%, 0.05%, 0.3%, 1%
        
        for fee_tier in fee_tiers {
            let output = calculator
                .calculate_clmm_output(
                    1_000_000,
                    0,
                    1,
                    1_000_000_000_000,
                    fee_tier,
                    true,
                )
                .unwrap();
            
            // Higher fee tier should result in less output
            let expected_fee = 1_000_000 * fee_tier / 1_000_000;
            let amount_after_fee = 1_000_000 - expected_fee;
            
            assert!(output < 1_000_000);
        }
    }

    #[test]
    fn test_insufficient_liquidity() {
        let calculator = ClmmQuoteCalculator::new();
        
        // Test with zero liquidity
        let result = calculator.calculate_clmm_output(
            1_000_000,
            0,
            1,
            0, // No liquidity
            500,
            true,
        );
        
        assert!(matches!(result, Err(SwapError::InsufficientLiquidity { .. })));
    }
}