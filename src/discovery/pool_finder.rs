use crate::core::{
    error::SwapResult, Config, PoolInfo,
};
use crate::discovery::amm_pool_parser::AmmPoolParser;
use crate::discovery::stable_pool_parser::StablePoolParser;
use crate::discovery::clmm_pool_parser_optimized::OptimizedClmmPoolParser;
use crate::discovery::cp_pool_parser::CpPoolParser;
use futures::future::join_all;
use log::{debug, info, warn};
use solana_client::{
    nonblocking::rpc_client::RpcClient,
};
use solana_sdk::{commitment_config::CommitmentConfig, pubkey::Pubkey};
use std::sync::Arc;

/// Trait for pool-specific discovery
#[async_trait::async_trait]
pub trait PoolFinder: Send + Sync {
    async fn find_pools(
        &self,
        token_a: Pubkey,
        token_b: Pubkey,
    ) -> SwapResult<Vec<PoolInfo>>;
}

/// Main pool discovery service
pub struct PoolDiscoveryService {
    rpc_client: Arc<RpcClient>,
    config: Config,
    finders: Vec<Box<dyn PoolFinder>>,
}

impl PoolDiscoveryService {
    pub fn new(config: Config) -> SwapResult<Self> {
        let rpc_client = Arc::new(RpcClient::new_with_commitment(
            config.rpc_url.clone(),
            CommitmentConfig::confirmed(),
        ));

        let finders: Vec<Box<dyn PoolFinder>> = vec![
            Box::new(AmmPoolFinder::new(rpc_client.clone(), config.rpc_url.clone())),
            Box::new(StablePoolFinder::new(rpc_client.clone())),
            Box::new(ClmmPoolFinder::new(rpc_client.clone(), config.rpc_url.clone())),
            Box::new(StandardPoolFinder::new(rpc_client.clone())), // This now handles CP pools
        ];

        Ok(Self {
            rpc_client,
            config,
            finders,
        })
    }

    /// Discover all pools in parallel
    pub async fn discover_all(
        &self,
        token_a: Pubkey,
        token_b: Pubkey,
    ) -> SwapResult<Vec<PoolInfo>> {
        info!("Discovering all pools for {}/{}", token_a, token_b);

        let futures = self.finders.iter().map(|finder| {
            finder.find_pools(token_a, token_b)
        });

        let results = join_all(futures).await;
        
        let mut all_pools = Vec::new();
        for (i, result) in results.into_iter().enumerate() {
            match result {
                Ok(pools) => {
                    debug!("Found {} pools from finder {}", pools.len(), i);
                    all_pools.extend(pools);
                }
                Err(e) => {
                    warn!("Error from finder {}: {}", i, e);
                }
            }
        }

        // TODO: handle this
        // Filter by minimum liquidity
        // all_pools.retain(|pool| pool.liquidity_usd >= self.config.min_liquidity_usd);

        info!("Found {} total pools after filtering", all_pools.len());
        Ok(all_pools)
    }

    /// Find all pools containing a specific token
    pub async fn find_pools_by_token(&self, token: Pubkey) -> SwapResult<Vec<PoolInfo>> {
        info!("Searching for all pools containing token {}", token);

        // TODO: Implement token-based discovery across all finders
        // For now, only use AMM finder - expand to other pool types later
        let amm_finder = AmmPoolFinder::new(self.rpc_client.clone(), self.config.rpc_url.clone());
        amm_finder.parser.find_pools_by_token(token).await
    }
}

/// AMM Pool Finder
struct AmmPoolFinder {
    parser: AmmPoolParser,
}

impl AmmPoolFinder {
    fn new(rpc_client: Arc<RpcClient>, rpc_url: String) -> Self {
        Self {
            parser: AmmPoolParser::new(rpc_client, rpc_url),
        }
    }
}

#[async_trait::async_trait]
impl PoolFinder for AmmPoolFinder {
    async fn find_pools(
        &self,
        token_a: Pubkey,
        token_b: Pubkey,
    ) -> SwapResult<Vec<PoolInfo>> {
        self.parser.find_pools_for_pair(token_a, token_b).await
    }
}

/// Stable Pool Finder
struct StablePoolFinder {
    parser: StablePoolParser,
}

impl StablePoolFinder {
    fn new(rpc_client: Arc<RpcClient>) -> Self {
        Self {
            parser: StablePoolParser::new(rpc_client),
        }
    }
}

#[async_trait::async_trait]
impl PoolFinder for StablePoolFinder {
    async fn find_pools(
        &self,
        token_a: Pubkey,
        token_b: Pubkey,
    ) -> SwapResult<Vec<PoolInfo>> {
        self.parser.find_pools_for_pair(token_a, token_b).await
    }
}

/// CLMM Pool Finder
struct ClmmPoolFinder {
    parser: OptimizedClmmPoolParser,
}

impl ClmmPoolFinder {
    fn new(rpc_client: Arc<RpcClient>, rpc_url: String) -> Self {
        Self {
            parser: OptimizedClmmPoolParser::new(rpc_client, rpc_url),
        }
    }
}

#[async_trait::async_trait]
impl PoolFinder for ClmmPoolFinder {
    async fn find_pools(
        &self,
        token_a: Pubkey,
        token_b: Pubkey,
    ) -> SwapResult<Vec<PoolInfo>> {
        self.parser.find_pools_for_pair(token_a, token_b).await
    }
}

/// Standard Pool Finder (delegates to CP pool finder since they're the same)
struct StandardPoolFinder {
    cp_finder: CpPoolFinder,
}

impl StandardPoolFinder {
    fn new(rpc_client: Arc<RpcClient>) -> Self {
        Self { 
            cp_finder: CpPoolFinder::new(rpc_client)
        }
    }
}

#[async_trait::async_trait]
impl PoolFinder for StandardPoolFinder {
    async fn find_pools(
        &self,
        token_a: Pubkey,
        token_b: Pubkey,
    ) -> SwapResult<Vec<PoolInfo>> {
        debug!("Searching for Standard (CP) pools for {}/{}", token_a, token_b);
        
        // Standard pools are actually CP pools
        self.cp_finder.find_pools(token_a, token_b).await
    }
}

/// CP Pool Finder
struct CpPoolFinder {
    parser: CpPoolParser,
}

impl CpPoolFinder {
    fn new(rpc_client: Arc<RpcClient>) -> Self {
        Self {
            parser: CpPoolParser::new(rpc_client),
        }
    }
}

#[async_trait::async_trait]
impl PoolFinder for CpPoolFinder {
    async fn find_pools(
        &self,
        token_a: Pubkey,
        token_b: Pubkey,
    ) -> SwapResult<Vec<PoolInfo>> {
        self.parser.find_pools_for_pair(token_a, token_b).await
    }
}