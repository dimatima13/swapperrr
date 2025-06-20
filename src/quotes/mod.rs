pub mod amm_calculator;
pub mod clmm_calculator;
pub mod stable_calculator;
pub mod standard_calculator;

use crate::core::{PoolInfo, QuoteRequest, QuoteResult, SwapResult};

pub use amm_calculator::AmmQuoteCalculator;
pub use clmm_calculator::ClmmQuoteCalculator;
pub use stable_calculator::StableQuoteCalculator;
pub use standard_calculator::StandardQuoteCalculator;

/// Trait for pool-specific quote calculation
#[async_trait::async_trait]
pub trait QuoteCalculator: Send + Sync {
    async fn calculate_quote(
        &self,
        pool: &PoolInfo,
        request: &QuoteRequest,
    ) -> SwapResult<QuoteResult>;
}

/// Main quote engine that delegates to pool-specific calculators
pub struct QuoteEngine {
    amm_calculator: AmmQuoteCalculator,
    stable_calculator: StableQuoteCalculator,
    clmm_calculator: ClmmQuoteCalculator,
    standard_calculator: StandardQuoteCalculator,
}

impl QuoteEngine {
    pub fn new() -> Self {
        Self {
            amm_calculator: AmmQuoteCalculator::new(),
            stable_calculator: StableQuoteCalculator::new(),
            clmm_calculator: ClmmQuoteCalculator::new(),
            standard_calculator: StandardQuoteCalculator::new(),
        }
    }

    /// Calculate quote for a specific pool
    pub async fn calculate_quote(
        &self,
        pool: &PoolInfo,
        request: &QuoteRequest,
    ) -> SwapResult<QuoteResult> {
        match pool.pool_type {
            crate::core::PoolType::AMM => {
                self.amm_calculator.calculate_quote(pool, request).await
            }
            crate::core::PoolType::Stable => {
                self.stable_calculator.calculate_quote(pool, request).await
            }
            crate::core::PoolType::CLMM => {
                self.clmm_calculator.calculate_quote(pool, request).await
            }
            crate::core::PoolType::Standard => {
                self.standard_calculator.calculate_quote(pool, request).await
            }
        }
    }

    /// Calculate quotes for multiple pools
    pub async fn calculate_quotes(
        &self,
        pools: &[PoolInfo],
        request: &QuoteRequest,
    ) -> Vec<SwapResult<QuoteResult>> {
        let mut results = Vec::new();
        
        for pool in pools {
            results.push(self.calculate_quote(pool, request).await);
        }
        
        results
    }
}

impl Default for QuoteEngine {
    fn default() -> Self {
        Self::new()
    }
}