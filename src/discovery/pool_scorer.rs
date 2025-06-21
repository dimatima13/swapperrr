use crate::core::{PoolInfo, PoolScore, PoolType};
use log::debug;

/// Pool scoring service
pub struct PoolScorer {
    liquidity_weight: f64,
    volume_weight: f64,
}

impl PoolScorer {
    pub fn new() -> Self {
        Self {
            liquidity_weight: 0.6,
            volume_weight: 0.4,
        }
    }

    /// Score all pools
    pub fn score_pools(&self, pools: Vec<PoolInfo>) -> Vec<PoolScore> {
        pools
            .into_iter()
            .map(|pool| self.score_pool(pool))
            .collect()
    }

    /// Score a single pool
    pub fn score_pool(&self, pool: PoolInfo) -> PoolScore {
        let liquidity_score = self.calculate_liquidity_score(pool.liquidity_usd);
        let volume_score = self.calculate_volume_score(pool.volume_24h_usd);
        let type_bonus = self.get_pool_type_bonus(&pool);

        debug!(
            "Pool {} scoring: liquidity={:.2}, volume={:.2}, type_bonus={:.2}",
            pool.address, liquidity_score, volume_score, type_bonus
        );

        PoolScore::new(pool, liquidity_score, volume_score, type_bonus)
    }

    /// Calculate liquidity score (logarithmic scale)
    fn calculate_liquidity_score(&self, liquidity_usd: f64) -> f64 {
        if liquidity_usd <= 0.0 {
            return 0.0;
        }

        // Logarithmic scoring to avoid over-weighting huge pools
        let score = (liquidity_usd.ln() / 1000000.0_f64.ln()).min(1.0).max(0.0);
        score * 100.0
    }

    /// Calculate volume score (logarithmic scale)
    fn calculate_volume_score(&self, volume_24h_usd: f64) -> f64 {
        if volume_24h_usd <= 0.0 {
            return 0.0;
        }

        // Logarithmic scoring
        let score = (volume_24h_usd.ln() / 100000.0_f64.ln()).min(1.0).max(0.0);
        score * 100.0
    }

    /// Get pool type bonus
    fn get_pool_type_bonus(&self, pool: &PoolInfo) -> f64 {
        match pool.pool_type {
            PoolType::Stable => {
                // Extra bonus for stable pairs
                if self.is_stable_pair(pool) {
                    1.5
                } else {
                    1.2
                }
            }
            PoolType::CLMM => {
                // CLMM pools get bonus for capital efficiency
                // Higher bonus for pools with tighter fee tiers
                if let crate::core::PoolState::CLMM { fee_tier, .. } = &pool.pool_state {
                    match fee_tier {
                        1 => 1.3,    // 0.01% fee - highest capital efficiency
                        5 => 1.25,   // 0.05% fee
                        30 => 1.2,   // 0.3% fee
                        100 => 1.15, // 1% fee
                        _ => 1.1,    // Other fee tiers
                    }
                } else {
                    1.1
                }
            }
            PoolType::AMM => 1.0,
            PoolType::Standard => 0.9, // Slight penalty for legacy pools
        }
    }

    /// Check if tokens are a stable pair
    fn is_stable_pair(&self, pool: &PoolInfo) -> bool {
        let stable_symbols = ["USDC", "USDT", "DAI", "BUSD", "TUSD", "USDP"];
        
        stable_symbols.contains(&pool.token_a.symbol.as_str())
            && stable_symbols.contains(&pool.token_b.symbol.as_str())
    }
}

impl Default for PoolScorer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{PoolState, TokenInfo};
    use solana_sdk::pubkey::Pubkey;

    fn create_test_pool(pool_type: PoolType, liquidity: f64, volume: f64) -> PoolInfo {
        PoolInfo {
            pool_type,
            address: Pubkey::new_unique(),
            token_a: TokenInfo {
                mint: Pubkey::new_unique(),
                symbol: "SOL".to_string(),
                decimals: 9,
                name: "Solana".to_string(),
            },
            token_b: TokenInfo {
                mint: Pubkey::new_unique(),
                symbol: "USDC".to_string(),
                decimals: 6,
                name: "USD Coin".to_string(),
            },
            liquidity_usd: liquidity,
            volume_24h_usd: volume,
            fee_rate: 0.0025,
            program_id: Pubkey::new_unique(),
            pool_state: PoolState::AMM {
                reserve_a: 1000000,
                reserve_b: 1000000,
            },
        }
    }

    #[test]
    fn test_liquidity_scoring() {
        let scorer = PoolScorer::new();

        // Test different liquidity levels
        assert_eq!(scorer.calculate_liquidity_score(0.0), 0.0);
        assert!(scorer.calculate_liquidity_score(1000.0) > 0.0);
        assert!(scorer.calculate_liquidity_score(100000.0) > scorer.calculate_liquidity_score(1000.0));
        assert!(scorer.calculate_liquidity_score(10000000.0) <= 100.0);
    }

    #[test]
    fn test_volume_scoring() {
        let scorer = PoolScorer::new();

        // Test different volume levels
        assert_eq!(scorer.calculate_volume_score(0.0), 0.0);
        assert!(scorer.calculate_volume_score(100.0) > 0.0);
        assert!(scorer.calculate_volume_score(10000.0) > scorer.calculate_volume_score(100.0));
        assert!(scorer.calculate_volume_score(1000000.0) <= 100.0);
    }

    #[test]
    fn test_pool_type_bonus() {
        let scorer = PoolScorer::new();

        let amm_pool = create_test_pool(PoolType::AMM, 100000.0, 50000.0);
        let stable_pool = create_test_pool(PoolType::Stable, 100000.0, 50000.0);
        let clmm_pool = create_test_pool(PoolType::CLMM, 100000.0, 50000.0);

        assert_eq!(scorer.get_pool_type_bonus(&amm_pool), 1.0);
        assert!(scorer.get_pool_type_bonus(&stable_pool) > 1.0);
        assert_eq!(scorer.get_pool_type_bonus(&clmm_pool), 1.1);
    }

    #[test]
    fn test_stable_pair_detection() {
        let scorer = PoolScorer::new();

        let mut pool = create_test_pool(PoolType::Stable, 100000.0, 50000.0);
        pool.token_a.symbol = "USDC".to_string();
        pool.token_b.symbol = "USDT".to_string();

        assert!(scorer.is_stable_pair(&pool));
        assert_eq!(scorer.get_pool_type_bonus(&pool), 1.5);
    }

    #[test]
    fn test_pool_comparison() {
        let scorer = PoolScorer::new();

        let high_liquidity_pool = create_test_pool(PoolType::AMM, 1000000.0, 100000.0);
        let high_volume_pool = create_test_pool(PoolType::AMM, 100000.0, 1000000.0);
        let stable_pool = create_test_pool(PoolType::Stable, 500000.0, 500000.0);

        let scores = scorer.score_pools(vec![
            high_liquidity_pool.clone(),
            high_volume_pool.clone(),
            stable_pool.clone(),
        ]);

        // Stable pool should have higher score due to type bonus
        let stable_score = scores.iter().find(|s| s.pool.pool_type == PoolType::Stable).unwrap();
        assert!(stable_score.type_bonus > 1.0);
    }
}