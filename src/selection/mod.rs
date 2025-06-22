use crate::core::{PoolInfo, PoolType, QuoteRequest, QuoteResult, SwapResult};
use crate::discovery::PoolDiscovery;
use crate::quotes::QuoteEngine;
use log::{debug, info};
use std::sync::Arc;

/// Smart pool selector that finds the best pool for a swap
pub struct PoolSelector {
    discovery: Arc<PoolDiscovery>,
    quote_engine: Arc<QuoteEngine>,
}

impl PoolSelector {
    pub fn new(discovery: Arc<PoolDiscovery>, quote_engine: Arc<QuoteEngine>) -> Self {
        Self {
            discovery,
            quote_engine,
        }
    }

    /// Select the best pool for a swap request
    pub async fn select_best_pool(
        &self,
        request: &QuoteRequest,
    ) -> SwapResult<Option<QuoteResult>> {
        // Discover all available pools
        let pools = self
            .discovery
            .discover_all_pools(request.token_in, request.token_out)
            .await?;

        if pools.is_empty() {
            return Ok(None);
        }

        info!(
            "Found {} pools for {}/{}",
            pools.len(),
            request.token_in,
            request.token_out
        );

        // Get quotes from all pools
        let quotes = self.get_quotes_from_pools(&pools, request).await;

        // Select the best quote
        let best_quote = self.select_best_quote(quotes);

        Ok(best_quote)
    }

    /// Get quotes from all pools
    async fn get_quotes_from_pools(
        &self,
        pools: &[PoolInfo],
        request: &QuoteRequest,
    ) -> Vec<QuoteResult> {
        let mut valid_quotes = Vec::new();

        for pool in pools {
            match self.quote_engine.calculate_quote(pool, request).await {
                Ok(quote) => {
                    debug!(
                        "Pool {} ({:?}): {} -> {} (impact: {:.2}%)",
                        pool.address,
                        pool.pool_type,
                        quote.amount_in,
                        quote.amount_out,
                        quote.price_impact
                    );
                    valid_quotes.push(quote);
                }
                Err(e) => {
                    debug!("Failed to get quote from pool {}: {}", pool.address, e);
                }
            }
        }

        valid_quotes
    }

    /// Select the best quote based on output amount and other factors
    fn select_best_quote(&self, quotes: Vec<QuoteResult>) -> Option<QuoteResult> {
        quotes
            .into_iter()
            .max_by_key(|quote| {
                // Primary criterion: maximum output amount
                let output_score = quote.amount_out;

                // Secondary criterion: prefer certain pool types for stability
                let type_bonus = match quote.pool_info.pool_type {
                    PoolType::Stable => {
                        // Bonus for stable pools if dealing with stablecoins
                        if self.is_stable_pair(&quote.pool_info) {
                            1000 // Small bonus
                        } else {
                            0
                        }
                    }
                    PoolType::CLMM => 100, // Small bonus for capital efficiency
                    _ => 0,
                };

                output_score + type_bonus
            })
    }

    /// Get quotes from all pools with details
    pub async fn get_all_quotes(
        &self,
        request: &QuoteRequest,
    ) -> SwapResult<Vec<QuoteResult>> {
        let pools = self
            .discovery
            .discover_all_pools(request.token_in, request.token_out)
            .await?;

        Ok(self.get_quotes_from_pools(&pools, request).await)
    }

    /// Get quotes grouped by pool type
    pub async fn get_quotes_by_type(
        &self,
        request: &QuoteRequest,
    ) -> SwapResult<QuotesByType> {
        let all_quotes = self.get_all_quotes(request).await?;

        let mut quotes_by_type = QuotesByType::default();

        for quote in all_quotes {
            match quote.pool_info.pool_type {
                PoolType::AMM => quotes_by_type.amm.push(quote),
                PoolType::Stable => quotes_by_type.stable.push(quote),
                PoolType::CLMM => quotes_by_type.clmm.push(quote),
                PoolType::Standard => quotes_by_type.standard.push(quote),
            }
        }

        Ok(quotes_by_type)
    }

    /// Check if tokens form a stable pair
    fn is_stable_pair(&self, pool: &PoolInfo) -> bool {
        let stable_symbols = ["USDC", "USDT", "DAI", "BUSD", "TUSD", "USDP"];

        stable_symbols.contains(&pool.token_a.symbol.as_str())
            && stable_symbols.contains(&pool.token_b.symbol.as_str())
    }
}

/// Container for quotes grouped by pool type
#[derive(Default)]
pub struct QuotesByType {
    pub amm: Vec<QuoteResult>,
    pub stable: Vec<QuoteResult>,
    pub clmm: Vec<QuoteResult>,
    pub standard: Vec<QuoteResult>,
}

impl QuotesByType {
    /// Get total number of quotes
    pub fn total(&self) -> usize {
        self.amm.len() + self.stable.len() + self.clmm.len() + self.standard.len()
    }

    /// Get best quote across all types
    pub fn best_quote(&self) -> Option<&QuoteResult> {
        let all_quotes: Vec<&QuoteResult> = self
            .amm
            .iter()
            .chain(self.stable.iter())
            .chain(self.clmm.iter())
            .chain(self.standard.iter())
            .collect();

        all_quotes
            .into_iter()
            .max_by_key(|quote| quote.amount_out)
    }

