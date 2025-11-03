use std::collections::BTreeMap;

use serde::Serialize;
use sp_core::{crypto::Ss58Codec, H256};
use sp_npos_elections::{BalancingConfig, ElectionResult, VoteWeight, assignment_ratio_to_staked_normalized, reduce, seq_phragmen, phragmms::phragmms, to_support_map};
use frame_support::{BoundedVec, pallet_prelude::ConstU32};
use sp_runtime::{Perbill};
use futures::future::join_all;

use crate::{
    models::{Algorithm, Validator, ValidatorNomination}, primitives::AccountId, snapshot, storage_client::{RpcClient, StorageClient}
};

#[derive(Debug, Serialize)]
pub struct SimulationResult {
    pub active_validators: Vec<Validator>
}

pub async fn simulate<C: RpcClient>(
    client: &StorageClient<C>,
    at: Option<H256>,
    targets_count: Option<usize>,
    algorithm: Algorithm,
    iterations: usize,
    apply_reduce: bool,
) -> Result<SimulationResult, Box<dyn std::error::Error>> {
    let (snapshot, stake_config) = snapshot::get_snapshot_data(client, at)
        .await
        .map_err(|e| format!("Error getting snapshot data: {}", e))?;
    let voters: Vec<(AccountId, u64, BoundedVec<AccountId, ConstU32<16>>)> = snapshot.voters.clone();
    let targets: Vec<AccountId> = snapshot.targets.clone();

    // Filter voters
    let min_nominator_bond = stake_config.min_nominator_bond;
    let filtered_voters: Vec<(AccountId, u64, BoundedVec<AccountId, ConstU32<16>>)> = voters.iter()
        .filter(|voter| voter.1 as u128 >= min_nominator_bond)
        .cloned()
        .collect();

    let desired_validators = if targets_count.is_some() {
        targets_count.unwrap()
    } else {
        stake_config.desired_validators as usize
    };

    let balancing_config = if iterations > 0 {
        Some(BalancingConfig { iterations: iterations, tolerance: 0 })
    } else {
        None
    };

    // Run the selected algorithm
    let election_result = match algorithm {
        Algorithm::SeqPhragmen => seq_phragmen::<AccountId, Perbill>(
            desired_validators,
            targets,
            filtered_voters.clone(),
            balancing_config,
        ),
        Algorithm::Phragmms => phragmms::<AccountId, Perbill>(
            desired_validators,
            targets,
            filtered_voters.clone(),
            balancing_config,
        ),
    };
    
    if election_result.is_err() {
        return Err("Election error".into());
    }

    let ElectionResult { winners, assignments } = election_result.unwrap();

    // Store voter weight in a map to use it in the assignment_ratio_to_staked function
    let mut voter_weight: BTreeMap<AccountId, VoteWeight> = BTreeMap::new();

	for (voter, budget, _) in filtered_voters.clone().iter() {
		voter_weight.insert(voter.clone(), *budget);
	}

	let weight_of = |who: &AccountId| -> VoteWeight { *voter_weight.get(who).unwrap() };

    let staked_assignments = assignment_ratio_to_staked_normalized(assignments, weight_of);
    if staked_assignments.is_err() {
        return Err("Error in assignment_ratio_to_staked_normalized".into());
    }
    let mut staked_assignments = staked_assignments.unwrap();
    
    if apply_reduce {
        if apply_reduce {
            let before_count: usize = staked_assignments.iter().map(|a| a.distribution.len()).sum();
            reduce(staked_assignments.as_mut());
            let after_count: usize = staked_assignments.iter().map(|a| a.distribution.len()).sum();
            println!("🔪 Reduce: {} edges → {} edges (removed {})", 
                     before_count, after_count, before_count - after_count);
        }
    }

    let supports = to_support_map::<AccountId>( staked_assignments.as_slice());

    let validator_futures: Vec<_> = winners.into_iter().map(|winner| {
        let support = supports.get(&winner.0).ok_or("Support not found").cloned();
        async move {
            let validator_prefs = client.get_validator_prefs(winner.0.clone(), at).await
                .map_err(|e| format!("Error getting validator prefs: {}", e))?;
            
            let validator_prefs = validator_prefs.ok_or("Validator prefs not found")?;
            let support = support?;

            let self_stake = support.voters.iter()
                .find(|voter| voter.0 == winner.0)
                .unwrap_or(&(winner.0.clone(), 0))
                .1;
            
            let nominations: Vec<ValidatorNomination> = support.voters.iter()
                .filter(|voter| voter.0 != winner.0)
                .map(|voter| {
                ValidatorNomination {
                    nominator: voter.0.to_ss58check(),
                    stake: voter.1,
                }
            }).collect();

            Ok::<Validator, String>(Validator {
                stash: winner.0.to_ss58check(),
                self_stake: self_stake,
                total_stake: support.total,
                commission: validator_prefs.commission.deconstruct() as f64 / 1_000_000_000.0,
                blocked: validator_prefs.blocked,
                nominations_count: nominations.len(),
                nominations: nominations,
            })
        }
    }).collect();
    
    let active_validators: Vec<Validator> = join_all(validator_futures)
        .await
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    let simulation_result = SimulationResult {
        active_validators
    };

    Ok(simulation_result)
}