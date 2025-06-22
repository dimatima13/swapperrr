# Swapperrr - Raydium Multi-Pool DeFi Tool

A professional Rust-based tool for interacting with multiple Raydium pool types (AMM, Stable, CLMM) on Solana.

## Features

- ✅ **Multi-Pool Support**: AMM V4, Stable Pools, CLMM (Concentrated Liquidity)
- ✅ **On-Chain Discovery**: Direct blockchain queries, no external APIs
- ✅ **Smart Routing**: Automatically finds the best pool for your swap
- ✅ **Price Impact Calculation**: Real-time slippage and impact analysis
- ✅ **Transaction Building**: Complete swap execution
- ✅ **CLI Interface**: Easy-to-use command line tool

## Installation

```bash
# Clone the repository
git clone https://github.com/yourusername/swapperrr.git
cd swapperrr

# Build the project
cargo build --release

# Run tests
cargo test
```

## Configuration

Create a `.env` file in the project root:

```env
RPC_URL=https://mainnet.helius-rpc.com/?api-key=YOUR_API_KEY
PRIVATE_KEY=your_wallet_private_key_base58_or_json_array
```

## Usage

### Get Quote

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

### Execute Swap

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

## Common Token Addresses

- **SOL**: `So11111111111111111111111111111111111111112`
- **USDC**: `EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v`
- **USDT**: `Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB`
- **BONK**: `DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263`

## Architecture

```
src/
├── cli/           # Command-line interface
├── core/          # Core types and layouts
├── discovery/     # Pool discovery modules
├── quotes/        # Quote calculation engines
├── selection/     # Pool selection logic
├── transaction/   # Transaction building
└── utils/         # Utility functions
```

## Pool Types Supported

1. **AMM Pools**: Constant product (x*y=k) pools
2. **Stable Pools**: Optimized for stable pairs with low slippage
3. **CLMM Pools**: Concentrated liquidity for capital efficiency

## Safety Features

- Slippage protection
- Price impact warnings
- Transaction simulation before execution
- Automatic retry with exponential backoff

## Development

```bash
# Run with debug logging
RUST_LOG=debug cargo run -- quote So11111111111111111111111111111111111111112 EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v 1

# Run specific example
cargo run --example test_quote_and_swap

# Format code
cargo fmt

# Run clippy
cargo clippy
```

## License

MIT

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.