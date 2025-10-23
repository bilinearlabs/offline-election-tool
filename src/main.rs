use log::info;
use sp_core::H256;
use clap::{arg, command, Parser, Subcommand};
use crate::models::Chain;

// mod network;
mod storage_client;
mod primitives;
mod snapshot;
mod models;
mod simulate;

#[derive(Subcommand, Debug)]
enum Action {
    /// Simulate the election using Sequential Phragmen algorithm
    Simulate,
    /// Retrieve actual snapshot containing validator candidates and their voters
    Snapshot,
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Polkadot, Kusama, Substrate
    #[arg(short, long)]
    chain: Chain,

    /// Block with Snapshot (Signed or Unsigned phase) 
    #[arg(short, long, default_value = "latest")]
    block: String,

    #[command(subcommand)]
    action: Action,
}


#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    
    // subxt metadata --url https://rpc.polkadot.io > polkadot_metadata.scale
    const NODE_URL: &str = "wss://rpc.polkadot.io";

    // Block with Snapshot (Signed or Unsigned phase) 
    // Block with PhaseTransitioned event without ElectionFinalized event after it
    // 0x7d5c645873ec013d9e1bd844c5fd24c60f5a1a1266c5a02fe5bc35e50a23f750
    let block: Option<H256> = if args.block == "latest" {
        None
    } else {
        Some(args.block.parse().unwrap())
    };
    
    // TODO remove debug prints
    // println!("Using chain: {:?}", args.chain);
    // println!("SS58 prefix: {}", args.chain.ss58_prefix());
    // println!("Block: {:?}", block);
    
    let client = storage_client::StorageClient::new(NODE_URL).await?;

    match args.action {
        Action::Simulate => {
            info!("Running election simulation...");
            let election_result = simulate::simulate_seq_phragmen(&client, block).await;
            if election_result.is_err() {
                return Err("Error in simulate_seq_phragmen".into());
            }
            let election_result = election_result.unwrap();
            let election_result_json = serde_json::to_string_pretty(&election_result).unwrap();
            println!("{}", election_result_json);
        }
        Action::Snapshot => {
            info!("Taking snapshot...");
            let snapshot = snapshot::build(&client, args.chain, block).await;
            if snapshot.is_err() {
                return Err(snapshot.err().unwrap());
            }
            let snapshot = snapshot.unwrap();
            let snapshot_json = serde_json::to_string_pretty(&snapshot).unwrap();
            println!("{}", snapshot_json);
        }
    }
    
    Ok(())

}
