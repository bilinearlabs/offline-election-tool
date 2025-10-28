use std::collections::BTreeMap;

use pallet_election_provider_multi_phase::RoundSnapshot;
use serde::Serialize;
use sp_core::{crypto::Ss58Codec, H256};
use sp_npos_elections::{BalancingConfig, ElectionResult, VoteWeight, assignment_ratio_to_staked_normalized, reduce, seq_phragmen, to_support_map};
use frame_support::{BoundedVec, pallet_prelude::ConstU32};
use sp_runtime::{Perbill};

use crate::{
    SimulateArgs, models::{Validator, ValidatorNomination}, primitives::AccountId, storage_client::{RpcClient, StorageClient}
};

#[derive(Debug, Serialize)]
pub struct SimulationResult {
    pub active_validators: Vec<Validator>
}

pub async fn simulate_seq_phragmen<C: RpcClient>(
    client: &StorageClient<C>,
    at: Option<H256>,
    args: SimulateArgs,
) -> Result<SimulationResult, Box<dyn std::error::Error>> {
    let desired_targets = if let Some(count) = args.count {
        count
    } else {
        let desired_targets = client.get_desired_targets(at).await?;
        desired_targets.unwrap_or(50) as usize
    };

    let snapshot = client.get_snapshot(at).await?;

    if snapshot.is_none() {
        return Err("No snapshot found".into());
    }
    let snapshot: RoundSnapshot<AccountId, (AccountId, u64, BoundedVec<AccountId, ConstU32<16>>)> = snapshot.unwrap();
    let voters: Vec<(AccountId, u64, BoundedVec<AccountId, ConstU32<16>>)> = snapshot.voters.clone();
    let targets: Vec<AccountId> = snapshot.targets.clone();

    // Filter voters
    let min_nominator_bond = client.get_min_nominator_bond(at).await?;
    if min_nominator_bond.is_none() {
        return Err("Min nominator bond not found".into());
    }
    let min_nominator_bond = min_nominator_bond.unwrap();
    let filtered_voters: Vec<(AccountId, u64, BoundedVec<AccountId, ConstU32<16>>)> = voters.iter().filter(|voter| voter.1 as u128 >= min_nominator_bond).cloned().collect();

    let max_nominations = client.get_max_nominations(at).await?;
    let max_nominations = max_nominations;
    let filtered_voters: Vec<(AccountId, u64, BoundedVec<AccountId, ConstU32<16>>)> = filtered_voters.into_iter().map(|(who, stake, mut targets)| {
        targets.truncate(max_nominations as usize);
        (who, stake, targets)
    }).collect();

    let election_result = seq_phragmen::<AccountId, Perbill>(
        desired_targets as usize,
        targets,
        filtered_voters.clone(),
        Some(BalancingConfig { iterations: args.iterations, tolerance: 0 }),
    );
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
    
    if args.reduce {
        let reduced_assignments = reduce(staked_assignments.as_mut());
    }

    let supports = to_support_map::<AccountId>( staked_assignments.as_slice());

    let mut active_validators = Vec::new();
    for winner in winners {
        let validator_prefs = client.get_validator_prefs(winner.0.clone(), at).await?;
        if validator_prefs.is_none() {
            return Err("Validator prefs not found".into());
        }

        let validator_prefs = validator_prefs.unwrap();
        let support = supports.get(&winner.0).unwrap();
        let nominations = support.voters.iter().map(|voter| {
            ValidatorNomination {
                nominator: voter.0.to_ss58check(),
                stake: voter.1,
            }
        }).collect();

        // TODO check total stake from ledger
        // let controller = client.get_controller_from_stash(winner.0.clone(), at).await?;
        // if controller.is_none() {
        //     return Err("Controller not found".into());
        // }
        // let controller = controller.unwrap();
        // let ledger = client.ledger(controller.clone(), at).await?;
        // if ledger.is_none() {
        //     println!("Controller: {:?}", account_to_ss58_for_chain(controller.clone(), models::Chain::Polkadot));
        //     return Err("Ledger not found".into());
        // }
        // let ledger = ledger.unwrap();
        // println!("Ledger: {:?}", ledger);
        
        active_validators.push(Validator {
            stash: winner.0.to_ss58check(),
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