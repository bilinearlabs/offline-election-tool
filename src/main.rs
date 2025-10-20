use sp_core::H256;

// mod network;
mod storage_client;
mod primitives;
mod snapshot;
mod models;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // subxt metadata --url https://rpc.polkadot.io > polkadot_metadata.scale
    const NODE_URL: &str = "wss://rpc.polkadot.io";
    let block: Option<H256> = Some("0xd6f086a0a3b747d4484275b910837cce933cde470bf4065deba03a58b77f362a".parse().unwrap());
    let client = storage_client::StorageClient::new(NODE_URL).await?;
     let total_issuance = client.get_total_issuance_at(block).await?;
    println!("Total issuance: {:?}", total_issuance);

    // No snapshot
    //let snapshot = client.get_snapshot(block).await?;
    //let snapshot = snapshot::build(&client, models::Chain::Polkadot, block).await?;
        
    // TODO remove, just for debug
    let snapshot = snapshot::build(&client, models::Chain::Polkadot, block).await?;
    let first_validator = serde_json::to_string_pretty(snapshot.validators.get(1).unwrap())?;
    println!("Validator JSON:");
    println!("{}", first_validator);
    let first_nominator = serde_json::to_string_pretty(snapshot.nominators.get(1).unwrap())?;
    println!("Nominator JSON:");
    println!("{}", first_nominator);
    println!("Staking config: {:?}", snapshot.config);

    Ok(())

}
