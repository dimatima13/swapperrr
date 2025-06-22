use crate::core::PoolInfo;
use dashmap::DashMap;
use solana_sdk::pubkey::Pubkey;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Cache entry with TTL
#[derive(Clone)]
struct CacheEntry {
    pools: Vec<PoolInfo>,
    expires_at: Instant,
}

/// Thread-safe pool cache with TTL
pub struct PoolCache {
    cache: Arc<DashMap<(Pubkey, Pubkey), CacheEntry>>,
    ttl: Duration,
}

impl PoolCache {
    pub fn new(ttl_secs: u64) -> Self {
        Self {
            cache: Arc::new(DashMap::new()),
            ttl: Duration::from_secs(ttl_secs),
        }
    }

    /// Get pools from cache if not expired
    pub async fn get(&self, key: &(Pubkey, Pubkey)) -> Option<Vec<PoolInfo>> {
        // Try both orderings of the key
        let keys = vec![*key, (key.1, key.0)];
        
        for k in keys {
            if let Some(entry) = self.cache.get(&k) {
                if entry.expires_at > Instant::now() {
                    return Some(entry.pools.clone());
                } else {
                    // Remove expired entry
                    drop(entry);
                    self.cache.remove(&k);
                }
            }
        }
        
        None
    }

    /// Set pools in cache
    pub async fn set(&self, key: (Pubkey, Pubkey), pools: Vec<PoolInfo>) {
        let entry = CacheEntry {
            pools,
            expires_at: Instant::now() + self.ttl,
        };
        
        // Store with both orderings for easy lookup
        self.cache.insert(key, entry.clone());
        self.cache.insert((key.1, key.0), entry);
    }

    /// Invalidate cache entry
    pub async fn invalidate(&self, key: &(Pubkey, Pubkey)) {
        self.cache.remove(key);
        self.cache.remove(&(key.1, key.0));
    }

    /// Clear all cache entries
    pub async fn clear(&self) {
        self.cache.clear();
    }

    /// Get cache size
    pub fn size(&self) -> usize {
        self.cache.len()
    }

    /// Clean up expired entries
    pub async fn cleanup_expired(&self) {
        let now = Instant::now();
        self.cache.retain(|_, entry| entry.expires_at > now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{PoolState, PoolType, TokenInfo};

    fn create_test_pool() -> PoolInfo {
        PoolInfo {
            pool_type: PoolType::AMM,
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
            // TODO: fix this
            pool_state: PoolState::AMM {
                reserve_a: 1000000,
                reserve_b: 1000000,
            },
        }
    }

    #[tokio::test]
    async fn test_cache_basic_operations() {
        let cache = PoolCache::new(5);
        let key = (Pubkey::new_unique(), Pubkey::new_unique());
        let pools = vec![create_test_pool()];

        // Test set and get
        cache.set(key, pools.clone()).await;
        let cached = cache.get(&key).await;
        assert!(cached.is_some());
        assert_eq!(cached.unwrap().len(), 1);

        // Test reverse key lookup
        let reverse_key = (key.1, key.0);
        let cached_reverse = cache.get(&reverse_key).await;
        assert!(cached_reverse.is_some());
    }

    #[tokio::test]
    async fn test_cache_expiration() {
        let cache = PoolCache::new(1); // 1 second TTL
        let key = (Pubkey::new_unique(), Pubkey::new_unique());
        let pools = vec![create_test_pool()];

        cache.set(key, pools).await;
        assert!(cache.get(&key).await.is_some());

        // Wait for expiration
        tokio::time::sleep(Duration::from_secs(2)).await;
        assert!(cache.get(&key).await.is_none());
    }

    #[tokio::test]
    async fn test_cache_invalidation() {
        let cache = PoolCache::new(60);
        let key = (Pubkey::new_unique(), Pubkey::new_unique());
        let pools = vec![create_test_pool()];

        cache.set(key, pools).await;
        assert!(cache.get(&key).await.is_some());

        cache.invalidate(&key).await;
        assert!(cache.get(&key).await.is_none());
    }
}