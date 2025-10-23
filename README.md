# Offline Election Tool

A Rust tool for simulating Substrate-based blockchain elections offline.

## Usage

```bash
offline-election-tool [OPTIONS] --chain <CHAIN> <COMMAND>
```

### Commands

- `simulate` - Simulate the election using Sequential Phragmen algorithm
- `snapshot` - Retrieve actual snapshot containing validator candidates and their voters
- `help` - Print help message

### Options

- `-c, --chain <CHAIN>` - Target chain (currently supports: polkadot)
- `-b, --block <BLOCK>` - Block with Snapshot (Signed or Unsigned phase) [default: latest]
- `-h, --help` - Print help
- `-V, --version` - Print version

### Examples

#### Retrieve actual snapshot (will only work if last block has snapshot):
```bash
cargo run -- --chain polkadot snapshot
```

#### Simulate election for latest block (will only work if last block has snapshot):
```bash
cargo run -- --chain polkadot simulate
```

#### Simulate election for specific block:
```bash
cargo run -- --chain polkadot --block 0x7d5c645873ec013d9e1bd844c5fd24c60f5a1a1266c5a02fe5bc35e50a23f750 simulate
```

#### Save output to files:
```bash
cargo run -- --chain polkadot simulate > election_results.json
cargo run -- --chain polkadot snapshot > staking_snapshot.json
```