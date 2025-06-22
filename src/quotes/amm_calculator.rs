use crate::core::{
    PoolInfo, PoolState, QuoteRequest, QuoteResult, SwapError,
    SwapResult,
};
use log::debug;
use rust_decimal::prelude::*;
use rust_decimal::Decimal;

/// AMM (Automated Market Maker) quote calculator
/// Uses constant product formula: x * y = k
pub struct AmmQuoteCalculator;

impl AmmQuoteCalculator {
    pub fn new() -> Self {
        Self
    }

    /// Calculate output amount using constant product formula
    fn calculate_output_amount(
        &self,
        amount_in: u64,
        reserve_in: u64,
        reserve_out: u64,
        fee_rate: f64,
    ) -> SwapResult<u64> {
        // Validate inputs
        if reserve_in == 0 || reserve_out == 0 {
            return Err(SwapError::InvalidPoolState(
                "Pool has zero reserves".to_string(),
            ));
        }

        if amount_in == 0 {
            return Ok(0);
        }

        // Use Decimal for precise calculation
        let amount_in_dec = Decimal::from(amount_in);
        let reserve_in_dec = Decimal::from(reserve_in);
        let reserve_out_dec = Decimal::from(reserve_out);
        let fee_multiplier = Decimal::from_f64(1.0 - fee_rate)
            .ok_or(SwapError::MathOverflow)?;

        // Apply fee to input amount
        let amount_in_with_fee = amount_in_dec * fee_multiplier;

        // Constant product formula: output = (reserve_out * amount_in_with_fee) / (reserve_in + amount_in_with_fee)
        let numerator = reserve_out_dec * amount_in_with_fee;
        let denominator = reserve_in_dec + amount_in_with_fee;

        if denominator.is_zero() {
            return Err(SwapError::MathOverflow);
        }

        let amount_out = numerator / denominator;

        // Convert back to u64
        amount_out
            .to_u64()
            .ok_or(SwapError::MathOverflow)
    }

    /// Calculate price impact
    fn calculate_price_impact(
        &self,
        amount_in: u64,
        amount_out: u64,
        reserve_in: u64,
        reserve_out: u64,
    ) -> f64 {
        if amount_in == 0 || amount_out == 0 || reserve_in == 0 || reserve_out == 0 {
            return 0.0;
        }

        // Initial price = reserve_out / reserve_in
        let initial_price = reserve_out as f64 / reserve_in as f64;

        // Execution price = amount_out / amount_in
        let execution_price = amount_out as f64 / amount_in as f64;

        // Price impact = 1 - (execution_price / initial_price)
        let price_impact = 1.0 - (execution_price / initial_price);

        // Return as percentage
        price_impact * 100.0
    }

    /// Calculate minimum output with slippage
    fn calculate_min_output(&self, amount_out: u64, slippage_bps: u16) -> u64 {
        let slippage_multiplier = 1.0 - (slippage_bps as f64 / 10000.0);
        let min_amount = amount_out as f64 * slippage_multiplier;
        min_amount as u64
    }
}

