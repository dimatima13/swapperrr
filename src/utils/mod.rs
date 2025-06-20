use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;

/// Parse token symbol or address to Pubkey
pub fn parse_token_identifier(input: &str) -> Option<Pubkey> {
    // First try to parse as Pubkey
    if let Ok(pubkey) = Pubkey::from_str(input) {
        return Some(pubkey);
    }

    // Common token mappings
    match input.to_uppercase().as_str() {
        "SOL" | "WSOL" => Some(Pubkey::from_str("So11111111111111111111111111111111111111112").unwrap()),
        "USDC" => Some(Pubkey::from_str("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v").unwrap()),
        "USDT" => Some(Pubkey::from_str("Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB").unwrap()),
        "BONK" => Some(Pubkey::from_str("DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263").unwrap()),
        _ => None,
    }
}

/// Format large numbers with thousands separators
pub fn format_number(n: u64) -> String {
    let s = n.to_string();
    let mut result = String::new();
    let mut count = 0;

    for ch in s.chars().rev() {
        if count == 3 {
            result.push(',');
            count = 0;
        }
        result.push(ch);
        count += 1;
    }

    result.chars().rev().collect()
}

/// Calculate percentage change
pub fn calculate_percentage_change(from: f64, to: f64) -> f64 {
    if from == 0.0 {
        return 0.0;
    }
    ((to - from) / from) * 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_token_identifier() {
        // Test known tokens
        assert!(parse_token_identifier("SOL").is_some());
        assert!(parse_token_identifier("USDC").is_some());
        assert!(parse_token_identifier("usdt").is_some()); // Case insensitive

        // Test pubkey parsing
        let pubkey_str = "So11111111111111111111111111111111111111112";
        assert!(parse_token_identifier(pubkey_str).is_some());

        // Test unknown token
        assert!(parse_token_identifier("UNKNOWN").is_none());
    }

    #[test]
    fn test_format_number() {
        assert_eq!(format_number(1234567890), "1,234,567,890");
        assert_eq!(format_number(1000), "1,000");
        assert_eq!(format_number(999), "999");
        assert_eq!(format_number(0), "0");
    }

    #[test]
    fn test_calculate_percentage_change() {
        assert_eq!(calculate_percentage_change(100.0, 110.0), 10.0);
        assert_eq!(calculate_percentage_change(100.0, 90.0), -10.0);
        assert_eq!(calculate_percentage_change(0.0, 100.0), 0.0);
    }
}