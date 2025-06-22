use crate::core::{SwapError, SwapResult};
use log::{debug, info, warn};
use solana_address_lookup_table_program::state::AddressLookupTable;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{
    account::Account,
    address_lookup_table::AddressLookupTableAccount,
    pubkey::Pubkey,
    message::v0::MessageAddressTableLookup,
};
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Popular ALT addresses on Solana mainnet
const POPULAR_ALTS: &[(&str, &str)] = &[
    ("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA", "Ea3r8VWC3hgaXjH7NM8RrbMCudTX8zgpzEarrDNPWpEd"), // SPL Token ALT
    ("11111111111111111111111111111111", "1g11BmFKKKBP4xKUQh27F8Qz3bdC23ECrZFdxCCnsT9"), // System Program ALT
    ("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL", "9mBcHPcLhkzmbjRqUxvb9n1vPNJPJaHv6PrTpDpBMuam"), // AToken ALT
];

/// ALT cache entry
#[derive(Clone, Debug)]
struct AltCacheEntry {
    lookup_table: AddressLookupTableAccount,
    addresses: Vec<Pubkey>,
}

/// Address Lookup Table manager
pub struct AltManager {
    rpc_client: Arc<RpcClient>,
    cache: Arc<RwLock<HashMap<Pubkey, AltCacheEntry>>>,
}