#[async_trait::async_trait]
impl crate::quotes::QuoteCalculator for AmmQuoteCalculator {
    async fn calculate_quote(
        &self,
        pool: &PoolInfo,
        request: &QuoteRequest,
    ) -> SwapResult<QuoteResult> {
        // Extract reserves from pool state
        let (reserve_in, reserve_out) = match &pool.pool_state {
            PoolState::AMM { reserve_a, reserve_b, .. } => {
                if pool.token_a.mint == request.token_in {
                    (*reserve_a, *reserve_b)
                } else if pool.token_b.mint == request.token_in {
                    (*reserve_b, *reserve_a)
                } else {
                    return Err(SwapError::InvalidTokenMint(
                        "Input token not found in pool".to_string(),
                    ));
                }
            }
            _ => {
                return Err(SwapError::InvalidPoolState(
                    "Expected AMM pool state".to_string(),
                ));
            }
        };

        debug!(
            "AMM Quote: amount_in={}, reserve_in={}, reserve_out={}, fee={}",
            request.amount_in, reserve_in, reserve_out, pool.fee_rate
        );
        debug!(
            "Token mapping: in={} (looking for {}), out={}",
            if pool.token_a.mint == request.token_in { &pool.token_a.symbol } else { &pool.token_b.symbol },
            request.token_in,
            if pool.token_a.mint == request.token_in { &pool.token_b.symbol } else { &pool.token_a.symbol }
        );

        // Calculate output amount
        let amount_out = self.calculate_output_amount(
            request.amount_in,
            reserve_in,
            reserve_out,
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
        let min_amount_out = self.calculate_min_output(amount_out, request.slippage_bps);

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

impl Default for AmmQuoteCalculator {
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

    fn create_test_pool(reserve_a: u64, reserve_b: u64) -> PoolInfo {
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
            pool_type: PoolType::AMM,
            address: Pubkey::new_unique(),
            token_a,
            token_b,
            liquidity_usd: 100000.0,
            volume_24h_usd: 50000.0,
            // TODO: fix this
            fee_rate: AMM_FEE_RATE,
            program_id: Pubkey::new_unique(),
            pool_state: PoolState::AMM {
                reserve_a,
                reserve_b,
            },
        }
    }

    #[test]
    fn test_calculate_output_amount() {
        let calculator = AmmQuoteCalculator::new();

        // Test basic calculation
        let amount_out = calculator
            .calculate_output_amount(1000, 1_000_000, 1_000_000, 0.0025)
            .unwrap();
        
        // With 0.25% fee and equal reserves, output should be slightly less than input
        assert!(amount_out < 1000);
        assert!(amount_out > 990); // Should be around 997
    }

    #[test]
    fn test_price_impact() {
        let calculator = AmmQuoteCalculator::new();

        // Small trade should have minimal impact
        let impact = calculator.calculate_price_impact(100, 99, 1_000_000, 1_000_000);
        assert!(impact < 0.1); // Less than 0.1%

        // Large trade should have significant impact
        let impact = calculator.calculate_price_impact(100_000, 90_000, 1_000_000, 1_000_000);
        assert!(impact > 5.0); // More than 5%
    }

    #[test]
    fn test_min_output_calculation() {
        let calculator = AmmQuoteCalculator::new();

        // 1% slippage (100 bps)
        let min_output = calculator.calculate_min_output(1000, 100);
        assert_eq!(min_output, 990);

        // 0.5% slippage (50 bps)
        let min_output = calculator.calculate_min_output(1000, 50);
        assert_eq!(min_output, 995);
    }

    #[tokio::test]
    async fn test_calculate_quote() {
        let calculator = AmmQuoteCalculator::new();
        let pool = create_test_pool(1_000_000, 1_000_000);
        
        let request = QuoteRequest {
            token_in: pool.token_a.mint,
            token_out: pool.token_b.mint,
            amount_in: 1000,
            slippage_bps: 50,
        };

        let quote = calculator.calculate_quote(&pool, &request).await.unwrap();
        
        assert_eq!(quote.amount_in, 1000);
        assert!(quote.amount_out > 0);
        assert!(quote.amount_out < 1000); // Should be less due to fees
        assert_eq!(quote.min_amount_out, quote.amount_out * 995 / 1000); // 0.5% slippage
        assert!(quote.price_impact >= 0.0);
        assert_eq!(quote.fee, 2); // 0.25% of 1000
    }

    #[test]
    fn test_edge_cases() {
        let calculator = AmmQuoteCalculator::new();

        // Zero input
        let amount_out = calculator
            .calculate_output_amount(0, 1_000_000, 1_000_000, 0.0025)
            .unwrap();
        assert_eq!(amount_out, 0);

        // Zero reserves should error
        let result = calculator.calculate_output_amount(1000, 0, 1_000_000, 0.0025);
        assert!(result.is_err());
    }
}