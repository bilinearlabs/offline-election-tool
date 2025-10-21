use sp_core::H256;
use sp_npos_elections::{seq_phragmen, ElectionResult};
use sp_runtime::PerU16;

use crate::{
    primitives::{AccountId},
    storage_client::StorageClient,
};

pub async fn simulate_seq_phragmen(
    client: &StorageClient,
    at: Option<H256>,
) -> Result<ElectionResult<AccountId, PerU16>, Box<dyn std::error::Error>> {
    let desired_targets = client.get_desired_targets(at).await?;
    if desired_targets.is_none() {
        return Err("No desired targets found".into());
    }
    let desired_targets = desired_targets.unwrap();

    let snapshot = client.get_snapshot(at).await?;

    if snapshot.is_none() {
        return Err("No snapshot found".into());
    }
    let snapshot = snapshot.unwrap();

    let election_result = seq_phragmen(
        desired_targets as usize,
        snapshot.targets,
        snapshot.voters,
        None,
    );
    if election_result.is_err() {
        return Err("Election error".into());
    }
    let election_result = election_result.unwrap();

    Ok(election_result)
}