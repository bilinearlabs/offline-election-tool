use std::collections::HashMap;

use sp_core::H256;
use sp_core::crypto::{Ss58Codec};

use crate::{
    error::AppError,
    models::{Chain, Nominator, NominatorStake, Snapshot, SnapshotNominator, SnapshotValidator, StakingConfig, Validator, ValidatorNomination}, primitives::AccountId, storage_client::{RpcClient, StorageClient}
};

// Build snapshot of current validators and their nominators
// pub async fn build<C: RpcClient>(client: &StorageClient<C>, chain: Chain, block: Option<H256>) -> Result<Snapshot, Box<dyn std::error::Error>> {
//     let complete_exposure = client.get_all_validators_complete_exposure(block).await?;
//     let (_era, validators) = complete_exposure;

//     let mut nominators_map: HashMap<String, Nominator> = HashMap::new();
//     let mut validators_vec: Vec<Validator> = Vec::new();

//     for (validator, complete_exposure_data) in validators {
//         let prefs = client.get_validator_prefs(validator.clone(), block).await?;
//         if prefs.is_none() {
//             continue;
//         }
//         let prefs = prefs.unwrap();

//         let validator_stash_ss58 = account_to_ss58_for_chain(&validator.clone(), chain);

//         let validator_struct = Validator {
//             stash: validator_stash_ss58.clone(),
//             self_stake: complete_exposure_data.own,
//             total_stake: complete_exposure_data.total,
//             commission: prefs.commission.deconstruct() as f64 / 1_000_000_000.0,
//             blocked: prefs.blocked,
//             nominations: complete_exposure_data.others
//                 .iter()
//                 .map(|nominator| ValidatorNomination {
//                     nominator: account_to_ss58_for_chain(&nominator.who, chain),
//                     stake: nominator.value,
//                 })
//                 .collect(),
//         };
//         validators_vec.push(validator_struct);

//         for nominator in complete_exposure_data.others.clone() {
//             let nominator_stake = NominatorStake {
//                 validator: validator_stash_ss58.clone(),
//                 stake: nominator.value,
//             };

//             let nominator_stash_ss58 = account_to_ss58_for_chain(&nominator.who, chain);
//             let found = nominators_map.get_mut(&nominator_stash_ss58);
//             if found.is_some() {
//                 let found = found.unwrap();
//                 found.active_stakes.push(nominator_stake);
//             } else {
//                 nominators_map.insert(nominator_stash_ss58.clone(), Nominator {
//                     stash: nominator_stash_ss58.clone(),
//                     active_stakes: vec![nominator_stake],
//                 });
//             }
//         }
//     }

//     let nominators: Vec<Nominator> = nominators_map.into_values().collect();
//     let staking_config = StakingConfig {
//         desired_validators: client.get_validator_count(block).await?,
//         max_nominations: client.get_max_nominations(block).await?,
//         min_nominator_bond: client.get_min_nominator_bond(block).await?.unwrap_or(0),
//         min_validator_bond: client.get_min_validator_bond(block).await?.unwrap_or(0),
//     };
//     Ok(SnapshotExposure { validators: validators_vec, nominators, config: staking_config })
// }

pub async fn build<C: RpcClient>(client: &StorageClient<C>, block: Option<H256>) -> Result<Snapshot, Box<dyn std::error::Error>> {
    let snapshot = client.get_snapshot(block)
        .await?
        .ok_or_else(|| AppError::NotFound("No snapshot found for the specified block".to_string()))?;
    let voters = snapshot.voters;
    let targets = snapshot.targets;

    let mut validators: Vec<SnapshotValidator> = Vec::new();
    let mut nominators: Vec<SnapshotNominator> = Vec::new();
    for target in targets {
        let validator_prefs = client.get_validator_prefs(target.clone(), block)
            .await
            .map_err(|e| format!("Error getting validator prefs: {}", e))?;
        
        let validator_prefs = validator_prefs.ok_or("Validator prefs not found")?;
        validators.push(SnapshotValidator {
            stash: target.to_ss58check(),
            commission: validator_prefs.commission.deconstruct() as f64 / 1_000_000_000.0,
            blocked: validator_prefs.blocked,
        });
    }
    for voter in voters {
        let nominator = SnapshotNominator {
            stash: voter.0.to_ss58check(),
            stake: voter.1 as u128,
            nominations: voter.2.iter().map(|nomination| nomination.clone()).collect(),
        };
        nominators.push(nominator);
    }

    // Await all calls first, then construct struct to avoid holding errors across awaits
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
    
    let staking_config = StakingConfig {
        desired_validators,
        max_nominations,
        min_nominator_bond,
        min_validator_bond,
    };
    Ok(Snapshot { validators, nominators, config: staking_config })
}

