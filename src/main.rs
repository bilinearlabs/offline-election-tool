use sp_core::H256;

use crate::models::account_to_ss58_for_chain;

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
    let block: Option<H256> = Some("0x7d5c645873ec013d9e1bd844c5fd24c60f5a1a1266c5a02fe5bc35e50a23f750".parse().unwrap());
    let client = storage_client::StorageClient::new(NODE_URL).await?;
    // TODO remove just for debugging connection
    let total_issuance = client.get_total_issuance_at(block).await?;
    println!("Total issuance: {:?}", total_issuance);

    let election_phase = client.get_election_phase(block).await?;
    println!("Election phase: {:?}", election_phase);

    let election_result = simulate::simulate_seq_phragmen(&client, block).await;
    if election_result.is_err() {
        println!("Error: {:?}", election_result.err().unwrap());
        return Ok(());
    }
    let election_result = election_result.unwrap();
    println!("Election result: {:#?}", election_result.winners.len());
    for winner in election_result.winners.iter().take(10) {
        println!("Winner: {:?}", account_to_ss58_for_chain(&winner.0, models::Chain::Polkadot));
    }

    Ok(())

}
