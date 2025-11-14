use std::collections::BTreeMap;

use frame_election_provider_support::BoundedSupport;
use pallet_staking::ValidatorPrefs;
use serde::{Serialize, Deserialize};
use sp_core::{crypto::Ss58Codec, Get, H256};
use sp_npos_elections::Support;
use pallet_election_provider_multi_block::{
    PagedRawSolution, unsigned::miner::{BaseMiner, MineInput}, verifier::feasibility_check_page_inner_with_snapshot
};
use pallet_election_provider_multi_block::unsigned::miner::MinerConfig;
use futures::future::join_all;
use sp_runtime::Perbill;
use tracing::info;
use frame_support::BoundedVec;
use crate::multi_block_state_client::{VoterData, VoterSnapshotPage};

use crate::{
    models::{Validator, ValidatorNomination}, multi_block_state_client::{ChainClientTrait, MultiBlockClient}, primitives::AccountId, raw_state_client::{RawClient, RpcClient}, snapshot
};

#[derive(Debug, Serialize)]
pub struct SimulationResult {
    pub active_validators: Vec<Validator>
}

#[derive(Debug, Deserialize, Clone)]
pub struct Override {
    pub voters: Vec<(String, u64, Vec<String>)>,
    pub voters_remove: Vec<String>,
    pub candidates: Vec<String>,
    pub candidates_remove: Vec<String>,
}

