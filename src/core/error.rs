use solana_client::client_error::ClientError;
use solana_sdk::pubkey::ParsePubkeyError;
use thiserror::Error;

pub type SwapResult<T> = Result<T, SwapError>;

#[derive(Error, Debug)]
pub enum SwapError {
    #[error("No pools found for pair {0}/{1}")]
    NoPoolsFound(String, String),

    #[error("Insufficient liquidity in {pool_type:?} pool: available {available}, required {required}")]
    InsufficientLiquidity {
        pool_type: crate::core::types::PoolType,
        available: u64,
        required: u64,
    },

    #[error("Slippage exceeded: expected {expected}, got {actual} (max allowed: {max_slippage}%)")]
    SlippageExceeded {
        expected: u64,
        actual: u64,
        max_slippage: f64,
    },

    #[error("Pool type {0:?} not supported for this pair")]
    UnsupportedPoolType(crate::core::types::PoolType),

    #[error("Invalid token mint: {0}")]
    InvalidTokenMint(String),

    #[error("Invalid amount: {0}")]
    InvalidAmount(String),

    #[error("RPC error: {0}")]
    RpcError(#[from] ClientError),

    #[error("Serialization error: {0}")]
    SerializationError(String),

    #[error("Math overflow in calculation")]
    MathOverflow,

    #[error("Invalid pool state: {0}")]
    InvalidPoolState(String),

    #[error("Transaction failed: {0}")]
    TransactionFailed(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("Cache error: {0}")]
    CacheError(String),

    #[error("Parse error: {0}")]
    ParseError(#[from] ParsePubkeyError),

    #[error("Pool not found: {0}")]
    PoolNotFound(String),

    #[error("Token not found: {0}")]
    TokenNotFound(String),

    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("Timeout: operation took longer than {0} seconds")]
    Timeout(u64),

    #[error("Invalid slippage: {0}")]
    InvalidSlippage(String),

    #[error("Simulation failed: {0}")]
    SimulationFailed(String),

    #[error("Other error: {0}")]
    Other(String),
}

impl From<anyhow::Error> for SwapError {
    fn from(err: anyhow::Error) -> Self {
        SwapError::Other(err.to_string())
    }
}

impl From<reqwest::Error> for SwapError {
    fn from(err: reqwest::Error) -> Self {
        SwapError::NetworkError(err.to_string())
    }
}

impl From<serde_json::Error> for SwapError {
    fn from(err: serde_json::Error) -> Self {
        SwapError::SerializationError(err.to_string())
    }
}

impl From<std::io::Error> for SwapError {
    fn from(err: std::io::Error) -> Self {
        SwapError::Other(err.to_string())
    }
}