use pallet_election_provider_multi_block::unsigned::miner::MinerConfig;
use sp_core::H256;
use sp_core::crypto::{Ss58Codec};
use sp_core::Get;
use futures::future::join_all;
use tracing::info;

use crate::multi_block_state_client::{BlockDetails, ChainClientTrait, ElectionSnapshotPage, MultiBlockClient, TargetSnapshotPage, VoterData, VoterSnapshotPage};
use crate::primitives::AccountId;
use frame_support::BoundedVec;
use crate::{
    models::{Snapshot, SnapshotNominator, SnapshotValidator, StakingConfig}, 
    raw_state_client::{RpcClient, RawClient}
};

pub async fn build<C: RpcClient, SC: ChainClientTrait, MC: MinerConfig + Send + Sync>(
multi_block_client: &MultiBlockClient<SC, MC>, raw_client: &RawClient<C>, block: Option<H256>) -> Result<Snapshot, Box<dyn std::error::Error>>
where
    MC: MinerConfig<AccountId = AccountId> + Send,
    MC::TargetSnapshotPerBlock: Send,
    MC::VoterSnapshotPerBlock: Send,
    MC::Pages: Send,
    MC::MaxVotesPerVoter: Send,
{
    let block_details = multi_block_client.get_block_details(block).await?;
    let (snapshot, staking_config) = get_snapshot_data_from_multi_block(&multi_block_client, raw_client, &block_details)
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

pub async fn get_snapshot_data_from_multi_block<C: RpcClient, SC: ChainClientTrait, MC: MinerConfig>(
    client: &MultiBlockClient<SC, MC>,
    raw_client: &RawClient<C>,
    block_details: &BlockDetails,
) -> Result<(ElectionSnapshotPage<MC>, StakingConfig), Box<dyn std::error::Error>>
where
    AccountId: Send,
{
    let staking_config = get_staking_config_from_multi_block(client, block_details).await?;
    if block_details.phase.has_snapshot() {
        let mut voters = Vec::new();
        for page in 0..block_details.n_pages {
            let voters_page = client.fetch_paged_voter_snapshot(&block_details.storage, block_details.round, page).await?;
            voters.push(voters_page);
        }

        let target_snapshot = client.fetch_paged_target_snapshot(&block_details.storage, block_details.round, block_details.n_pages - 1).await?;

        return Ok((
            ElectionSnapshotPage::<MC> {
                voters,
                targets: target_snapshot,
            },
            staking_config));
    }
    info!("No snapshot found, getting validators and nominators from staking storage");

    let nominators = raw_client.get_nominators(block_details.block_hash).await?;
    let mut validators = raw_client.get_validators(block_details.block_hash).await?;

    // Prepare data for ElectionSnapshotPage
    let min_nominator_bond = staking_config.min_nominator_bond;

    let nominator_futures: Vec<_> = nominators.into_iter().map(|nominator| {
        let storage = &block_details.storage;
        async move {
            let nominations = client.get_nominator(storage, nominator.clone()).await
                .map_err(|e| e.to_string())?
                .filter(|n| !n.suppressed);
            let nominations = match nominations {
                Some(n) => n,
                None => return Ok::<Option<VoterData<MC>>, String>(None),
            };
            let controller = client.get_controller_from_stash(storage, nominator.clone()).await
                .map_err(|e| e.to_string())?;
            if controller.is_none() {
                return Ok(None);
            }
            let controller = controller.unwrap();
            let stake = client.ledger(storage, controller).await
                .map_err(|e| e.to_string())?
                .filter(|s| s.active >= min_nominator_bond);
            let stake = match stake {
                Some(s) => s,
                None => return Ok(None),
            };
            // Trim targets to max nominations per voter
            let max_nominations = MC::MaxVotesPerVoter::get();
            let mut targets = nominations.targets.clone();
            targets.truncate(max_nominations as usize);
            let targets_mc = BoundedVec::try_from(
                targets.into_iter().map(|t| t.into()).collect::<Vec<AccountId>>()
            ).map_err(|_| "Too many targets in voter".to_string())?;
            Ok(Some((nominator, stake.active as u64, targets_mc)))
        }
    }).collect();
    
    let voters: Vec<VoterData<MC>> = join_all(nominator_futures)
        .await
        .into_iter()
        .filter_map(|result| result.ok().flatten())
        .collect();

    // Filter validators by min validator bond if > 0 requesting for ledger
    let min_validator_bond = staking_config.min_validator_bond;
    
    if min_validator_bond > 0 {
        let storage = &block_details.storage;
        let validators_futures: Vec<_> = validators.into_iter().map(|validator| {
            let client = client;
            async move {
                let controller = client.get_controller_from_stash(storage, validator.clone()).await
                    .map_err(|e| e.to_string())?;
                if controller.is_none() {
                    return Ok(None);
                }
                let controller = controller.unwrap();
                let has_sufficient_bond = client.ledger(storage, controller).await
                    .map_err(|e| e.to_string())?
                    .map_or(false, |l| l.active >= min_validator_bond);
                    Ok::<Option<AccountId>, String>(has_sufficient_bond.then_some(validator))
            }
        }).collect();
        validators = join_all(validators_futures)
            .await
            .into_iter()
            .filter_map(|result| result.ok().flatten())
            .collect();
    }

    // Prepare data for ElectionSnapshotPage
    // divide in pages
    let voters: Vec<VoterSnapshotPage<MC>> = voters
        .chunks(MC::VoterSnapshotPerBlock::get() as usize)
        .map(|chunk| BoundedVec::try_from(chunk.to_vec()).map_err(|_| "Too many voters in chunk"))
        .collect::<Result<Vec<_>, _>>()?;

    let targets = TargetSnapshotPage::<MC>::try_from(
        validators.into_iter().map(|v| v.into()).collect::<Vec<AccountId>>()
    ).map_err(|_| "Too many targets")?;

    let election_snapshot_page = ElectionSnapshotPage::<MC> {
        voters,
        targets,
    };

    Ok((election_snapshot_page, staking_config))
}

pub async fn get_staking_config_from_multi_block<C: crate::multi_block_state_client::ChainClientTrait, MC: MinerConfig>(
    client: &MultiBlockClient<C, MC>,
    block_details: &BlockDetails,
) -> Result<StakingConfig, Box<dyn std::error::Error>> {
    let max_nominations = MC::MaxVotesPerVoter::get();
    let min_nominator_bond = client.get_min_nominator_bond(&block_details.storage).await?;
    let min_validator_bond = client.get_min_validator_bond(&block_details.storage).await?;
    Ok(StakingConfig { desired_validators: block_details.desired_targets, max_nominations, min_nominator_bond, min_validator_bond: min_validator_bond })
}

// Multi-phase snapshot - TODO remove when not neded
// pub async fn get_snapshot_data<C: RpcClient>(client: &RawClient<C>, block: Option<H256>) -> Result<(ElectionSnapshot, StakingConfig), Box<dyn std::error::Error>> {
//     let snapshot = client.get_snapshot(block)
//         .await?;
//     let staking_config = get_staking_config(client, block).await?;
//     if snapshot.is_some() {
//         return Ok((snapshot.unwrap(), staking_config));
//     }
//     info!("No snapshot found, getting validators and nominators from staking storage");
//     // TODO check if nominators include validators self-stake as nominations in snapshot
//     let mut validators = client.get_validators(block).await?;
//     let nominators = client.get_nominators(block).await?;
    
//     let min_bond = staking_config.min_nominator_bond;
    
//     let nominator_futures: Vec<_> = nominators.into_iter().map(|nominator| async move {
//         let nominations = client.get_nominator(nominator.clone(), block).await
//             .map_err(|e| e.to_string())?;
//         if nominations.is_none() {
//             return Ok::<Option<StorageVoterData>, String>(None);
//         }
//         let nominations = nominations.unwrap();
//         if nominations.suppressed {
//             return Ok(None);
//         }
//         let stake = client.ledger(nominator.clone(), block).await
//             .map_err(|e| e.to_string())?;
//         if stake.is_none() {
//             return Ok(None);
//         }
//         let stake = stake.unwrap();
//         let stake_amount = stake.active;
//         if stake_amount < min_bond {
//             return Ok(None);
//         }
//         let targets = nominations.targets.clone();
//         let vote_weight = stake_amount as u64;
//         Ok(Some((nominator, vote_weight, targets)))
//     }).collect();
    
//     let voters = join_all(nominator_futures)
//         .await
//         .into_iter()
//         .collect::<Result<Vec<_>, _>>()
//         .map_err(|e: String| e)?;

//     let voters: Vec<StorageVoterData> = voters.into_iter().filter_map(|x| x).collect();

//     // Filter validators by min validator bond if > 0 requesting for ledger
//     let min_validator_bond = staking_config.min_validator_bond;
    
//     if min_validator_bond > 0 {
//         let validators_futures: Vec<_> = validators.into_iter().map(|validator| async move {
//             let ledger = client.ledger(validator.clone(), block).await
//                 .map_err(|e| e.to_string())?;
//             if ledger.is_none() {
//                 return Ok(None);
//             }
//             let ledger = ledger.unwrap();
//             if ledger.active < min_validator_bond {
//                 return Ok(None);
//             }
//             Ok(Some(validator))
//         }).collect();
//         let collected_validators = join_all(validators_futures)
//             .await
//             .into_iter()
//             .collect::<Result<Vec<_>, _>>()
//             .map_err(|e: String| e)?;
//         validators = collected_validators.into_iter().filter_map(|x| x).collect();
//     }

//     Ok((ElectionSnapshot {
//         voters: voters,
//         targets: validators,
//     }, staking_config))
// }

// pub async fn get_staking_config<C: RpcClient>(client: &RawClient<C>, block: Option<H256>) -> Result<StakingConfig, Box<dyn std::error::Error>> {
//     let desired_validators = client.get_validator_count(block)
//         .await
//         .map_err(|e| format!("Error getting validator count: {}", e))?;
//     let max_nominations = client.get_max_nominations(block)
//         .await
//         .map_err(|e| format!("Error getting max nominations: {}", e))?;
//     let min_nominator_bond = client.get_min_nominator_bond(block)
//         .await
//         .map_err(|e| format!("Error getting min nominator bond: {}", e))?
//         .unwrap_or(0);
//     let min_validator_bond = client.get_min_validator_bond(block)
//         .await
//         .map_err(|e| format!("Error getting min validator bond: {}", e))?
//         .unwrap_or(0);
//     Ok(StakingConfig {
//         desired_validators,
//         max_nominations,
//         min_nominator_bond,
//         min_validator_bond,
//     })
// }