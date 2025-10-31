use sp_core::H256;
use sp_core::crypto::{Ss58Codec};
use futures::future::join_all;

use crate::storage_client::VoterData;
use crate::{
    models::{Snapshot, SnapshotNominator, SnapshotValidator, StakingConfig}, 
    storage_client::{ElectionSnapshot, RpcClient, StorageClient}
};

pub async fn build<C: RpcClient>(client: &StorageClient<C>, block: Option<H256>) -> Result<Snapshot, Box<dyn std::error::Error>> {
    let (snapshot, staking_config) = get_snapshot_data(client, block)
        .await
        .map_err(|e| format!("Error getting snapshot data: {}", e))?;

    let voters = snapshot.voters;
    let targets = snapshot.targets;
    let mut nominators: Vec<SnapshotNominator> = Vec::new();
    
    // Parallelize validator preferences fetching using join_all
    let validator_futures: Vec<_> = targets.into_iter().map(|target| async move {
        let validator_prefs = client.get_validator_prefs(target.clone(), block)
            .await
            .map_err(|e| format!("Error getting validator prefs: {}", e))?;
        
        let validator_prefs = validator_prefs.ok_or("Validator prefs not found")?;
        Ok::<SnapshotValidator, String>(SnapshotValidator {
            stash: target.to_ss58check(),
            commission: validator_prefs.commission.deconstruct() as f64 / 1_000_000_000.0,
            blocked: validator_prefs.blocked,
        })
    }).collect();
    
    let validators: Vec<SnapshotValidator> = join_all(validator_futures)
        .await
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    
    for voter in voters {
        let nominator = SnapshotNominator {
            stash: voter.0.to_ss58check(),
            stake: voter.1 as u128,
            nominations: voter.2.iter().map(|nomination| nomination.clone()).collect(),
        };
        nominators.push(nominator);
    }
    
    Ok(Snapshot { validators, nominators, config: staking_config })
}

pub async fn get_snapshot_data<C: RpcClient>(client: &StorageClient<C>, block: Option<H256>) -> Result<(ElectionSnapshot, StakingConfig), Box<dyn std::error::Error>> {
    let snapshot = client.get_snapshot(block)
        .await?;
    let staking_config = get_staking_config(client, block).await?;
    if snapshot.is_some() {
        return Ok((snapshot.unwrap(), staking_config));
    }
    let mut validators = client.get_validators(block).await?;
    let nominators = client.get_nominators(block).await?;
    
    let min_bond = staking_config.min_nominator_bond;
    
    let nominator_futures: Vec<_> = nominators.into_iter().map(|nominator| async move {
        let nominations = client.get_nominator(nominator.clone(), block).await
            .map_err(|e| e.to_string())?;
        if nominations.is_none() {
            return Ok::<Option<VoterData>, String>(None);
        }
        let nominations = nominations.unwrap();
        if nominations.suppressed {
            return Ok(None);
        }
        let stake = client.ledger(nominator.clone(), block).await
            .map_err(|e| e.to_string())?;
        if stake.is_none() {
            return Ok(None);
        }
        let stake = stake.unwrap();
        let stake_amount = stake.active;
        if stake_amount < min_bond {
            return Ok(None);
        }
        let targets = nominations.targets.clone();
        let vote_weight = stake_amount as u64;
        Ok(Some((nominator, vote_weight, targets)))
    }).collect();
    
    let voters = join_all(nominator_futures)
        .await
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e: String| e)?;

    let voters: Vec<VoterData> = voters.into_iter().filter_map(|x| x).collect();

    // Filter validators by min validator bond if > 0 requesting for ledger
    let min_validator_bond = staking_config.min_validator_bond;
    
    if min_validator_bond > 0 {
        let validators_futures: Vec<_> = validators.into_iter().map(|validator| async move {
            let ledger = client.ledger(validator.clone(), block).await
                .map_err(|e| e.to_string())?;
            if ledger.is_none() {
                return Ok(None);
            }
            let ledger = ledger.unwrap();
            if ledger.active < min_validator_bond {
                return Ok(None);
            }
            Ok(Some(validator))
        }).collect();
        let collected_validators = join_all(validators_futures)
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e: String| e)?;
        validators = collected_validators.into_iter().filter_map(|x| x).collect();
    }

    Ok((ElectionSnapshot {
        voters: voters,
        targets: validators,
    }, staking_config))
}

pub async fn get_staking_config<C: RpcClient>(client: &StorageClient<C>, block: Option<H256>) -> Result<StakingConfig, Box<dyn std::error::Error>> {
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
    Ok(StakingConfig {
        desired_validators,
        max_nominations,
        min_nominator_bond,
        min_validator_bond,
    })
}