pub async fn simulate<C: RpcClient, SC: ChainClientTrait, MC: MinerConfig>(
    raw_state_client: &RawClient<C>,
    multi_block_state_client: &MultiBlockClient<SC, MC>,
    at: Option<H256>,
    desired_validators: Option<u32>,
    apply_reduce: bool,
    manual_override: Option<Override>,
    min_nominator_bond: Option<u128>,
    min_validator_bond: Option<u128>,
) -> Result<SimulationResult, Box<dyn std::error::Error>>
where
    MC: MinerConfig + 'static,
    MC: MinerConfig<AccountId = AccountId> + Send,
    <MC as MinerConfig>::TargetSnapshotPerBlock: Send,
    <MC as MinerConfig>::VoterSnapshotPerBlock: Send,
    <MC as MinerConfig>::Pages: Send,
    <MC as MinerConfig>::MaxVotesPerVoter: Send,
    <MC as MinerConfig>::Solution: Send,
{
    let block_details = multi_block_state_client.get_block_details(at).await?;
    info!("Fetching snapshot data for election...");
    let (mut snapshot, staking_config) = snapshot::get_snapshot_data_from_multi_block(multi_block_state_client, raw_state_client, &block_details).await?;

    // Apply min_nominator_bond filter if provided > 0
    let effective_min_nominator_bond = min_nominator_bond.unwrap_or(0);
    if effective_min_nominator_bond > 0 {
        info!("Filtering voters by min_nominator_bond: {}", effective_min_nominator_bond);
        let mut filtered_voter_pages = Vec::new();
        for voter_page in snapshot.voters.iter() {
            let filtered_page: Vec<_> = voter_page.iter()
                .filter(|voter| voter.1 as u128 >= effective_min_nominator_bond)
                .cloned()
                .collect();
            if !filtered_page.is_empty() {
                let bounded_page = BoundedVec::try_from(filtered_page)
                    .map_err(|_| "Failed to create bounded voter page")?;
                filtered_voter_pages.push(bounded_page);
            }
        }
        snapshot.voters = filtered_voter_pages.try_into()
            .map_err(|_| "Failed to create AllVoterPagesOf")?;
    }
    
    // Apply min_validator_bond filter if provided > 0
    let effective_min_validator_bond = min_validator_bond.unwrap_or(0);
    if effective_min_validator_bond > 0 {
        info!("Filtering validators by min_validator_bond: {}", effective_min_validator_bond);
        let validator_futures: Vec<_> = snapshot.targets.iter().map(|validator| {
            let validator = validator.clone();
            let storage = block_details.storage.clone();
            async move {
                let controller = multi_block_state_client.get_controller_from_stash(&storage, validator.clone()).await
                    .map_err(|e| format!("Error getting controller: {}", e))?;
                if controller.is_none() {
                    return Ok::<Option<AccountId>, String>(None);
                }
                let controller = controller.unwrap();
                let ledger = multi_block_state_client.ledger(&storage, controller).await
                    .map_err(|e| format!("Error getting ledger: {}", e))?;
                let has_sufficient_bond = ledger.map_or(false, |l| l.active >= effective_min_validator_bond);
                Ok(has_sufficient_bond.then_some(validator))
            }
        }).collect();
        
        let filtered_validators: Vec<_> = join_all(validator_futures)
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Error filtering validators: {}", e))?;
        
        let filtered_validators: Vec<_> = filtered_validators.into_iter().filter_map(|x| x).collect();
        snapshot.targets = BoundedVec::try_from(filtered_validators)
            .map_err(|_| "Failed to create bounded target page")?;
    }
    
    // Manual override
    if let Some(manual) = manual_override {
        // Convert targets to Vec for manipulation
        let mut targets: Vec<AccountId> = snapshot.targets.iter().cloned().collect();

        // Add any additional candidates
        for c in &manual.candidates {
            let candidate_id: AccountId = AccountId::from_ss58check(c)?;
            if targets.contains(&candidate_id) {
                info!("manual override: {:?} is already a candidate.", c);
            } else {
                info!("manual override: {:?} is added as candidate.", c);
                targets.push(candidate_id);
            }
        }

        // Remove candidates in the removal list
        for c in &manual.candidates_remove {
            let candidate_id: AccountId = AccountId::from_ss58check(c)?;
            if targets.contains(&candidate_id) {
                info!("manual override: {:?} is removed as candidate.", c);
                targets.retain(|x| x != &candidate_id);
            }
        }

        // Convert back to BoundedVec
        snapshot.targets = BoundedVec::try_from(targets)
            .map_err(|_| "Failed to create bounded target page")?;

        // Collect all voters from pages into a flat Vec for manipulation
        let mut all_voters: Vec<VoterData<MC>> = Vec::new();
        for voter_page in snapshot.voters.iter() {
            for voter in voter_page.iter() {
                all_voters.push(voter.clone());
            }
        }

        // Add or override voters
        for v in &manual.voters {
            let voter_id: AccountId = AccountId::from_ss58check(&v.0)?;
            let stake = v.1;
            let votes: Vec<AccountId> = v.2.iter()
                .map(|vote| AccountId::from_ss58check(vote).map(|id| id.into()))
                .collect::<Result<_, _>>()?;
            let bounded_votes = BoundedVec::try_from(votes)
                .map_err(|_| "Too many nominations")?;

            let voter_data: VoterData<MC> = (voter_id.clone(), stake, bounded_votes);
            if let Some(existing_voter) = all_voters.iter_mut().find(|vv| vv.0 == voter_data.0) {
                info!("manual override: {:?} is already a voter. Overriding votes.", v.0);
                *existing_voter = voter_data;
            } else {
                info!("manual override: {:?} is added as voter.", v.0);
                all_voters.push(voter_data);
            }
        }

        // Remove voters in the removal list
        for v in &manual.voters_remove {
            let voter_id: AccountId = AccountId::from_ss58check(v)?;
            if all_voters.iter().any(|vv| vv.0 == voter_id) {
                info!("manual override: {:?} is removed as voter.", v);
                all_voters.retain(|vv| vv.0 != voter_id);
            }
        }

        // Repage voters back into AllVoterPagesOf
        let voters_vec: Vec<BoundedVec<VoterData<MC>, MC::VoterSnapshotPerBlock>> = all_voters
            .chunks(MC::VoterSnapshotPerBlock::get() as usize)
            .map(|chunk| BoundedVec::try_from(chunk.to_vec()).map_err(|_| "Too many voters in chunk"))
            .collect::<Result<Vec<_>, _>>()?;
        snapshot.voters = voters_vec.try_into()
            .map_err(|_| "Failed to create AllVoterPagesOf")?;
    }

    let desired_targets = if let Some(desired_validators) = desired_validators {
        desired_validators
    } else {
        staking_config.desired_validators
    };

    let voter_pages: BoundedVec<VoterSnapshotPage<MC>, MC::Pages> = BoundedVec::truncate_from(snapshot.voters);

    // Use actual voter pages for mining solution when snapshot is not available and is created from staking
    let actual_voter_pages = voter_pages.len() as u32;
    
    let mine_input = MineInput {
		desired_targets: desired_targets,
		all_targets: snapshot.targets.clone(),
		voter_pages: voter_pages.clone(),
		pages: actual_voter_pages,
		do_reduce: apply_reduce,
		round: block_details.round,
	};
    info!("Mining solution for election...");
    
    let paged_solution = tokio::task::spawn_blocking(move || -> Result<PagedRawSolution<MC>, String> {
        let solution = BaseMiner::<MC>::mine_solution(mine_input)
            .map_err(|e| format!("Error mining solution: {:?}", e))?;
        Ok(solution) 
    }).await.unwrap()?;

    // Convert each solution page to supports and combine them
    let mut total_supports: BTreeMap<AccountId, Support<AccountId>> = BTreeMap::new();

    let paged_supports = BaseMiner::<MC>::check_feasibility(
        &paged_solution, &voter_pages, &snapshot.targets, desired_targets)
        .map_err(|e| format!("Error checking feasibility: {:?}", e))?;

    for page in paged_supports.iter() {
        for (winner, support) in page.iter() {
            let entry = total_supports.entry(winner.clone()).or_insert_with(|| Support {
                total: 0,
                voters: Vec::new(),
            });
            entry.total = entry.total.saturating_add(support.total);
            entry.voters.extend(support.voters.clone().into_iter());
        }
    }

    let validator_futures: Vec<_> = total_supports.into_iter().map(|(winner, support)| {
        let storage = block_details.storage.clone();
        async move {
            let validator_prefs = multi_block_state_client.get_validator_prefs(&storage, winner.clone()).await
                .unwrap_or(ValidatorPrefs {
                    commission: Perbill::from_parts(0),
                    blocked: false,
                });

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