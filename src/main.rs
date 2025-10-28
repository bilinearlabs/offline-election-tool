use log::info;
use sp_core::H256;
use clap::{arg, command, Parser, Subcommand};
use sp_core::crypto::{set_default_ss58_version};
use std::fs::File;
use std::io::Write;
use crate::models::Chain;

// mod network;
mod storage_client;
mod primitives;
mod snapshot;
mod models;
mod simulate;

#[derive(Parser, Debug)]
pub struct SimulateArgs {
    /// Count of validators to elect (optional, uses chain default if not specified)
    #[arg(short, long)]
    pub count: Option<usize>,

    /// Number of iterations for the balancing algorithm
    #[arg(short, long, default_value = "0")]
    pub iterations: usize,

    /// Apply reduce algorithm to output assignments
    #[arg(long)]
    pub reduce: bool,
}

#[derive(Subcommand, Debug)]
enum Action {
    /// Simulate the election using Sequential Phragmen algorithm
    Simulate(SimulateArgs),
    /// Retrieve actual snapshot containing validator candidates and their voters
    Snapshot,
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

    /// Block with Snapshot (Signed or Unsigned phase) 
    #[arg(short, long, default_value = "latest")]
    block: String,

    /// Output file path (if not specified, prints to stdout)
    #[arg(short, long)]
    output: Option<String>,

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
    let args = Args::parse();
    
    // Block with Snapshot (Signed or Unsigned phase) 
    // Block with PhaseTransitioned event without ElectionFinalized event after it
    // 0x7d5c645873ec013d9e1bd844c5fd24c60f5a1a1266c5a02fe5bc35e50a23f750
    let block: Option<H256> = if args.block == "latest" {
        None
    } else {
        Some(args.block.parse().unwrap())
    };

    let client = storage_client::StorageClient::new(&args.rpc_endpoint).await?;
    let runtime_version = client.get_runtime_version(block).await?;
    let runtime_chain = match runtime_version.spec_name.to_string().as_str() {
        "polkadot" => Chain::Polkadot,
        "kusama" => Chain::Kusama,
        "substrate" => Chain::Substrate,
        _ => return Err("Unsupported chain".into()),
    };
    let chain = args.chain.unwrap_or(runtime_chain);
    set_default_ss58_version(chain.ss58_address_format());

    match args.action {
        Action::Simulate(simulate_args) => {
            info!("Running election simulation...");
            let election_result = simulate::simulate_seq_phragmen(&client, block, simulate_args).await;
            if election_result.is_err() {  
                return Err(format!("Error in election simulation -> {}", election_result.err().unwrap()).into());
            }
            write_output(&election_result.unwrap(), args.output)?;
        }
        Action::Snapshot => {
            info!("Taking snapshot...");
            let snapshot = snapshot::build(&client, block).await;
            if snapshot.is_err() {
                return Err(format!("Error generating snapshot -> {}", snapshot.err().unwrap()).into());
            }
            let snapshot = snapshot.unwrap();
            let snapshot_json = serde_json::to_string_pretty(&snapshot).unwrap();
            println!("{}", snapshot_json);
        }
    }
    
    Ok(())

}
