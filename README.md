# Swapperrr - Technical Assessment Solutions

This repository contains solutions for three technical assessment problems:
1. **Problem 1**: Function to add leading zeros to numbers in a string
2. **Problem 2**: Getting quotes from a DeFi protocol on Solana
3. **Problem 3**: Executing swaps with slippage management

## Solutions Overview

### Problem 1: Pad Zeros
Implemented `pad_zeros` function that finds all numbers in a string and adds leading zeros to reach the specified width.

**Performance**:
- Time complexity: O(n), where n is the length of the input string
- Space complexity: O(n) for the resulting string
- Used `regex` library for correct number matching

### Problem 2 & 3: DeFi Integration
Chose **Raydium** protocol - the leading DEX on Solana with support for multiple pool types.

**Supported Pool Types**:
- AMM V4 (classic x*y=k pools)
- Stable Pools (optimized for stablecoins)
- CLMM (concentrated liquidity)
- Standard/CP Pools

**Key Features**:
- Getting quotes from all available pools
- Automatic selection of optimal pool
- Swap execution with slippage protection
- Detailed transaction reports

## Installation and Setup

```bash
# Clone the repository
git clone https://github.com/yourusername/swapperrr.git
cd swapperrr

# Build the project
cargo build --release

# Run all tests
cargo test

# Run tests for Problem 1
cd problem1 && cargo test
```

## Configuration

For DeFi functionality, create a `.env` file in the project root:

```env
# RPC endpoint (required)
RPC_URL=https://mainnet.helius-rpc.com/?api-key=YOUR_API_KEY

# Wallet private key (for swap execution)
# Format: Base58 string or JSON byte array
PRIVATE_KEY=your_wallet_private_key_base58_or_json_array

# Optional settings
DEFAULT_SLIPPAGE=50      # Default slippage (50 = 0.5%)
CACHE_TTL_POOLS=30        # Cache TTL for pools (seconds)
CACHE_TTL_METADATA=300    # Cache TTL for metadata (seconds)
```

## Running Solutions

### Problem 1: Pad Zeros

```bash
# Run example
cd problem1
cargo run --example demo

# Usage in code:
use problem1::pad_zeros;

let result = pad_zeros("James Bond 7", 3);
assert_eq!(result, "James Bond 007");
```

### Problem 2: Getting Quotes

#### Quote Command

Get swap quotes from all available pools:

```bash
# Get quote for swapping 1 TOKEN to SOL (default)
cargo run -- quote DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263 1

# Get quote for swapping 1 SOL to USDC (specify output token)
cargo run -- quote So11111111111111111111111111111111111111112 1 EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v

# With custom slippage (100 = 1%)
cargo run -- quote DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263 1 --slippage 100

# Show all pools, not just the best
cargo run -- quote DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263 1 --all
```

### Problem 3: Executing Swaps

#### Swap Command

Execute a swap through the best available pool:

```bash
# Swap 1 TOKEN to SOL (default)
cargo run -- swap DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263 1

# Swap 1 SOL to USDC (specify output token)
cargo run -- swap So11111111111111111111111111111111111111112 1 EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v

# Skip confirmation prompt
cargo run -- swap DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263 1 --yes

# With higher slippage tolerance
cargo run -- swap DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263 1 --slippage 200
```

### List Pools

List all available pools for a token pair:

```bash
# List all SOL/USDC pools
cargo run -- pools So11111111111111111111111111111111111111112 EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v

# With detailed information
cargo run -- pools So11111111111111111111111111111111111111112 EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v --detailed
```

### Find Token Pools

Find all pools containing a specific token:

```bash
# Find all pools containing BONK
cargo run -- token-pools DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263

# With detailed information
cargo run -- token-pools DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263 --detailed

# Filter by pool type (amm, stable, clmm)
cargo run -- token-pools DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263 --pool-type amm
```

## Performance and Optimizations

### Problem 1
- **Algorithm**: Single-pass search with regex and replacement
- **Regex choice**: Ensures correctness and code readability
- **Memory optimization**: Minimal allocations

### Problem 2 & 3
- **Parallel requests**: All pool types are queried simultaneously via tokio
- **Caching**: 3-level caching (pools, metadata, tokens) with different TTLs
- **Batch RPC**: Grouped requests to minimize network calls
- **rust_decimal**: Precise calculations without precision loss for financial operations

## AI Usage Disclosure

During the project development, I consulted with an AI assistant (Claude) on the following topics:

### Performance Optimization
**Prompt**: "How to optimize parallel requests to Solana RPC for fetching pool data?"
- Received recommendations for using tokio::join! for parallel requests
- Implemented batch RPC requests to reduce load

### Caching Strategy
**Prompt**: "What's the optimal caching strategy for a DeFi application with frequent quote requests?"
- Implemented multi-level caching with different TTLs
- Added LRU cache for frequently used pools

### Mathematical Calculations
**Prompt**: "How to correctly calculate price impact for CLMM pools considering tick spacing?"
- Validated formulas for concentrated liquidity calculations
- Verified edge case handling correctness

All solutions were thoroughly tested, validated, and adapted to the project's specific requirements.

## Limitations and Assumptions

1. **wSOL requirement**: Some pools only work with wrapped SOL. Implemented wrap/unwrap commands for conversion.
2. **RPC limits**: Using Helius RPC with request rate limits.
3. **Slippage**: Maximum slippage limited to 50% for user protection.
4. **Transaction versions**: Support for both legacy and v0 transactions with ALT.

## Token Addresses

- **SOL**: `So11111111111111111111111111111111111111112`
- **USDC**: `EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v`
- **USDT**: `Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB`
- **BONK**: `DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263`

## Project Architecture

```
swapperrr/
├── problem1/          # Problem 1 solution (pad zeros)
│   ├── src/
│   │   └── lib.rs     # Main pad_zeros function
│   └── examples/
│       └── demo.rs    # Usage examples
├── src/               # Problem 2 & 3 solutions
│   ├── cli/           # CLI interface using clap
│   ├── core/          # Core types and constants
│   ├── discovery/     # On-chain pool discovery
│   ├── quotes/        # Quote calculators
│   ├── selection/     # Optimal pool selection
│   ├── transaction/   # Transaction building
│   └── utils/         # Helper utilities
└── tests/             # Integration tests
```

## Testing

- **Problem 1**: 21 unit tests covering all cases from spec and edge conditions
- **Problem 2 & 3**: 43 unit tests for critical components
- Integration tests for complete quote → swap cycle

## Transaction Report Examples

```
✅ Swap executed successfully!

📊 Transaction Report:
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
Pool Type: AMM V4
Expected price: 0.000234 BONK/SOL
Actual price: 0.000235 BONK/SOL
Slippage: 0.43%
Amount sold: 1000 BONK
Amount received: 0.235 SOL
Transaction: 3xY9k2L...8nF4
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
```

## Development and Debugging

```bash
# Run with debug logging
RUST_LOG=debug cargo run -- quote So11111111111111111111111111111111111111112 1

# Run examples
cargo run --example test_quote_and_swap

# Formatting and linting
cargo fmt && cargo clippy
```

## License

MIT

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.