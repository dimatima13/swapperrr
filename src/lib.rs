#[macro_use]
extern crate lazy_static;

pub mod cli;
pub mod core;
pub mod discovery;
pub mod quotes;
pub mod selection;
pub mod transaction;
pub mod utils;

// Re-export commonly used types
pub use core::{Config, PoolInfo, PoolType, QuoteRequest, QuoteResult, SwapError, SwapResult};
pub use discovery::PoolDiscovery;
pub use quotes::QuoteCalculator;
pub use selection::PoolSelector;
pub use transaction::TransactionExecutor;