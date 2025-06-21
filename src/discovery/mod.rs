pub mod amm_pool_parser;
pub mod stable_pool_parser;
pub mod clmm_pool_parser;
pub mod pool_cache;
pub mod pool_finder;
pub mod pool_scorer;

#[cfg(test)]
mod tests;

use crate::core::{Config, PoolInfo, PoolType, SwapResult};
use solana_sdk::pubkey::Pubkey;
use std::sync::Arc;

pub use pool_cache::PoolCache;
pub use pool_finder::{PoolFinder, PoolDiscoveryService};
pub use pool_scorer::PoolScorer;

/// Main interface for pool discovery
pub struct PoolDiscovery {
    finder: Arc<PoolDiscoveryService>,
    cache: Arc<PoolCache>,
    scorer: PoolScorer,
}

impl PoolDiscovery {
    pub fn new(config: Config) -> SwapResult<Self> {
        let finder = Arc::new(PoolDiscoveryService::new(config.clone())?);
        let cache = Arc::new(PoolCache::new(config.cache_ttl_secs));
        let scorer = PoolScorer::new();

        Ok(Self {
            finder,
            cache,
            scorer,
        })
    }

    /// Discover all available pools for a token pair
    pub async fn discover_all_pools(
        &self,
        token_a: Pubkey,
        token_b: Pubkey,
    ) -> SwapResult<Vec<PoolInfo>> {
        // Check cache first
        if let Some(pools) = self.cache.get(&(token_a, token_b)).await {
            return Ok(pools);
        }

        // Parallel discovery of all pool types
        let pools = self.finder.discover_all(token_a, token_b).await?;

        // Score and sort pools
        let mut scored_pools = self.scorer.score_pools(pools);
        scored_pools.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());

        // Extract pool info
        let pools: Vec<PoolInfo> = scored_pools.into_iter().map(|sp| sp.pool).collect();

        // Cache the results
        self.cache.set((token_a, token_b), pools.clone()).await;

        Ok(pools)
    }

    /// Find best pool for a token pair
    pub async fn find_best_pool(
        &self,
        token_a: Pubkey,
        token_b: Pubkey,
    ) -> SwapResult<Option<PoolInfo>> {
        let pools = self.discover_all_pools(token_a, token_b).await?;
        Ok(pools.into_iter().next())
    }

    /// Find pools by type
    pub async fn find_pools_by_type(
        &self,
        token_a: Pubkey,
        token_b: Pubkey,
        pool_type: PoolType,
    ) -> SwapResult<Vec<PoolInfo>> {
        let pools = self.discover_all_pools(token_a, token_b).await?;
        Ok(pools
            .into_iter()
            .filter(|p| p.pool_type == pool_type)
            .collect())
    }

    /// Invalidate cache for a token pair
    pub async fn invalidate_cache(&self, token_a: Pubkey, token_b: Pubkey) {
        self.cache.invalidate(&(token_a, token_b)).await;
    }
}