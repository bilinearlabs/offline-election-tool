use pallet_election_provider_multi_block::unsigned::miner::MinerConfig;
use sp_core::H256;
use sp_core::crypto::{Ss58Codec};
use futures::future::join_all;
use tracing::info;

use crate::multi_block_storage_client::{BlockDetails, ChainClientTrait, ElectionSnapshotPage, MultiBlockClient, VoterSnapshotPage};
use crate::storage_client::VoterData;
use crate::{
    models::{Snapshot, SnapshotNominator, SnapshotValidator, StakingConfig}, 
    storage_client::{ElectionSnapshot, RpcClient, StorageClient}
};

pub async fn build<C: ChainClientTrait, MC: MinerConfig + Send + Sync>(multi_block_client: &MultiBlockClient<C, MC>, block: Option<H256>) -> Result<Snapshot, Box<dyn std::error::Error>>
where
    <MC as MinerConfig>::AccountId: Ss58Codec,
    MC::TargetSnapshotPerBlock: Send,
    MC::VoterSnapshotPerBlock: Send,
    MC::Pages: Send,
    MC::MaxVotesPerVoter: Send,
{
    let block_details = multi_block_client.get_block_details(block).await?;
    let (snapshot, staking_config) = get_snapshot_data_from_multi_block(&multi_block_client, &block_details)
        .await
        .map_err(|e| format!("Error getting snapshot data: {}", e))?;

    let voters = snapshot.voters;
    let targets = snapshot.targets;
    
    let storage = &block_details.storage;
    
    let validator_futures: Vec<_> = targets.into_iter().map(|target| {
        async move {
            let validator_prefs = multi_block_client.get_validator_prefs(storage, target.clone())
                .await
                .map_err(|e| format!("Error getting validator prefs: {}", e))?;
            
            Ok::<SnapshotValidator, String>(SnapshotValidator {
                stash: target.to_ss58check(),
                commission: validator_prefs.commission.deconstruct() as f64 / 1_000_000_000.0,
                blocked: validator_prefs.blocked,
            })
        }
    }).collect();
    
    let validators: Vec<SnapshotValidator> = join_all(validator_futures)
        .await
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    
    let mut nominators: Vec<SnapshotNominator> = Vec::new();
    for voter_page in voters {
        for voter in voter_page {
            let nominator = SnapshotNominator {
                stash: voter.0.to_ss58check(),
                stake: voter.1 as u128,
                nominations: voter.2.iter().map(|nomination| nomination.to_ss58check()).collect(),
            };
            nominators.push(nominator);
        }
    }
    
    Ok(Snapshot { validators, nominators, config: staking_config })
}

pub async fn get_snapshot_data_from_multi_block<C: crate::multi_block_storage_client::ChainClientTrait, T: MinerConfig>(
    client: &MultiBlockClient<C, T>,
    block_details: &BlockDetails,
) -> Result<(ElectionSnapshotPage<T>, StakingConfig), Box<dyn std::error::Error>> {
    if block_details.phase.has_snapshot() {
        let mut voters: Vec<VoterSnapshotPage<T>> = Vec::new();
        for page in 0..block_details.n_pages {
            let voters_page = client.fetch_paged_voter_snapshot(&block_details.storage, block_details.round, page).await?;
            voters.push(voters_page);
        }

        let target_snapshot = client.fetch_paged_target_snapshot(&block_details.storage, block_details.round, block_details.n_pages - 1).await?;

        let staking_config = get_staking_config_from_multi_block(client, block_details).await?;
        return Ok((
            ElectionSnapshotPage::<T> {
                voters: voters,
                targets: target_snapshot,
            },
            staking_config));
    }
    info!("No snapshot found, getting validators and nominators from staking storage");
    // TODO get validators and nominators from staking storage
    Err("No snapshot found".into())
}

