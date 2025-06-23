use crate::core::{SwapError, SwapResult};
use cached::proc_macro::cached;
use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use solana_client::rpc_client::RpcClient;
use solana_sdk::{pubkey::Pubkey, program_pack::Pack};
use std::str::FromStr;

/// Token metadata information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenMetadata {
    pub mint: Pubkey,
    pub decimals: u8,
    pub name: Option<String>,
    pub symbol: Option<String>,
    pub uri: Option<String>,
}

/// Metaplex Token Metadata Program ID
const TOKEN_METADATA_PROGRAM_ID: &str = "metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s";

/// Get token metadata from mint
#[cached(
    result = true,
    time = 3600, // Cache for 1 hour
    size = 1000,
    key = "String",
    convert = r#"{ format!("{}", mint) }"#
)]
pub fn get_token_metadata_cached(
    rpc_client: &RpcClient,
    mint: &Pubkey,
) -> SwapResult<TokenMetadata> {
    get_token_metadata_impl(rpc_client, mint)
}

/// Implementation of token metadata fetching
fn get_token_metadata_impl(
    rpc_client: &RpcClient,
    mint: &Pubkey,
) -> SwapResult<TokenMetadata> {
    info!("Fetching metadata for token: {}", mint);
    
    // First, get decimals from the mint account
    let mint_account = rpc_client
        .get_account(mint)
        .map_err(|e| SwapError::Other(format!("Failed to get mint account: {}", e)))?;
    
    // Parse SPL Token mint data
    let mint_data = spl_token::state::Mint::unpack(&mint_account.data)
        .map_err(|e| SwapError::Other(format!("Failed to parse mint data: {}", e)))?;
    
    let decimals = mint_data.decimals;
    debug!("Token {} has {} decimals", mint, decimals);
    
    // Try to get Metaplex metadata
    let (metadata_name, metadata_symbol, metadata_uri) = match get_metaplex_metadata(rpc_client, mint) {
        Ok((name, symbol, uri)) => (Some(name), Some(symbol), Some(uri)),
        Err(e) => {
            warn!("Could not fetch Metaplex metadata for {}: {}", mint, e);
            (None, None, None)
        }
    };
    
    Ok(TokenMetadata {
        mint: *mint,
        decimals,
        name: metadata_name,
        symbol: metadata_symbol,
        uri: metadata_uri,
    })
}

/// Get Metaplex metadata for a token
fn get_metaplex_metadata(
    rpc_client: &RpcClient,
    mint: &Pubkey,
) -> SwapResult<(String, String, String)> {
    // Derive metadata account PDA
    let metadata_program_id = Pubkey::from_str(TOKEN_METADATA_PROGRAM_ID)
        .map_err(|e| SwapError::Other(format!("Invalid metadata program ID: {}", e)))?;
    
    let (metadata_pda, _) = Pubkey::find_program_address(
        &[
            b"metadata",
            metadata_program_id.as_ref(),
            mint.as_ref(),
        ],
        &metadata_program_id,
    );
    
    debug!("Metadata PDA for {}: {}", mint, metadata_pda);
    
    // Fetch metadata account
    let metadata_account = rpc_client
        .get_account(&metadata_pda)
        .map_err(|e| SwapError::Other(format!("Failed to get metadata account: {}", e)))?;
    
    // Parse metadata
    let metadata = parse_metadata(&metadata_account.data)?;
    
    Ok((metadata.name, metadata.symbol, metadata.uri))
}

/// Metaplex metadata structure (simplified)
#[derive(Debug)]
struct Metadata {
    pub name: String,
    pub symbol: String,
    pub uri: String,
}

/// Parse Metaplex metadata from account data
fn parse_metadata(data: &[u8]) -> SwapResult<Metadata> {
    // Skip discriminator and key (1 + 1 = 2 bytes)
    let mut offset = 2;
    
    // Skip update authority (32 bytes)
    offset += 32;
    
    // Skip mint (32 bytes)
    offset += 32;
    
    // Read name
    let name = read_string(data, &mut offset)?;
    
    // Read symbol  
    let symbol = read_string(data, &mut offset)?;
    
    // Read uri
    let uri = read_string(data, &mut offset)?;
    
    Ok(Metadata {
        name: name.trim_end_matches('\0').to_string(),
        symbol: symbol.trim_end_matches('\0').to_string(),
        uri: uri.trim_end_matches('\0').to_string(),
    })
}

/// Read a string from buffer with length prefix
fn read_string(data: &[u8], offset: &mut usize) -> SwapResult<String> {
    if *offset + 4 > data.len() {
        return Err(SwapError::Other("Buffer too small for string length".to_string()));
    }
    
    // Read length (4 bytes, little endian)
    let len = u32::from_le_bytes([
        data[*offset],
        data[*offset + 1],
        data[*offset + 2],
        data[*offset + 3],
    ]) as usize;
    
    *offset += 4;
    
    if *offset + len > data.len() {
        return Err(SwapError::Other("Buffer too small for string data".to_string()));
    }
    
    let string_data = &data[*offset..*offset + len];
    *offset += len;
    
    String::from_utf8(string_data.to_vec())
        .map_err(|e| SwapError::Other(format!("Invalid UTF-8 string: {}", e)))
}

/// Get token decimals only (faster than full metadata)
pub fn get_token_decimals(
    rpc_client: &RpcClient,
    mint: &Pubkey,
) -> SwapResult<u8> {
    // Check common known tokens first
    if let Some(decimals) = get_known_token_decimals(mint) {
        return Ok(decimals);
    }
    
    // Fetch from chain
    let mint_account = rpc_client
        .get_account(mint)
        .map_err(|e| SwapError::Other(format!("Failed to get mint account: {}", e)))?;
    
    let mint_data = spl_token::state::Mint::unpack(&mint_account.data)
        .map_err(|e| SwapError::Other(format!("Failed to parse mint data: {}", e)))?;
    
    Ok(mint_data.decimals)
}

/// Get decimals for known tokens without RPC call
fn get_known_token_decimals(mint: &Pubkey) -> Option<u8> {
    match mint.to_string().as_str() {
        // SOL
        "So11111111111111111111111111111111111111112" => Some(9),
        // USDC
        "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v" => Some(6),
        // USDT
        "Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB" => Some(6),
        // RAY
        "4k3Dyjzvzp8eMZWUXbBCjEvwSkkk59S5iCNLY3QrkX6R" => Some(6),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_known_token_decimals() {
        let sol_mint = Pubkey::from_str("So11111111111111111111111111111111111111112").unwrap();
        assert_eq!(get_known_token_decimals(&sol_mint), Some(9));
        
        let usdc_mint = Pubkey::from_str("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v").unwrap();
        assert_eq!(get_known_token_decimals(&usdc_mint), Some(6));
        
        let random_mint = Pubkey::new_unique();
        assert_eq!(get_known_token_decimals(&random_mint), None);
    }
}