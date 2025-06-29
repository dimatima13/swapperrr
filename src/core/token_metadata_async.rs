use crate::core::{SwapError, SwapResult, TokenInfo};
use borsh::BorshDeserialize;
use log::debug;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{
    account::Account,
    pubkey::Pubkey,
};
use std::str::FromStr;
use std::sync::Arc;

// Metaplex Token Metadata Program
pub const TOKEN_METADATA_PROGRAM_ID: &str = "metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s";

// Metadata account structure
#[derive(Debug, BorshDeserialize)]
pub struct Metadata {
    pub key: u8,
    pub update_authority: Pubkey,
    pub mint: Pubkey,
    pub data: Data,
    pub primary_sale_happened: bool,
    pub is_mutable: bool,
    pub edition_nonce: Option<u8>,
    pub token_standard: Option<u8>,
    pub collection: Option<Collection>,
    pub uses: Option<Uses>,
}

#[derive(Debug, BorshDeserialize)]
pub struct Data {
    pub name: String,
    pub symbol: String,
    pub uri: String,
    pub seller_fee_basis_points: u16,
    pub creators: Option<Vec<Creator>>,
}

#[derive(Debug, BorshDeserialize)]
pub struct Creator {
    pub address: Pubkey,
    pub verified: bool,
    pub share: u8,
}

#[derive(Debug, BorshDeserialize)]
pub struct Collection {
    pub verified: bool,
    pub key: Pubkey,
}

#[derive(Debug, BorshDeserialize)]
pub struct Uses {
    pub use_method: u8,
    pub remaining: u64,
    pub total: u64,
}

pub struct AsyncTokenMetadataFetcher {
    client: Arc<RpcClient>,
}

impl AsyncTokenMetadataFetcher {
    pub fn new(client: Arc<RpcClient>) -> Self {
        Self { client }
    }

    /// Get token metadata for a given mint
    pub async fn get_token_metadata(&self, mint: &Pubkey) -> SwapResult<TokenInfo> {
        // Derive metadata account address
        let metadata_program = Pubkey::from_str(TOKEN_METADATA_PROGRAM_ID).unwrap();
        let metadata_seeds = &[
            b"metadata",
            metadata_program.as_ref(),
            mint.as_ref(),
        ];
        
        let (metadata_account, _) = Pubkey::find_program_address(metadata_seeds, &metadata_program);
        
        debug!("Looking for metadata account: {} for mint: {}", metadata_account, mint);
        
        // Try to fetch metadata account
        match self.client.get_account(&metadata_account).await {
            Ok(account) => {
                // Parse metadata
                match self.parse_metadata(&account) {
                    Ok(metadata) => {
                        let name = metadata.data.name.trim_matches('\0').to_string();
                        let symbol = metadata.data.symbol.trim_matches('\0').to_string();
                        
                        debug!("Found metadata: name={}, symbol={}", name, symbol);
                        
                        // Get decimals from mint account
                        let decimals = self.get_token_decimals(mint).await?;
                        
                        Ok(TokenInfo {
                            mint: *mint,
                            symbol,
                            decimals,
                            name,
                        })
                    }
                    Err(e) => {
                        debug!("Failed to parse metadata: {}", e);
                        // Return default if metadata parsing fails
                        self.get_default_token_info(mint).await
                    }
                }
            }
            Err(e) => {
                debug!("No metadata account found: {}", e);
                // Return default if no metadata found
                self.get_default_token_info(mint).await
            }
        }
    }

    /// Parse metadata from account data
    fn parse_metadata(&self, account: &Account) -> SwapResult<Metadata> {
        // Skip the first byte (discriminator) and parse just the metadata part
        if account.data.len() < 100 {
            return Err(SwapError::ParseError("Account data too small for metadata".to_string()));
        }
        
        // Try parsing from the beginning of the data
        // Metadata structure is at the start of the account
        match Metadata::deserialize(&mut &account.data[..]) {
            Ok(metadata) => Ok(metadata),
            Err(e) => Err(SwapError::ParseError(format!("Failed to parse metadata: {}", e)))
        }
    }

    /// Get token decimals from mint account
    async fn get_token_decimals(&self, mint: &Pubkey) -> SwapResult<u8> {
        // Special case for SOL
        let sol_mint = Pubkey::from_str("So11111111111111111111111111111111111111112").unwrap();
        if mint == &sol_mint {
            return Ok(9);
        }

        match self.client.get_account(mint).await {
            Ok(account) => {
                // SPL Token mint is 82 bytes, decimals is at offset 44
                if account.data.len() >= 82 {
                    Ok(account.data[44])
                } else {
                    debug!("Invalid mint account data length: {}", account.data.len());
                    Ok(6) // Default to 6 decimals
                }
            }
            Err(e) => {
                debug!("Failed to get mint account: {}", e);
                Ok(6) // Default to 6 decimals
            }
        }
    }

    /// Get default token info when metadata is not available
    async fn get_default_token_info(&self, mint: &Pubkey) -> SwapResult<TokenInfo> {
        let sol_mint = Pubkey::from_str("So11111111111111111111111111111111111111112").unwrap();
        let usdc_mint = Pubkey::from_str("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v").unwrap();
        let usdt_mint = Pubkey::from_str("Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB").unwrap();
        
        let (symbol, name, decimals) = if mint == &sol_mint {
            ("SOL".to_string(), "Solana".to_string(), 9)
        } else if mint == &usdc_mint {
            ("USDC".to_string(), "USD Coin".to_string(), 6)
        } else if mint == &usdt_mint {
            ("USDT".to_string(), "Tether USD".to_string(), 6)
        } else {
            // For unknown tokens, try to get decimals at least
            let decimals = self.get_token_decimals(mint).await?;
            ("UNKNOWN".to_string(), "Unknown Token".to_string(), decimals)
        };

        Ok(TokenInfo {
            mint: *mint,
            symbol,
            decimals,
            name,
        })
    }
}