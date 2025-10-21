use std::collections::HashMap;

use sp_core::H256;

use crate::{
    models::{account_to_ss58_for_chain, Chain, Nominator, NominatorStake, Snapshot, StakingConfig, Validator, ValidatorNomination}, storage_client::StorageClient
};

pub async fn build(client: &StorageClient, chain: Chain, block: Option<H256>) -> Result<Snapshot, Box<dyn std::error::Error>> {
    let complete_exposure = client.get_all_validators_complete_exposure(block).await?;
    let (_era, validators) = complete_exposure;

    let mut nominators_map: HashMap<String, Nominator> = HashMap::new();
    let mut validators_vec: Vec<Validator> = Vec::new();

    for (validator, complete_exposure_data) in validators {
        let prefs = client.get_validator_prefs(validator.clone(), block).await?;
        if prefs.is_none() {
            continue;
        }
        let prefs = prefs.unwrap();

        let stash_account = client.get_stash_for_controller(validator.clone(), block).await?;
        if stash_account.is_none() {
            continue;
        }
        let stash = stash_account.unwrap();
        let validator_stash_ss58 = account_to_ss58_for_chain(&stash, chain);

        let validator_struct = Validator {
            stash: validator_stash_ss58.clone(),
            self_stake: complete_exposure_data.own,
            total_stake: complete_exposure_data.total,
            commission: prefs.commission.deconstruct() as f64 / 1_000_000_000.0,
            blocked: prefs.blocked,
            nominations: complete_exposure_data.others
                .iter()
                .map(|nominator| ValidatorNomination {
                    nominator: account_to_ss58_for_chain(&nominator.who, chain),
                    stake: nominator.value,
                })
                .collect(),
        };
        validators_vec.push(validator_struct);

        for nominator in complete_exposure_data.others.clone() {
            let nominator_stake = NominatorStake {
                validator: validator_stash_ss58.clone(),
                stake: nominator.value,
            };

            let nominator_stash_ss58 = account_to_ss58_for_chain(&nominator.who, chain);
            let found = nominators_map.get_mut(&nominator_stash_ss58);
            if found.is_some() {
                let found = found.unwrap();
                found.active_stakes.push(nominator_stake);
            } else {
                nominators_map.insert(nominator_stash_ss58.clone(), Nominator {
                    stash: nominator_stash_ss58.clone(),
                    active_stakes: vec![nominator_stake],
                });
            }
        }
    }

    let nominators: Vec<Nominator> = nominators_map.into_values().collect();
    let staking_config = StakingConfig {
        desired_validators: client.get_validator_count(block).await?,
        max_nominations: client.get_max_nominations(block).await?,
        min_nominator_bond: client.get_min_nominator_bond(block).await?.unwrap_or(0),
        min_validator_bond: client.get_min_validator_bond(block).await?.unwrap_or(0),
    };
    Ok(Snapshot { validators: validators_vec, nominators, config: staking_config })
}


