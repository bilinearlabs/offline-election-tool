use std::collections::BTreeMap;

use pallet_election_provider_multi_phase::RoundSnapshot;
use serde::Serialize;
use sp_core::{ConstU32, H256};
use sp_npos_elections::{assignment_ratio_to_staked_normalized, seq_phragmen, to_support_map, VoteWeight, ElectionResult};
use sp_runtime::{BoundedVec, PerU16};

use crate::{
    models::{self, account_to_ss58_for_chain, Validator, ValidatorNomination}, primitives::AccountId, storage_client::{RpcClient, StorageClient}
};

#[derive(Debug, Serialize)]
pub struct SimulationResult {
    pub active_validators: Vec<Validator>
}

pub async fn simulate_seq_phragmen<C: RpcClient>(
    client: &StorageClient<C>,
    at: Option<H256>,
) -> Result<SimulationResult, Box<dyn std::error::Error>> {
    let desired_targets = client.get_desired_targets(at).await?;
    if desired_targets.is_none() {
        return Err("No desired targets found".into());
    }
    let desired_targets = desired_targets.unwrap();

    let snapshot = client.get_snapshot(at).await?;

    if snapshot.is_none() {
        return Err("No snapshot found".into());
    }
    let snapshot: RoundSnapshot<AccountId, (AccountId, u64, BoundedVec<AccountId, ConstU32<16>>)> = snapshot.unwrap();
    let voters: Vec<(sp_runtime::AccountId32, u64, BoundedVec<sp_runtime::AccountId32, ConstU32<16>>)> = snapshot.voters.clone();
    println!("Voters: {:?}", snapshot.voters.len());

    // Filter voters
    let min_nominator_bond = client.get_min_nominator_bond(at).await?;
    if min_nominator_bond.is_none() {
        return Err("Min nominator bond not found".into());
    }
    let min_nominator_bond = min_nominator_bond.unwrap();
    println!("Min nominator bond: {:?}", min_nominator_bond);
    let filtered_voters: Vec<(sp_runtime::AccountId32, u64, BoundedVec<sp_runtime::AccountId32, ConstU32<16>>)> = voters.iter().filter(|voter| voter.1 as u128 >= min_nominator_bond).cloned().collect();

    let max_nominations = client.get_max_nominations(at).await?;
    let max_nominations = max_nominations;
    println!("Max nominations: {:?}", max_nominations);
    let filtered_voters: Vec<(sp_runtime::AccountId32, u64, BoundedVec<sp_runtime::AccountId32, ConstU32<16>>)> = filtered_voters.into_iter().map(|(who, stake, mut targets)| {
        targets.truncate(max_nominations as usize);
        (who, stake, targets)
    }).collect();

    println!("Filtered voters: {:?}", filtered_voters.len());

    let election_result = seq_phragmen(
        desired_targets as usize,
        snapshot.targets,
        filtered_voters.clone(),
        None,
    );
    if election_result.is_err() {
        return Err("Election error".into());
    }
    let election_result: ElectionResult<AccountId, PerU16> = election_result.unwrap();

    let assignments = election_result.assignments.clone();
    let winners = election_result.winners.clone();

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
    let staked_assignments = staked_assignments.unwrap();
    
    let supports = to_support_map::<AccountId>( staked_assignments.as_slice());

    let mut active_validators = Vec::new();
    for winner in winners {
        let validator_prefs = client.get_validator_prefs(winner.0.clone(), at).await?;
        if validator_prefs.is_none() {
            return Err("Validator prefs not found".into());
        }

        let active_era = client.get_active_era(at).await?;
        if active_era.is_none() {
            return Err("Active era not found".into());
        }
        let active_era = active_era.unwrap();
        // TODO Check if can be retrieved from other source as if validator is waiting returns None
        let validator_exposure = client.get_complete_validator_exposure(active_era.index, winner.0.clone(), at).await?;
        if validator_exposure.is_none() {
            println!("Validator own not found for validator: {:?}", winner.0);
            return Err("Validator own not found".into());
        }
        let validator_exposure = validator_exposure.unwrap();
        let validator_self_stake = validator_exposure.own;
        let validator_prefs = validator_prefs.unwrap();
        let support = supports.get(&winner.0).unwrap();
        let nominations = support.voters.iter().map(|voter| {
            ValidatorNomination {
                nominator: account_to_ss58_for_chain(voter.0.clone(), models::Chain::Polkadot),
                stake: voter.1,
            }
        }).collect();
        
        active_validators.push(Validator {
            stash: account_to_ss58_for_chain(winner.0, models::Chain::Polkadot),
            self_stake: validator_self_stake,
            total_stake: support.total,
            commission: validator_prefs.commission.deconstruct() as f64 / 1_000_000_000.0,
            blocked: validator_prefs.blocked,
            nominations
        });
    }

    let simulation_result = SimulationResult {
        active_validators
    };
    Ok(simulation_result)
}