impl AltManager {
    /// Create new ALT manager
    pub fn new(rpc_client: Arc<RpcClient>) -> Self {
        Self {
            rpc_client,
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Load popular ALTs into cache
    pub async fn load_popular_alts(&self) -> SwapResult<()> {
        info!("Loading popular Address Lookup Tables");
        
        for (name, alt_address) in POPULAR_ALTS {
            match Pubkey::from_str(alt_address) {
                Ok(pubkey) => {
                    match self.fetch_alt(&pubkey).await {
                        Ok(_) => debug!("Loaded ALT for {}: {}", name, alt_address),
                        Err(e) => warn!("Failed to load ALT for {}: {}", name, e),
                    }
                }
                Err(e) => warn!("Invalid ALT address for {}: {}", name, e),
            }
        }
        
        Ok(())
    }

    /// Fetch and cache an ALT
    async fn fetch_alt(&self, alt_address: &Pubkey) -> SwapResult<AltCacheEntry> {
        // Check cache first
        {
            let cache = self.cache.read().await;
            if let Some(entry) = cache.get(alt_address) {
                debug!("ALT cache hit for {}", alt_address);
                return Ok(entry.clone());
            }
        }

        // Fetch from RPC
        debug!("Fetching ALT from RPC: {}", alt_address);
        let account = self.rpc_client
            .get_account(alt_address)
            .await
            .map_err(|e| SwapError::Other(format!("Failed to fetch ALT {}: {}", alt_address, e)))?;

        // Parse ALT
        let lookup_table = self.parse_alt_account(&account)?;
        let addresses = lookup_table.addresses.to_vec();
        
        let entry = AltCacheEntry {
            lookup_table: lookup_table.clone(),
            addresses: addresses.clone(),
        };

        // Cache the result
        {
            let mut cache = self.cache.write().await;
            cache.insert(*alt_address, entry.clone());
        }

        info!("Loaded ALT {} with {} addresses", alt_address, addresses.len());
        Ok(entry)
    }

    /// Parse ALT account data
    fn parse_alt_account(&self, account: &Account) -> SwapResult<AddressLookupTableAccount> {
        let table = AddressLookupTable::deserialize(&account.data)
            .map_err(|e| SwapError::Other(format!("Failed to parse ALT account: {}", e)))?;
        
        // Convert AddressLookupTable to AddressLookupTableAccount
        Ok(AddressLookupTableAccount {
            key: table.meta.authority.unwrap_or_default(), // Using authority as a placeholder for the key
            addresses: table.addresses.to_vec(),
        })
    }

    /// Find optimal ALTs for a set of accounts
    /// Returns ALT accounts that contain the requested accounts
    pub async fn find_optimal_alts(&self, accounts: &[Pubkey]) -> SwapResult<Vec<AddressLookupTableAccount>> {
        if accounts.is_empty() {
            return Ok(vec![]);
        }

        debug!("Finding optimal ALTs for {} accounts", accounts.len());
        
        // Load popular ALTs if not already loaded
        if self.cache.read().await.is_empty() {
            self.load_popular_alts().await?;
        }

        let mut selected_alts = Vec::new();
        let mut remaining_accounts: Vec<Pubkey> = accounts.to_vec();
        
        // Check each cached ALT
        let cache = self.cache.read().await;
        for (_alt_pubkey, entry) in cache.iter() {
            if remaining_accounts.is_empty() {
                break;
            }

            // Find accounts that exist in this ALT
            let mut found_accounts = Vec::new();

            for account in &remaining_accounts {
                if entry.addresses.contains(account) {
                    found_accounts.push(*account);
                }
            }

            if !found_accounts.is_empty() {
                debug!("Found {} accounts in ALT", found_accounts.len());
                selected_alts.push(entry.lookup_table.clone());

                // Remove found accounts from remaining list
                remaining_accounts.retain(|a| !found_accounts.contains(a));
            }
        }

        if !remaining_accounts.is_empty() {
            debug!("{} accounts not found in any ALT", remaining_accounts.len());
        }

        Ok(selected_alts)
    }

    /// Check if using ALTs would be beneficial
    pub fn should_use_alts(accounts_count: usize) -> bool {
        // Use ALTs if we have more than 20 accounts
        // This threshold can be adjusted based on testing
        accounts_count > 20
    }

    /// Get specific ALT by address
    pub async fn get_alt(&self, alt_address: &Pubkey) -> SwapResult<AddressLookupTableAccount> {
        let entry = self.fetch_alt(alt_address).await?;
        Ok(entry.lookup_table)
    }

    /// Clear ALT cache
    pub async fn clear_cache(&self) {
        let mut cache = self.cache.write().await;
        cache.clear();
        debug!("ALT cache cleared");
    }

    /// Get cache statistics
    pub async fn get_cache_stats(&self) -> (usize, usize) {
        let cache = self.cache.read().await;
        let total_alts = cache.len();
        let total_addresses = cache.values()
            .map(|entry| entry.addresses.len())
            .sum();
        (total_alts, total_addresses)
    }
}

/// Helper functions for ALT operations
pub mod helpers {
    use super::*;

    /// Create lookups for common Raydium accounts
    pub fn create_raydium_lookups(_pool_type: &str) -> Vec<MessageAddressTableLookup> {
        // This would return pre-configured lookups for different pool types
        // For now, return empty as we'd need specific ALT addresses
        vec![]
    }

    /// Estimate transaction size reduction from using ALTs
    pub fn estimate_size_reduction(_accounts: &[Pubkey], lookups: &[MessageAddressTableLookup]) -> usize {
        let accounts_in_alts: usize = lookups.iter()
            .map(|l| l.writable_indexes.len() + l.readonly_indexes.len())
            .sum();
        
        // Each account saves ~32 bytes when using ALT (replaced by 1-2 byte index)
        accounts_in_alts * 30
    }

    /// Check if account is in any of the provided ALTs
    pub fn is_account_in_alts(account: &Pubkey, alts: &[(Pubkey, Vec<Pubkey>)]) -> Option<(Pubkey, u8)> {
        for (alt_pubkey, addresses) in alts {
            if let Some(index) = addresses.iter().position(|a| a == account) {
                return Some((*alt_pubkey, index as u8));
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_use_alts() {
        assert!(!AltManager::should_use_alts(10));
        assert!(!AltManager::should_use_alts(20));
        assert!(AltManager::should_use_alts(21));
        assert!(AltManager::should_use_alts(30));
    }

    #[test]
    fn test_size_reduction_estimation() {
        use helpers::*;
        
        let lookups = vec![
            MessageAddressTableLookup {
                account_key: Pubkey::new_unique(),
                writable_indexes: vec![0, 1],
                readonly_indexes: vec![2, 3, 4],
            },
        ];
        
        let reduction = estimate_size_reduction(&[], &lookups);
        assert_eq!(reduction, 150); // 5 accounts * 30 bytes
    }
}