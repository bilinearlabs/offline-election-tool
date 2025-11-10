use std::collections::BTreeMap;

use serde::{Serialize, Deserialize};
use sp_core::{crypto::Ss58Codec, H256};
use sp_npos_elections::Support;
use pallet_election_provider_multi_block::{
    unsigned::miner::{BaseMiner, MineInput},
    verifier::feasibility_check_page_inner_with_snapshot,
};
use pallet_election_provider_multi_block::unsigned::miner::MinerConfig;
use futures::future::join_all;
use tracing::info;

use crate::{
    models::{Algorithm, Validator, ValidatorNomination}, multi_block_state_client::{ChainClientTrait, MultiBlockClient}, snapshot, raw_state_client::{RpcClient, RawClient}
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

pub async fn simulate<C: RpcClient, SC: ChainClientTrait, MC: MinerConfig>(
    _client: &RawClient<C>,
    multi_block_client: &MultiBlockClient<SC, MC>,
    at: Option<H256>,
    _targets_count: Option<usize>,
    _algorithm: Algorithm,
    apply_reduce: bool,
    _manual_override: Option<String>,
) -> Result<SimulationResult, Box<dyn std::error::Error>>
where
    <MC as MinerConfig>::AccountId: Ss58Codec + From<crate::primitives::AccountId>,
    <MC as MinerConfig>::TargetSnapshotPerBlock: Send,
    <MC as MinerConfig>::VoterSnapshotPerBlock: Send,
    <MC as MinerConfig>::Pages: Send,
    <MC as MinerConfig>::MaxVotesPerVoter: Send,
{
    info!("Simulating election...");
    let block_details = multi_block_client.get_block_details(at).await?;

    // TODO remove when multi-block is implemented
    // let (snapshot, stake_config) = snapshot::get_snapshot_data(client, at)
    //     .await
    //     .map_err(|e| format!("Error getting snapshot data: {}", e))?;
    // let mut voters: Vec<(AccountId, u64, BoundedVec<AccountId, ConstU32<16>>)> = snapshot.voters.clone();
    // let mut targets: Vec<AccountId> = snapshot.targets.clone();

    // // Apply manual override if provided
    // if let Some(path) = manual_override {
    //     let file = std::fs::read(&path)
    //         .map_err(|e| format!("Failed to read manual override file '{}': {}", path, e))?;
    //     let manual: Override = serde_json::from_slice(&file)
    //         .map_err(|e| format!("Failed to parse manual override JSON: {}", e))?;

    //     // Add any additional candidates
    //     for c in &manual.candidates {
    //         let candidate_id = AccountId::from_ss58check(c)?;
    //         if targets.contains(&candidate_id) {
    //             info!("manual override: {:?} is already a candidate.", c);
    //         } else {
    //             info!("manual override: {:?} is added as candidate.", c);
    //             targets.push(candidate_id);
    //         }
    //     }

    //     // Remove candidates in the removal list
    //     let candidates_to_remove: Vec<AccountId> = manual.candidates_remove
    //         .iter()
    //         .map(|c| AccountId::from_ss58check(c))
    //         .collect::<Result<_, _>>()?;
    //     targets.retain(|c| !candidates_to_remove.contains(c));

    //     // Add or override voters
    //     for v in &manual.voters {
    //         let voter_id = AccountId::from_ss58check(&v.0)?;
    //         let stake = v.1;
    //         let votes: Vec<AccountId> = v.2.iter()
    //             .map(|vote| AccountId::from_ss58check(vote))
    //             .collect::<Result<_, _>>()?;
    //         let bounded_votes: BoundedVec<AccountId, ConstU32<16>> = votes.try_into()
    //             .map_err(|_| "Too many nominations (max 16)")?;

    //         if let Some(existing_voter) = voters.iter_mut().find(|vv| vv.0 == voter_id) {
    //             info!("manual override: {:?} is already a voter. Overriding votes.", v.0);
    //             existing_voter.1 = stake;
    //             existing_voter.2 = bounded_votes;
    //         } else {
    //             info!("manual override: {:?} is added as voter.", v.0);
    //             voters.push((voter_id, stake, bounded_votes));
    //         }
    //     }

    //     // Remove voters in the removal list
    //     let voters_to_remove: Vec<AccountId> = manual.voters_remove
    //         .iter()
    //         .map(|v| AccountId::from_ss58check(v))
    //         .collect::<Result<_, _>>()?;
    //     voters.retain(|v| !voters_to_remove.contains(&v.0));
    // }

    // // Filter voters
    // let min_nominator_bond = stake_config.min_nominator_bond;
    // let filtered_voters: Vec<(AccountId, u64, BoundedVec<AccountId, ConstU32<16>>)> = voters.iter()
    //     .filter(|voter| voter.1 as u128 >= min_nominator_bond)
    //     .cloned()
    //     .collect();

    // let desired_validators = if targets_count.is_some() {
    //     targets_count.unwrap()
    // } else {
    //     stake_config.desired_validators as usize
    // };

    // let iterations = get_balancing_iterations();
    // let balancing_config = if iterations > 0 {
    //     Some(BalancingConfig { iterations: iterations, tolerance: 0 })
    // } else {
    //     None
    // };


    let (snapshot, staking_config) = snapshot::get_snapshot_data_from_multi_block(multi_block_client, _client, &block_details).await?;
    // TODO new simulate from multi_block
    let mine_input = MineInput {
		desired_targets: staking_config.desired_validators,
		all_targets: snapshot.targets.clone(),
		voter_pages: snapshot.voters.clone(),
		pages: block_details.n_pages,
		do_reduce: apply_reduce,
		round: block_details.round,
	};
    let paged_solution = BaseMiner::<MC>::mine_solution(mine_input)
        .map_err(|e| format!("Error mining solution: {:?}", e))?;
    
    // Convert each solution page to supports and combine them
    let mut total_supports: BTreeMap<<MC as MinerConfig>::AccountId, Support<<MC as MinerConfig>::AccountId>> = BTreeMap::new();
    
    for (page_index, solution_page) in paged_solution.solution_pages.iter().enumerate() {
        let voter_page = snapshot.voters.get(page_index)
            .ok_or(format!("Voter page {} not found", page_index))?;
        
        // Convert solution page to supports
        let page_supports = feasibility_check_page_inner_with_snapshot::<MC>(
            solution_page.clone(),
            voter_page,
            &snapshot.targets,
            staking_config.desired_validators,
        ).map_err(|e| format!("Error converting solution page {} to supports: {:?}", page_index, e))?;
        
        // Combine supports from this page into total supports
        for (winner, support) in page_supports.into_iter() {
            let entry = total_supports.entry(winner.clone()).or_insert_with(|| Support {
                total: 0,
                voters: Vec::new(),
            });
            entry.total = entry.total.saturating_add(support.total);
            for (voter, stake) in support.voters {
                if let Some(existing) = entry.voters.iter_mut().find(|(v, _)| *v == voter) {
                    existing.1 = existing.1.max(stake);
                    tracing::warn!("Voter {} appears multiple times for validator {}", voter.to_ss58check(), winner.to_ss58check());
                } else {
                    entry.voters.push((voter, stake));
                }
            }
        }
    }
    
    // Extract winners from supports
    let winners: Vec<(<MC as MinerConfig>::AccountId, Support<<MC as MinerConfig>::AccountId>)> = 
        total_supports.into_iter().collect();

    // // Run the selected algorithm
    // let election_result = match algorithm {
    //     Algorithm::SeqPhragmen => seq_phragmen::<AccountId, Perbill>(
    //         desired_validators,
    //         targets,
    //         filtered_voters.clone(),
    //         balancing_config,
    //     ),
    //     Algorithm::Phragmms => phragmms::<AccountId, Perbill>(
    //         desired_validators,
    //         targets,
    //         filtered_voters.clone(),
    //         balancing_config,
    //     ),
    // };
    
    // if election_result.is_err() {
    //     return Err("Election error".into());
    // }

    // let ElectionResult { winners, assignments } = election_result.unwrap();

    // // Store voter weight in a map to use it in the assignment_ratio_to_staked function
    // let mut voter_weight: BTreeMap<AccountId, VoteWeight> = BTreeMap::new();

	// for (voter, budget, _) in filtered_voters.clone().iter() {
	// 	voter_weight.insert(voter.clone(), *budget);
	// }

	// let weight_of = |who: &AccountId| -> VoteWeight { *voter_weight.get(who).unwrap() };

    // let staked_assignments = assignment_ratio_to_staked_normalized(assignments, weight_of);
    // if staked_assignments.is_err() {
    //     return Err("Error in assignment_ratio_to_staked_normalized".into());
    // }
    // let mut staked_assignments = staked_assignments.unwrap();
    
    // if apply_reduce {
    //     reduce(staked_assignments.as_mut());
    // }

    // let supports = to_support_map::<AccountId>( staked_assignments.as_slice());

    let validator_futures: Vec<_> = winners.into_iter().map(|(winner, support)| {
        let storage = block_details.storage.clone();
        async move {
            let validator_prefs = multi_block_client.get_validator_prefs(&storage, winner.clone()).await
                .map_err(|e| format!("Error getting validator prefs: {}", e))?;

            let self_stake = support.voters.iter()
                .find(|voter| voter.0 == winner)
                .unwrap_or(&(winner.clone(), 0))
                .1;
            
            let nominations: Vec<ValidatorNomination> = support.voters.iter()
                .filter(|voter| voter.0 != winner)
                .map(|voter| {
                ValidatorNomination {
                    nominator: voter.0.to_ss58check(),
                    stake: voter.1,
                }
            }).collect();

            Ok::<Validator, String>(Validator {
                stash: winner.to_ss58check(),
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