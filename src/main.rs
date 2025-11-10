use tracing::info;
use sp_core::H256;
use clap::{arg, command, Parser, Subcommand};
use sp_core::crypto::set_default_ss58_version;
use std::fs::File;
use std::io::Write;
use std::sync::Arc;
use jsonrpsee_ws_client::WsClient;
use crate::api::routes::root;
use crate::models::{Chain, Algorithm};
use crate::multi_block_state_client::MultiBlockClient;
use crate::subxt_client::Client;

mod raw_state_client;
mod primitives;
mod snapshot;
mod models;
mod simulate;
mod api;
mod subxt_client;
mod multi_block_state_client;
mod miner_config;

#[derive(Parser, Debug)]
pub struct SimulateArgs {
    /// Block with Snapshot (Signed or Unsigned phase) 
    #[arg(short, long, default_value = "latest")]
    pub block: String,

    /// Count of validators to elect (optional, uses chain default if not specified)
    #[arg(short, long)]
    pub count: Option<usize>,

    /// Election algorithm to use (seq-phragmen or phragmms)
    #[arg(short, long, default_value = "seq-phragmen")]
    pub algorithm: Algorithm,

    /// Number of iterations for the balancing algorithm
    #[arg(short, long, default_value = "0")]
    pub iterations: usize,

    /// Apply reduce algorithm to output assignments
    #[arg(long)]
    pub reduce: bool,

    /// Output file path (if not specified, prints to stdout)
    #[arg(short, long)]
    pub output: Option<String>,

    /// Manual override JSON file path for voters and candidates
    #[arg(short = 'm', long)]
    pub manual_override: Option<String>,
}

#[derive(Parser, Debug)]
pub struct SnapshotArgs {
    /// Block with Snapshot (Signed or Unsigned phase) 
    #[arg(short, long, default_value = "latest")]
    pub block: String,

    /// Output file path (if not specified, prints to stdout)
    #[arg(short, long)]
    pub output: Option<String>,
}

#[derive(Subcommand, Debug)]
enum Action {
    /// Simulate the election using the specified algorithm (seq_phragmen or phragmms)
    Simulate(SimulateArgs),
    /// Retrieve actual snapshot containing validator candidates and their voters
    Snapshot(SnapshotArgs),

    /// Start REST API server
    Server {
        /// Server address to bind to
        #[arg(short, long, default_value = "127.0.0.1:3000")]
        address: String,
    },
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Polkadot, Kusama, Substrate. If not specified, the chain will be inferred from the runtime version.
    #[arg(short, long)]
    chain: Option<Chain>,

    /// RPC endpoint URL (must be aligned with the chain)
    #[arg(short, long)]
    rpc_endpoint: String,

    #[command(subcommand)]
    action: Action,
}

fn write_output<T: serde::Serialize>(data: &T, output: Option<String>) -> Result<(), Box<dyn std::error::Error>> {
    let json = serde_json::to_string_pretty(data)?;
    if let Some(file_path) = output {
        let mut file = File::create(file_path)?;
        file.write_all(json.as_bytes())?;
    } else {
        println!("{}", json);
    }
    Ok(())
}


#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing for all commands
    // Use INFO level for CLI commands, DEBUG level for server
    let args = Args::parse();
    
    let log_level = if matches!(args.action, Action::Server { .. }) {
        tracing::Level::DEBUG
    } else {
        tracing::Level::INFO
    };
    
    tracing_subscriber::fmt()
        .with_max_level(log_level)
        .with_target(false)
        .init();

    let raw_client = raw_state_client::RawClient::new(&args.rpc_endpoint).await?;
    let subxt_client = subxt_client::Client::new(&args.rpc_endpoint).await?;
    
    let runtime_version = raw_client.get_runtime_version().await?;
    let runtime_chain = match runtime_version.spec_name.to_string().as_str() {
        "polkadot" => Chain::Polkadot,
        "kusama" => Chain::Kusama,
        "substrate" => Chain::Substrate,
        "statemint" => Chain::Polkadot,
        "statemine" => Chain::Kusama,
        _ => return Err("Unsupported chain".into()),
    };

    let chain = args.chain.unwrap_or(runtime_chain);
    set_default_ss58_version(chain.ss58_address_format());

    // Fetch all constants from chain API
    let miner_constants = miner_config::fetch_constants(&subxt_client).await?;
    info!("Fetched constants: pages={}, max_winners_per_page={}, max_backers_per_winner={}, voter_snapshot_per_block={}, target_snapshot_per_block={}, max_length={}, max_votes_per_voter={}",
        miner_constants.pages,
        miner_constants.max_winners_per_page,
        miner_constants.max_backers_per_winner,
        miner_constants.voter_snapshot_per_block,
        miner_constants.target_snapshot_per_block,
        miner_constants.max_length,
        miner_constants.max_votes_per_voter
    );
    
    // Set runtime constants
    miner_config::set_runtime_constants(miner_constants.clone());
    
    // Set balancing iterations from args if simulating
    if let Action::Simulate(ref simulate_args) = args.action {
        miner_config::set_balancing_iterations(simulate_args.iterations);
    }

    match args.action {
        Action::Simulate(simulate_args) => {
            let block: Option<H256> = if simulate_args.block == "latest" {
                None
            } else {
                Some(simulate_args.block.parse().unwrap())
            };

            let output = simulate_args.output.clone();
            info!("Running election simulation with {:?} algorithm...", simulate_args.algorithm);
            let targets_count = simulate_args.count;
            let algorithm = simulate_args.algorithm;
            let iterations = simulate_args.iterations;
            miner_config::set_balancing_iterations(iterations);
            let apply_reduce = simulate_args.reduce;
            let manual_override = simulate_args.manual_override.clone();
            
            let election_result = with_miner_config!(chain, {
                let multi_block_client = MultiBlockClient::<Client, MinerConfig>::new(subxt_client.clone());
                simulate::simulate::<_, Client, MinerConfig>(
                    &raw_client,
                    &multi_block_client,
                    block,
                    targets_count,
                    algorithm,
                    apply_reduce,
                    manual_override,
                ).await
            });
            if election_result.is_err() {  
                return Err(format!("Error in election simulation -> {}", election_result.err().unwrap()).into());
            }
            write_output(&election_result.unwrap(), output)?;
        }
        Action::Snapshot(snapshot_args) => {
            let block: Option<H256> = if snapshot_args.block == "latest" {
                None
            } else {
                Some(snapshot_args.block.parse().unwrap())
            };

            info!("Taking snapshot...");
            let snapshot = with_miner_config!(chain, {
                let multi_block_client = MultiBlockClient::<Client, MinerConfig>::new(subxt_client.clone());
                snapshot::build::<WsClient, Client, MinerConfig>(&multi_block_client, &raw_client, block).await
            });
            if snapshot.is_err() {
                return Err(format!("Error generating snapshot -> {}", snapshot.err().unwrap()).into());
            }
            let snapshot = snapshot.unwrap();
            write_output(&snapshot, snapshot_args.output)?;
        }
        Action::Server { address } => {
            info!("Starting server on {}", address);
            let storage_client = Arc::new(raw_client);
            let listener = tokio::net::TcpListener::bind(address).await?;
            with_miner_config!(chain, {
                let multi_block_client = Arc::new(MultiBlockClient::<Client, MinerConfig>::new(subxt_client.clone()));
                let router = root::routes::<MinerConfig>(storage_client, multi_block_client, chain);
                axum::serve(listener, router)
                    .await
                    .unwrap_or_else(|e| panic!("Error starting server: {}", e));
            });
        }
    }
    Ok(())
}
