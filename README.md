# Offline Election Tool

A Rust tool for simulating Substrate-based blockchain elections offline.

## Usage

```bash
offline-election-tool [OPTIONS] --chain <CHAIN> --rpc-endpoint <RPC_ENDPOINT> <COMMAND>
```

### Commands

- `simulate` - Simulate the election using Sequential Phragmen algorithm
- `snapshot` - Retrieve actual snapshot containing validator candidates and their voters
- `help` - Print help message

### Options

- `-c, --chain <CHAIN>` - Target chain (currently supports: polkadot)
- `-r, --rpc-endpoint <RPC_ENDPOINT>` - RPC endpoint URL (must be aligned with the chain)
- `-b, --block <BLOCK>` - Block with Snapshot (Signed or Unsigned phase) [default: latest]
- `-h, --help` - Print help
- `-V, --version` - Print version

**Important:** The `--chain` and `--rpc-endpoint` arguments must be aligned. For example, if you specify `--chain polkadot`, you must use a Polkadot RPC endpoint.

### Examples

#### Retrieve actual snapshot (will only work if last block has snapshot):
```bash
cargo run -- --chain polkadot --rpc-endpoint wss://rpc.polkadot.io snapshot
```

#### Simulate election for latest block (will only work if last block has snapshot):
```bash
cargo run -- --chain polkadot --rpc-endpoint wss://rpc.polkadot.io simulate
```

#### Simulate election for specific block:
```bash
cargo run -- --chain polkadot --rpc-endpoint wss://rpc.polkadot.io --block 0xc9b9a5d6efa7c36e9501b53a4ebdf77def3e7560d2520254ed1a5bb6035acae4 simulate
```

#### Save output to files:
```bash
cargo run -- --chain polkadot --rpc-endpoint wss://rpc.polkadot.io simulate > election_results.json
cargo run -- --chain polkadot --rpc-endpoint wss://rpc.polkadot.io snapshot > staking_snapshot.json
```