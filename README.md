# Offline Election Tool

A Rust tool for simulating Substrate-based blockchain elections offline.

## Usage

```bash
offline-election-tool [OPTIONS] --rpc-endpoint <RPC_ENDPOINT> <COMMAND>
```

### Commands

- `simulate [OPTIONS]` - Simulate the election using Sequential Phragmen algorithm
- `snapshot` - Retrieve actual snapshot containing validator candidates and their voters
- `server [OPTIONS]` - Start REST API server
- `help` - Print help message

### Global Options

- `-c, --chain <CHAIN>` - Target chain (polkadot, kusama, substrate). If not specified, the chain will be inferred from the runtime version.
- `-r, --rpc-endpoint <RPC_ENDPOINT>` - RPC endpoint URL (must be aligned with the chain)
- `-h, --help` - Print help
- `-V, --version` - Print version

### Simulate Command Options

- `-b, --block <BLOCK>` - Block hash for snapshot (default: "latest" for latest block)
- `-c, --count <COUNT>` - Count of validators to elect (optional, uses chain default if not specified)
- `-i, --iterations <ITERATIONS>` - Number of iterations for the balancing algorithm (default: 0)
- `--reduce` - Apply reduce algorithm to minimize output assignments
- `-o, --output <FILE>` - Write JSON output to file (optional, prints to stdout if not specified)

### Snapshot Command Options

- `-b, --block <BLOCK>` - Block hash for snapshot (default: "latest" for latest block)
- `-o, --output <FILE>` - Write JSON output to file (optional, prints to stdout if not specified)

### Server Command Options

- `-a, --address <ADDRESS>` - Server address to bind to (default: "127.0.0.1:3000")

**Important:** The `--chain` and `--rpc-endpoint` arguments must be aligned. For example, if you specify `--chain polkadot`, you must use a Polkadot RPC endpoint.

### Examples

#### Retrieve snapshot for latest block:
```bash
cargo run -- --chain polkadot --rpc-endpoint wss://rpc.polkadot.io snapshot
```
*Note: If the block contains an election snapshot, it will be retrieved. Otherwise, a snapshot will be generated from current staking data.*

#### Simulate election for latest block:
```bash
cargo run -- --chain polkadot --rpc-endpoint wss://rpc.polkadot.io simulate
```

#### Simulate election for specific block:
```bash
cargo run -- --chain polkadot --rpc-endpoint wss://rpc.polkadot.io --block 0xc9b9a5d6efa7c36e9501b53a4ebdf77def3e7560d2520254ed1a5bb6035acae4 simulate
```

#### Save output to files:
```bash
cargo run -- --chain polkadot --rpc-endpoint wss://rpc.polkadot.io --output simulate_output.json simulate
cargo run -- --chain polkadot --rpc-endpoint wss://rpc.polkadot.io --output snapshot.json snapshot
```

#### Start REST API server:
```bash
cargo run -- --chain polkadot --rpc-endpoint wss://rpc.polkadot.io server
```

Start server on custom address:
```bash
cargo run -- --chain polkadot --rpc-endpoint wss://rpc.polkadot.io server --address 0.0.0.0:8080
```
