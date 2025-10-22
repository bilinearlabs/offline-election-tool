use sp_core::{H256, crypto::Ss58Codec};

use crate::{models::account_to_ss58_for_chain, primitives::AccountId};

// mod network;
mod storage_client;
mod primitives;
mod snapshot;
mod models;
mod simulate;


#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // subxt metadata --url https://rpc.polkadot.io > polkadot_metadata.scale
    const NODE_URL: &str = "wss://rpc.polkadot.io";

    // Block with Snapshot (Signed or Unsigned phase) 
    // Block with PhaseTransitioned event without ElectionFinalized event after it
    // 0x7d5c645873ec013d9e1bd844c5fd24c60f5a1a1266c5a02fe5bc35e50a23f750
    let block: Option<H256> = Some("0x7d5c645873ec013d9e1bd844c5fd24c60f5a1a1266c5a02fe5bc35e50a23f750".parse().unwrap());
    let client = storage_client::StorageClient::new(NODE_URL).await?;

    // let active_era = client.get_active_era(block).await?;
    // if active_era.is_none() {
    //     return Err("Active era not found".into());
    // }
    // let active_era = active_era.unwrap();
    // println!("Active era: {:?}", active_era);

    // let total_issuance = client.get_total_issuance_at(block).await?;
    // println!("Total issuance: {:?}", total_issuance);

    // let election_phase = client.get_election_phase(block).await?;
    // println!("Election phase: {:?}", election_phase);

    let election_result = simulate::simulate_seq_phragmen(&client, block).await;
    if election_result.is_err() {
        return Err("Error in simulate_seq_phragmen".into());
    }
    let election_result = election_result.unwrap();
    println!("Election result:");
    let election_result_json = serde_json::to_string_pretty(&election_result).unwrap();
    println!("{}", election_result_json);
    Ok(())

}