pub async fn get_staking_config_from_multi_block<C: crate::multi_block_storage_client::ChainClientTrait, T: MinerConfig>(
    client: &MultiBlockClient<C, T>,
    block_details: &BlockDetails,
) -> Result<StakingConfig, Box<dyn std::error::Error>> {
    let max_nominations = client.get_max_nominations().await?;
    let min_nominator_bond = client.get_min_nominator_bond(&block_details.storage).await?;
    let min_validator_bond = client.get_min_validator_bond(&block_details.storage).await?;
    Ok(StakingConfig { desired_validators: block_details.desired_targets, max_nominations, min_nominator_bond, min_validator_bond: min_validator_bond })
}

// Multi-phase snapshot
pub async fn get_snapshot_data<C: RpcClient>(client: &StorageClient<C>, block: Option<H256>) -> Result<(ElectionSnapshot, StakingConfig), Box<dyn std::error::Error>> {
    let snapshot = client.get_snapshot(block)
        .await?;
    let staking_config = get_staking_config(client, block).await?;
    if snapshot.is_some() {
        return Ok((snapshot.unwrap(), staking_config));
    }
    info!("No snapshot found, getting validators and nominators from staking storage");
    // TODO check if nominators include validators self-stake as nominations in snapshot
    let mut validators = client.get_validators(block).await?;
    let nominators = client.get_nominators(block).await?;
    
    let min_bond = staking_config.min_nominator_bond;
    
    let nominator_futures: Vec<_> = nominators.into_iter().map(|nominator| async move {
        let nominations = client.get_nominator(nominator.clone(), block).await
            .map_err(|e| e.to_string())?;
        if nominations.is_none() {
            return Ok::<Option<VoterData>, String>(None);
        }
        let nominations = nominations.unwrap();
        if nominations.suppressed {
            return Ok(None);
        }
        let stake = client.ledger(nominator.clone(), block).await
            .map_err(|e| e.to_string())?;
        if stake.is_none() {
            return Ok(None);
        }
        let stake = stake.unwrap();
        let stake_amount = stake.active;
        if stake_amount < min_bond {
            return Ok(None);
        }
        let targets = nominations.targets.clone();
        let vote_weight = stake_amount as u64;
        Ok(Some((nominator, vote_weight, targets)))
    }).collect();
    
    let voters = join_all(nominator_futures)
        .await
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e: String| e)?;

    let voters: Vec<VoterData> = voters.into_iter().filter_map(|x| x).collect();

    // Filter validators by min validator bond if > 0 requesting for ledger
    let min_validator_bond = staking_config.min_validator_bond;
    
    if min_validator_bond > 0 {
        let validators_futures: Vec<_> = validators.into_iter().map(|validator| async move {
            let ledger = client.ledger(validator.clone(), block).await
                .map_err(|e| e.to_string())?;
            if ledger.is_none() {
                return Ok(None);
            }
            let ledger = ledger.unwrap();
            if ledger.active < min_validator_bond {
                return Ok(None);
            }
            Ok(Some(validator))
        }).collect();
        let collected_validators = join_all(validators_futures)
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e: String| e)?;
        validators = collected_validators.into_iter().filter_map(|x| x).collect();
    }

    Ok((ElectionSnapshot {
        voters: voters,
        targets: validators,
    }, staking_config))
}

pub async fn get_staking_config<C: RpcClient>(client: &StorageClient<C>, block: Option<H256>) -> Result<StakingConfig, Box<dyn std::error::Error>> {
    let desired_validators = client.get_validator_count(block)
        .await
        .map_err(|e| format!("Error getting validator count: {}", e))?;
    let max_nominations = client.get_max_nominations(block)
        .await
        .map_err(|e| format!("Error getting max nominations: {}", e))?;
    let min_nominator_bond = client.get_min_nominator_bond(block)
        .await
        .map_err(|e| format!("Error getting min nominator bond: {}", e))?
        .unwrap_or(0);
    let min_validator_bond = client.get_min_validator_bond(block)
        .await
        .map_err(|e| format!("Error getting min validator bond: {}", e))?
        .unwrap_or(0);
    Ok(StakingConfig {
        desired_validators,
        max_nominations,
        min_nominator_bond,
        min_validator_bond,
    })
}