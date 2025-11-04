use std::collections::BTreeMap;

use serde::{Serialize, Deserialize};
use sp_core::{crypto::Ss58Codec, H256};
use sp_npos_elections::{BalancingConfig, ElectionResult, VoteWeight, assignment_ratio_to_staked_normalized, reduce, seq_phragmen, phragmms::phragmms, to_support_map};
use frame_support::{BoundedVec, pallet_prelude::ConstU32};
use sp_runtime::{Perbill};
use futures::future::join_all;
use tracing::info;

use crate::{
    models::{Algorithm, Validator, ValidatorNomination}, primitives::AccountId, snapshot, storage_client::{RpcClient, StorageClient}
};

#[derive(Debug, Serialize)]
pub struct SimulationResult {
    pub active_validators: Vec<Validator>
}

#[derive(Debug, Deserialize)]
struct Override {
    voters: Vec<(String, u64, Vec<String>)>,
    voters_remove: Vec<String>,
    candidates: Vec<String>,
    candidates_remove: Vec<String>,
}

pub async fn simulate<C: RpcClient>(
    client: &StorageClient<C>,
    at: Option<H256>,
    targets_count: Option<usize>,
    algorithm: Algorithm,
    iterations: usize,
    apply_reduce: bool,
    manual_override: Option<String>,
) -> Result<SimulationResult, Box<dyn std::error::Error>> {
    let (snapshot, stake_config) = snapshot::get_snapshot_data(client, at)
        .await
        .map_err(|e| format!("Error getting snapshot data: {}", e))?;
    let mut voters: Vec<(AccountId, u64, BoundedVec<AccountId, ConstU32<16>>)> = snapshot.voters.clone();
    let mut targets: Vec<AccountId> = snapshot.targets.clone();

    // Apply manual override if provided
    if let Some(path) = manual_override {
        let file = std::fs::read(&path)
            .map_err(|e| format!("Failed to read manual override file '{}': {}", path, e))?;
        let manual: Override = serde_json::from_slice(&file)
            .map_err(|e| format!("Failed to parse manual override JSON: {}", e))?;

        // Add any additional candidates
        for c in &manual.candidates {
            let candidate_id = AccountId::from_ss58check(c)?;
            if targets.contains(&candidate_id) {
                info!("manual override: {:?} is already a candidate.", c);
            } else {
                info!("manual override: {:?} is added as candidate.", c);
                targets.push(candidate_id);
            }
        }

        // Remove candidates in the removal list
        let candidates_to_remove: Vec<AccountId> = manual.candidates_remove
            .iter()
            .map(|c| AccountId::from_ss58check(c))
            .collect::<Result<_, _>>()?;
        targets.retain(|c| !candidates_to_remove.contains(c));

        // Add or override voters
        for v in &manual.voters {
            let voter_id = AccountId::from_ss58check(&v.0)?;
            let stake = v.1;
            let votes: Vec<AccountId> = v.2.iter()
                .map(|vote| AccountId::from_ss58check(vote))
                .collect::<Result<_, _>>()?;
            let bounded_votes: BoundedVec<AccountId, ConstU32<16>> = votes.try_into()
                .map_err(|_| "Too many nominations (max 16)")?;

            if let Some(existing_voter) = voters.iter_mut().find(|vv| vv.0 == voter_id) {
                info!("manual override: {:?} is already a voter. Overriding votes.", v.0);
                existing_voter.1 = stake;
                existing_voter.2 = bounded_votes;
            } else {
                info!("manual override: {:?} is added as voter.", v.0);
                voters.push((voter_id, stake, bounded_votes));
            }
        }

        // Remove voters in the removal list
        let voters_to_remove: Vec<AccountId> = manual.voters_remove
            .iter()
            .map(|v| AccountId::from_ss58check(v))
            .collect::<Result<_, _>>()?;
        voters.retain(|v| !voters_to_remove.contains(&v.0));
    }

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
        reduce(staked_assignments.as_mut());
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