    /// Get summary statistics
    pub fn summary(&self) -> PoolTypeSummary {
        PoolTypeSummary {
            amm_count: self.amm.len(),
            stable_count: self.stable.len(),
            clmm_count: self.clmm.len(),
            standard_count: self.standard.len(),
            best_amm: self.amm.iter().max_by_key(|q| q.amount_out).cloned(),
            best_stable: self.stable.iter().max_by_key(|q| q.amount_out).cloned(),
            best_clmm: self.clmm.iter().max_by_key(|q| q.amount_out).cloned(),
            best_standard: self.standard.iter().max_by_key(|q| q.amount_out).cloned(),
        }
    }
}

/// Summary of quotes by pool type
pub struct PoolTypeSummary {
    pub amm_count: usize,
    pub stable_count: usize,
    pub clmm_count: usize,
    pub standard_count: usize,
    pub best_amm: Option<QuoteResult>,
    pub best_stable: Option<QuoteResult>,
    pub best_clmm: Option<QuoteResult>,
    pub best_standard: Option<QuoteResult>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{Config, PoolState, TokenInfo};
    use solana_sdk::pubkey::Pubkey;


    fn create_test_quote(pool_type: PoolType, amount_out: u64) -> QuoteResult {
        let pool_info = PoolInfo {
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
            liquidity_usd: 100000.0,
            volume_24h_usd: 50000.0,
            fee_rate: 0.0025,
            program_id: Pubkey::new_unique(),
            pool_state: match pool_type {
                PoolType::AMM => PoolState::AMM {
                    reserve_a: 1000000,
                    reserve_b: 1000000,
                    nonce: 1,
                },
                PoolType::Stable => PoolState::Stable {
                    reserves: vec![1000000, 1000000],
                    amp_factor: 1000,
                },
                PoolType::CLMM => PoolState::CLMM {
                    current_tick: 0,
                    tick_spacing: 64,
                    liquidity: 1000000,
                    fee_tier: 250,
                },
                PoolType::Standard => PoolState::Standard {
                    reserve_a: 1000000,
                    reserve_b: 1000000,
                },
            },
        };

        QuoteResult {
            pool_info: pool_info.clone(),
            amount_in: 1000,
            amount_out,
            min_amount_out: amount_out * 99 / 100,
            price_impact: 0.1,
            fee: 2,
            route: vec![Pubkey::new_unique()],
            token_in: pool_info.token_a.mint,
            token_out: pool_info.token_b.mint,
        }
    }

    #[test]
    fn test_select_best_quote() {
        let selector = PoolSelector {
            discovery: Arc::new(PoolDiscovery::new(Config::default()).unwrap()),
            quote_engine: Arc::new(QuoteEngine::new()),
        };

        let quotes = vec![
            create_test_quote(PoolType::AMM, 1000),
            create_test_quote(PoolType::Stable, 1005),
            create_test_quote(PoolType::CLMM, 1003),
            create_test_quote(PoolType::Standard, 995),
        ];

        let best = selector.select_best_quote(quotes).unwrap();
        assert_eq!(best.amount_out, 1005); // Stable pool has best output
    }

    #[test]
    fn test_quotes_by_type() {
        let mut quotes_by_type = QuotesByType::default();

        quotes_by_type.amm.push(create_test_quote(PoolType::AMM, 1000));
        quotes_by_type.stable.push(create_test_quote(PoolType::Stable, 1005));
        quotes_by_type.clmm.push(create_test_quote(PoolType::CLMM, 1003));

        assert_eq!(quotes_by_type.total(), 3);

        let best = quotes_by_type.best_quote().unwrap();
        assert_eq!(best.amount_out, 1005);

        let summary = quotes_by_type.summary();
        assert_eq!(summary.amm_count, 1);
        assert_eq!(summary.stable_count, 1);
        assert_eq!(summary.clmm_count, 1);
        assert_eq!(summary.standard_count, 0);
    }

    #[test]
    fn test_stable_pair_detection() {
        let selector = PoolSelector {
            discovery: Arc::new(PoolDiscovery::new(Config::default()).unwrap()),
            quote_engine: Arc::new(QuoteEngine::new()),
        };

        let mut pool = PoolInfo {
            pool_type: PoolType::Stable,
            address: Pubkey::new_unique(),
            token_a: TokenInfo {
                mint: Pubkey::new_unique(),
                symbol: "USDC".to_string(),
                decimals: 6,
                name: "USD Coin".to_string(),
            },
            token_b: TokenInfo {
                mint: Pubkey::new_unique(),
                symbol: "USDT".to_string(),
                decimals: 6,
                name: "Tether".to_string(),
            },
            liquidity_usd: 1000000.0,
            volume_24h_usd: 500000.0,
            fee_rate: 0.0004,
            program_id: Pubkey::new_unique(),
            pool_state: PoolState::Stable {
                reserves: vec![1000000, 1000000],
                amp_factor: 1000,
            },
        };

        assert!(selector.is_stable_pair(&pool));

        // Change one token to non-stable
        pool.token_a.symbol = "SOL".to_string();
        assert!(!selector.is_stable_pair(&pool));
    }
}