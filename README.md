# Offline Election Tool

Networks like Polkadot, Kusama, and their parachains rely on **staking elections** to decide which validators secure the network. Those elections determine who gets to validate blocks and how nominator stake is distributed, but they happen on-chain, in real time, and the results aren’t always easy to predict or analyze in advance.

The **Offline Election Tool** lets you run the same election logic **off-chain**, using real chain data. This means you can test scenarios, experiment with parameters and see potential outcomes without modifying the live network. That makes it easier to understand, plan, and improve participation in proof-of-stake systems.

### Who Is It For?

- **Validators** — See how you might perform in upcoming elections, test different setups, and plan for parameter changes.
- **Nominators** — Explore how your nominations could translate into elected validators and stake distribution before you commit.
- **Researchers & analysts** — Study election algorithms (e.g. Phragmén, PhragMMS), compare outcomes across blocks, and analyze fairness and distribution.
- **Developers & operators** — Integrate election simulation into dashboards, tooling via the REST API, or run one-off experiments from the command line.

### What You Can Do With It

- **Simulate elections** using the same algorithms as the chain (sequential Phragmén or PhragMMS), including optional balancing and reduce steps.
- **Snapshot staking state** at any block (latest or historical) to see candidates and nominators exactly as the chain sees them.
- **Experiment safely** with manual overrides: add or remove candidates and voters, adjust stake amounts, and see how results change without affecting the live network.
- **Run it as a service** via the REST API for integration with other tools, UIs, or automation.

Whether you’re checking your validator strategy, exploring nomination strategies, or just curious how an election would look under different conditions, this tool lets you simulate scenarios and inspect results without affecting the live network.

## Usage

```bash
offline-election-tool [OPTIONS] --rpc-endpoint <RPC_ENDPOINT> <COMMAND>
```

### Commands

- `simulate [OPTIONS]` - Simulate the election using the specified algorithm (seq-phragmen or phragmms)
- `snapshot` - Retrieve actual snapshot containing validator candidates and their voters
- `server [OPTIONS]` - Start REST API server
- `help` - Print help message

### Global Options

- `-r, --rpc-endpoint <RPC_ENDPOINT>` - RPC endpoint URL
- `-h, --help` - Print help
- `-V, --version` - Print version

### Simulate Command Options

- `-b, --block <BLOCK>` - Block hash for snapshot (default: "latest" for latest block)
- `-a, --algorithm <ALGORITHM>` - Election algorithm to use: `seq-phragmen` (default) or `phragmms`
- `-i, --iterations <ITERATIONS>` - Number of iterations for the balancing algorithm (default: 0)
- `--reduce` - Apply reduce algorithm to minimize output assignments
- `--desired-validators <COUNT>` - Desired number of validators to elect (optional, uses chain default if not specified)
- `--max-nominations <COUNT>` - Maximum nominations per voter (optional, uses chain default if not specified)
- `--min-nominator-bond <AMOUNT>` - Minimum nominator bond (optional, uses chain default if not specified)
- `--min-validator-bond <AMOUNT>` - Minimum validator bond (optional, uses chain default if not specified)
- `-o, --output <FILE>` - Write JSON output to file (default: "simulate.json", use "-" to print to stdout)
- `-m, --manual-override <FILE>` - Path to JSON file for manual override of voters and candidates

### Snapshot Command Options

- `-b, --block <BLOCK>` - Block hash for snapshot (default: "latest" for latest block)
- `-o, --output <FILE>` - Write JSON output to file (default: "snapshot.json", use "-" to print to stdout)

### Server Command Options

- `-a, --address <ADDRESS>` - Server address to bind to (default: "127.0.0.1:3000")


### Examples

#### Retrieve snapshot for latest block:
```bash
cargo run -- --rpc-endpoint wss://sys.ibp.network/asset-hub-polkadot snapshot
```
*Note: If the block contains an election snapshot, it will be retrieved. Otherwise, a snapshot will be generated from current staking data.*

#### Simulate election for latest block:
```bash
cargo run -- --rpc-endpoint wss://sys.ibp.network/asset-hub-polkadot simulate
```

#### Simulate election for specific block:
```bash
cargo run -- --rpc-endpoint wss://sys.ibp.network/asset-hub-polkadot simulate --block 0xc9b9a5d6efa7c36e9501b53a4ebdf77def3e7560d2520254ed1a5bb6035acae4
```

