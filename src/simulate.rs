use std::collections::BTreeMap;

use serde::Serialize;
use sp_core::{crypto::Ss58Codec, H256};
use sp_npos_elections::{BalancingConfig, ElectionResult, VoteWeight, assignment_ratio_to_staked_normalized, reduce, seq_phragmen, to_support_map};
use frame_support::{BoundedVec, pallet_prelude::ConstU32};
use sp_runtime::{Perbill};
use futures::future::join_all;

use crate::{
    models::{Validator, ValidatorNomination}, primitives::AccountId, snapshot, storage_client::{RpcClient, StorageClient}
};

#[derive(Debug, Serialize)]
pub struct SimulationResult {
    pub active_validators: Vec<Validator>
}

pub async fn simulate_seq_phragmen<C: RpcClient>(
    client: &StorageClient<C>,
    at: Option<H256>,
    _targets_count: Option<usize>,
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
    let filtered_voters: Vec<(AccountId, u64, BoundedVec<AccountId, ConstU32<16>>)> = voters.iter().filter(|voter| voter.1 as u128 >= min_nominator_bond).cloned().collect();

    let election_result = seq_phragmen::<AccountId, Perbill>(
        stake_config.desired_validators as usize,
        targets,
        filtered_voters.clone(),
        Some(BalancingConfig { iterations: iterations, tolerance: 0 }),
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
    
    if apply_reduce {
        let _reduced_assignments = reduce(staked_assignments.as_mut());
    }

    let supports = to_support_map::<AccountId>( staked_assignments.as_slice());

    let validator_futures: Vec<_> = winners.into_iter().map(|winner| {
        let support = supports.get(&winner.0).ok_or("Support not found").cloned();
        async move {
            let validator_prefs = client.get_validator_prefs(winner.0.clone(), at).await
                .map_err(|e| format!("Error getting validator prefs: {}", e))?;
            
            let validator_prefs = validator_prefs.ok_or("Validator prefs not found")?;
            let support = support?;
            
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
            
            Ok::<Validator, String>(Validator {
                stash: winner.0.to_ss58check(),
                total_stake: support.total,
                commission: validator_prefs.commission.deconstruct() as f64 / 1_000_000_000.0,
                blocked: validator_prefs.blocked,
                nominations
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