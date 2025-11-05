use crate::{subxt_client::Client, primitives::{Storage, ChainClient}};
use crate::runtime::multi_block::{runtime_types::pallet_election_provider_multi_block::types::Phase, self as runtime};
use parity_scale_codec as codec;
use sp_core::H256;
use subxt::ext::scale_value;

pub struct BlockDetails {
	pub storage: Storage,
	pub phase: Phase,
	pub n_pages: u32,
	pub round: u32,
	pub desired_targets: u32,
	// pub block_number: u32,
}

impl BlockDetails {
    pub async fn new(client: &Client, block: Option<H256>) -> Result<Self, Box<dyn std::error::Error>> {
        let storage = get_storage(client, block).await?;
		let phase = get_phase(&storage).await?;
        let round = get_round(&storage).await?;
        let desired_targets = get_desired_targets(&storage, round).await?;
		
		let n_pages = fetch_constant::<u32>(client.chain_api(), "MultiBlockElection", "Pages").await?;
		
        Ok(Self { storage, phase, n_pages, round, desired_targets })
    }
}

pub async fn fetch_constant<T: serde::de::DeserializeOwned>(
	chain_client: &ChainClient,
	pallet: &str,
	constant_name: &str,
) -> Result<T, Box<dyn std::error::Error>> {
	let constant_key = subxt::dynamic::constant(pallet, constant_name);
	
	let val = chain_client
		.constants()
		.at(&constant_key)
		.map_err(|e| format!("Failed to fetch constant {pallet}::{constant_name}: {e}"))?
        .to_value()
        .map_err(|e| format!("Failed to convert constant {pallet}::{constant_name} to value: {e}"))?;
	
	let val = scale_value::serde::from_value::<_, T>(val).map_err(|e| {
        format!("Failed to decode constant {pallet}::{constant_name} as {}: {e}", std::any::type_name::<T>())
    })?;
	
	Ok(val)
}

pub async fn get_storage(client: &Client, block: Option<H256>) -> Result<Storage, Box<dyn std::error::Error>> {
    if let Some(block) = block {
        Ok(client.chain_api().storage().at(block))
    } else {
        Ok(client.chain_api().storage().at_latest().await?)
    }
}

pub async fn get_phase(storage: &Storage) -> Result<Phase, Box<dyn std::error::Error>> {
    let phase = storage
		.fetch_or_default(&runtime::storage().multi_block_election().current_phase())
		.await?;
    Ok(phase)
}

pub async fn get_round(storage: &Storage) -> Result<u32, Box<dyn std::error::Error>> {
    let storage_key = subxt::dynamic::storage("MultiBlockElection", "Round", vec![]);
    let round_entry = storage
        .fetch(&storage_key)
        .await?
        .ok_or("Round not found")?;
    let round: u32 = codec::Decode::decode(&mut round_entry.encoded())?;
    Ok(round)
}

pub async fn get_desired_targets(storage: &Storage, round: u32) -> Result<u32, Box<dyn std::error::Error>> {
    let storage_key = subxt::dynamic::storage(
        "MultiBlockElection",
        "DesiredTargets",
        vec![subxt::dynamic::Value::u128(round as u128)],
    );
    let desired_targets_entry = storage
        .fetch(&storage_key)
        .await?
        .ok_or("DesiredTargets not found")?;
    let desired_targets: u32 = codec::Decode::decode(&mut desired_targets_entry.encoded())?;
    Ok(desired_targets)
}