#### Simulate with PhragMMS algorithm:
```bash
cargo run -- --rpc-endpoint wss://sys.ibp.network/asset-hub-polkadot simulate --algorithm phragmms
```

#### Simulate with balancing iterations and reduce:
```bash
cargo run -- --rpc-endpoint wss://sys.ibp.network/asset-hub-polkadot simulate --iterations 10 --reduce
```

#### Simulate with manual override:
```bash
cargo run -- --rpc-endpoint wss://sys.ibp.network/asset-hub-polkadot simulate --manual-override override.json
```

Manual override JSON file format:
```json
{
  "candidates": ["15S7YtETM31QxYYqubAwRJKRSM4v4Ua6WGFYnx1VuFBnWqdG"],
  "candidates_remove": [],
  "voters": [
    ["15S7YtETM31QxYYqubAwRJKRSM4v4Ua6WGFYnx1VuFBnWqdG", 1000000, ["15S7YtETM31QxYYqubAwRJKRSM4v4Ua6WGFYnx1VuFBnWqdG"]]
  ],
  "voters_remove": []
}
```

The manual override feature allows you to:
- Add candidates that may not exist on-chain
- Remove specific candidates from the election
- Add or override voters with custom stake amounts (regardless of on-chain bonded amounts)
- Remove specific voters from the election

#### Save output to specific file names:
```bash
cargo run -- --rpc-endpoint wss://sys.ibp.network/asset-hub-polkadot simulate --output simulate_output.json
cargo run -- --rpc-endpoint wss://sys.ibp.network/asset-hub-polkadot snapshot --output snapshot_output.json
```

#### Start REST API server:
```bash
cargo run -- --rpc-endpoint wss://sys.ibp.network/asset-hub-polkadot server
```

Start server on custom address:
```bash
cargo run -- --rpc-endpoint wss://sys.ibp.network/asset-hub-polkadot server --address 0.0.0.0:8080
```

## REST API Endpoints

When running in server mode, the following REST API endpoints are available:

### POST /simulate

Simulate an election with specified parameters.

**Query Parameters:**
- `block` (optional) - Block hash for snapshot (defaults to latest block)

**Request Body (JSON):**
```json
{
  "desired_validators": 297,
  "algorithm": "seq-phragmen",
  "iterations": 10,
  "reduce": true,
  "max_nominations": 16,
  "min_nominator_bond": 1000000000,
  "min_validator_bond": 1000000000,
  "manual_override": {
    "candidates": [],
    "candidates_remove": [],
    "voters": [],
    "voters_remove": []
  }
}
```

- `desired_validators` (optional) - Desired number of validators to elect (uses chain default if not specified)
- `algorithm` (optional) - Election algorithm: `"seq-phragmen"` or `"phragmms"` (default: `"seq-phragmen"`)
- `iterations` (optional) - Number of balancing iterations (default: 0)
- `reduce` (optional) - Apply reduce algorithm to minimize assignments (default: false)
- `max_nominations` (optional) - Maximum nominations per voter (uses chain default if not specified)
- `min_nominator_bond` (optional) - Minimum nominator bond (uses chain default if not specified)
- `min_validator_bond` (optional) - Minimum validator bond (uses chain default if not specified)
- `manual_override` (optional) - Manual override object for voters and candidates (same format as CLI manual override file)

**Success Response (200 OK):**
```json
{
  "result": {
    "run_parameters": {...},
    "active_validators": [...]
  }
}
```

### GET /snapshot

Retrieve election snapshot containing validator candidates and their voters.

**Query Parameters:**
- `block` (optional) - Block hash for snapshot (defaults to latest block)

**Success Response (200 OK):**
```json
{
  "result": {
    "validators": [...],
    "nominators": [...],
    "config": {...}
  }
}
```

## Docker

To build the Docker image locally, run:
```bash
docker build -t offline-election-tool .
```
Image can be pulled from Docker Hub:
```bash
docker pull bilinearlabs/offline-election-tool:<commit-hash>
```

Then run the available commands in the container:
```bash
docker run bilinearlabs/offline-election-tool:<commit-hash> <command>
```

To run the tool in server mode in the container:
```bash
docker run -p 3000:3000 bilinearlabs/offline-election-tool:<commit-hash> --rpc-endpoint wss://sys.ibp.network/asset-hub-polkadot server --address 0.0.0.0:3000
```