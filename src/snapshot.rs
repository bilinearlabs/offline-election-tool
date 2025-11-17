use pallet_election_provider_multi_block::unsigned::miner::MinerConfig;
use sp_core::H256;
use sp_core::crypto::{Ss58Codec};
use sp_core::Get;
use futures::future::join_all;
use tracing::info;

use crate::multi_block_state_client::{BlockDetails, ChainClientTrait, ElectionSnapshotPage, MultiBlockClient, MultiBlockClientTrait, StorageTrait, TargetSnapshotPage, VoterData, VoterSnapshotPage};
use crate::primitives::{AccountId, Storage};
use frame_support::BoundedVec;
use crate::{
    models::{Snapshot, SnapshotNominator, SnapshotValidator, StakingConfig}, 
    raw_state_client::{RpcClient, RawClient}
};

pub async fn build<C: RpcClient, SC: ChainClientTrait, MC: MinerConfig + Send + Sync>(
multi_block_client: &MultiBlockClient<SC, MC>, raw_client: &RawClient<C>, block: Option<H256>) -> Result<Snapshot, Box<dyn std::error::Error>>
where
    MC: MinerConfig<AccountId = AccountId> + Send,
    MC::TargetSnapshotPerBlock: Send,
    MC::VoterSnapshotPerBlock: Send,
    MC::Pages: Send,
    MC::MaxVotesPerVoter: Send,
{
    let block_details = multi_block_client.get_block_details(block).await?;
    let (snapshot, staking_config) = get_snapshot_data_from_multi_block(&multi_block_client, raw_client, &block_details)
        .await
        .map_err(|e| format!("Error getting snapshot data: {}", e))?;

    let voters = snapshot.voters;
    let targets = snapshot.targets;
    
    let storage = &block_details.storage;
    
    let validator_futures: Vec<_> = targets.into_iter().map(|target| {
        async move {
            let validator_prefs = multi_block_client.get_validator_prefs(storage, target.clone())
                .await
                .map_err(|e| format!("Error getting validator prefs: {}", e))?;
            
            Ok::<SnapshotValidator, String>(SnapshotValidator {
                stash: target.to_ss58check(),
                commission: validator_prefs.commission.deconstruct() as f64 / 1_000_000_000.0,
                blocked: validator_prefs.blocked,
            })
        }
    }).collect();
    
    let validators: Vec<SnapshotValidator> = join_all(validator_futures)
        .await
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    
    let mut nominators: Vec<SnapshotNominator> = Vec::new();
    for voter_page in voters {
        for voter in voter_page {
            let nominator = SnapshotNominator {
                stash: voter.0.to_ss58check(),
                stake: voter.1 as u128,
                nominations: voter.2.iter().map(|nomination| nomination.to_ss58check()).collect(),
            };
            nominators.push(nominator);
        }
    }
    
    Ok(Snapshot { validators, nominators, config: staking_config })
}

pub async fn get_snapshot_data_from_multi_block<C: RpcClient, SC: ChainClientTrait, MC: MinerConfig>(
    client: &MultiBlockClient<SC, MC>,
    raw_client: &RawClient<C>,
    block_details: &BlockDetails<Storage>,
) -> Result<(ElectionSnapshotPage<MC>, StakingConfig), Box<dyn std::error::Error>>
where
    AccountId: Send,
    MC: Send + Sync + 'static,
{
    let staking_config = get_staking_config_from_multi_block(client, block_details).await?;
    if block_details.phase.has_snapshot() {
        let mut voters = Vec::new();
        for page in 0..block_details.n_pages {
            let voters_page = client.fetch_paged_voter_snapshot(&block_details.storage, block_details.round, page).await?;
            voters.push(voters_page);
        }

        let target_snapshot = client.fetch_paged_target_snapshot(&block_details.storage, block_details.round, block_details.n_pages - 1).await?;

        return Ok((
            ElectionSnapshotPage::<MC> {
                voters,
                targets: target_snapshot,
            },
            staking_config));
    }
    info!("No snapshot found, getting validators and nominators from staking storage");

    let nominators = raw_client.get_nominators(block_details.block_hash).await?;
    let mut validators = raw_client.get_validators(block_details.block_hash).await?;

    // Prepare data for ElectionSnapshotPage
    let min_nominator_bond = staking_config.min_nominator_bond;

    let nominator_futures: Vec<_> = nominators.into_iter().map(|nominator| {
        let storage = &block_details.storage;
        async move {
            let nominations = client.get_nominator(storage, nominator.clone()).await
                .map_err(|e| e.to_string())?
                .filter(|n| !n.suppressed);
            let nominations = match nominations {
                Some(n) => n,
                None => return Ok::<Option<VoterData<MC>>, String>(None),
            };
            let controller = client.get_controller_from_stash(storage, nominator.clone()).await
                .map_err(|e| e.to_string())?;
            if controller.is_none() {
                return Ok(None);
            }
            let controller = controller.unwrap();
            let stake = client.ledger(storage, controller).await
                .map_err(|e| e.to_string())?
                .filter(|s| s.active >= min_nominator_bond);
            let stake = match stake {
                Some(s) => s,
                None => return Ok(None),
            };
            // Trim targets to max nominations per voter
            let max_nominations = MC::MaxVotesPerVoter::get();
            let mut targets = nominations.targets.clone();
            targets.truncate(max_nominations as usize);
            let targets_mc = BoundedVec::try_from(
                targets.into_iter().map(|t| t.into()).collect::<Vec<AccountId>>()
            ).map_err(|_| "Too many targets in voter".to_string())?;
            Ok(Some((nominator, stake.active as u64, targets_mc)))
        }
    }).collect();
    
    let voters: Vec<VoterData<MC>> = join_all(nominator_futures)
        .await
        .into_iter()
        .filter_map(|result| result.ok().flatten())
        .collect();

    // Filter validators by min validator bond if > 0 requesting for ledger
    let min_validator_bond = staking_config.min_validator_bond;
    
    if min_validator_bond > 0 {
        let storage = &block_details.storage;
        let validators_futures: Vec<_> = validators.into_iter().map(|validator| {
            let client = client;
            async move {
                let controller = client.get_controller_from_stash(storage, validator.clone()).await
                    .map_err(|e| e.to_string())?;
                if controller.is_none() {
                    return Ok(None);
                }
                let controller = controller.unwrap();
                let has_sufficient_bond = client.ledger(storage, controller).await
                    .map_err(|e| e.to_string())?
                    .map_or(false, |l| l.active >= min_validator_bond);
                    Ok::<Option<AccountId>, String>(has_sufficient_bond.then_some(validator))
            }
        }).collect();
        validators = join_all(validators_futures)
            .await
            .into_iter()
            .filter_map(|result| result.ok().flatten())
            .collect();
    }

    // Prepare data for ElectionSnapshotPage
    // divide in pages
    let voters: Vec<VoterSnapshotPage<MC>> = voters
        .chunks(MC::VoterSnapshotPerBlock::get() as usize)
        .map(|chunk| BoundedVec::try_from(chunk.to_vec()).map_err(|_| "Too many voters in chunk"))
        .collect::<Result<Vec<_>, _>>()?;

    let targets = TargetSnapshotPage::<MC>::try_from(
        validators.into_iter().map(|v| v.into()).collect::<Vec<AccountId>>()
    ).map_err(|_| "Too many targets")?;

    let election_snapshot_page = ElectionSnapshotPage::<MC> {
        voters,
        targets,
    };

    Ok((election_snapshot_page, staking_config))
}

pub async fn get_staking_config_from_multi_block<
    C: crate::multi_block_state_client::ChainClientTrait + Send + Sync + 'static, 
    MC: MinerConfig + Send + Sync + 'static, 
    MBC: MultiBlockClientTrait<C, MC> + Send + Sync + 'static,
    S: StorageTrait + 'static>(
    client: &MBC,
    block_details: &BlockDetails<S>,
) -> Result<StakingConfig, Box<dyn std::error::Error>>
where
    MC: Send + Sync + 'static,
{
    let max_nominations = MC::MaxVotesPerVoter::get();
    let min_nominator_bond = client.get_min_nominator_bond(&block_details.storage).await?;
    let min_validator_bond = client.get_min_validator_bond(&block_details.storage).await?;
    Ok(StakingConfig { desired_validators: block_details.desired_targets, max_nominations, min_nominator_bond, min_validator_bond: min_validator_bond })
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockall::mock;
    use pallet_staking::ValidatorPrefs;
    use crate::miner_config::polkadot::MinerConfig as PolkadotMinerConfig;
    use crate::miner_config::kusama::MinerConfig as KusamaMinerConfig;
    use crate::miner_config::substrate::MinerConfig as SubstrateMinerConfig;
    use crate::models::Chain;
    use crate::multi_block_state_client::{StorageTrait, Phase};
    use crate::primitives::{Hash, Storage};
    use crate::raw_state_client::{NominationsLight, StakingLedger};
    use crate::miner_config::{MinerConstants, set_runtime_constants};

    mock! {
        pub MultiBlockClient<C: ChainClientTrait + Send + Sync + 'static, MC: MinerConfig + Send + Sync + 'static> {}
    
        #[async_trait::async_trait]
        impl<C: ChainClientTrait + Send + Sync + 'static, MC: MinerConfig + Send + Sync + 'static> MultiBlockClientTrait<C, MC> for MultiBlockClient<C, MC> {
            async fn get_storage<S: StorageTrait + From<Storage> + 'static>(&self, block: Option<Hash>) -> Result<S, Box<dyn std::error::Error>>;
            async fn get_block_details<S: StorageTrait + From<Storage> + 'static>(&self, block: Option<Hash>) -> Result<BlockDetails<S>, Box<dyn std::error::Error>>;
            async fn get_phase<S: StorageTrait + 'static>(&self, storage: &S) -> Result<Phase, Box<dyn std::error::Error>>;
            async fn get_round<S: StorageTrait + 'static>(&self, storage: &S) -> Result<u32, Box<dyn std::error::Error>>;
            async fn get_desired_targets<S: StorageTrait + 'static>(&self, storage: &S, round: u32) -> Result<u32, Box<dyn std::error::Error>>;
            async fn get_block_number<S: StorageTrait + 'static>(&self, storage: &S) -> Result<u32, Box<dyn std::error::Error>>;
            async fn get_min_nominator_bond<S: StorageTrait + 'static>(&self, storage: &S) -> Result<u128, Box<dyn std::error::Error>> where S: Send + Sync + 'static;
            async fn get_min_validator_bond<S: StorageTrait + 'static>(&self, storage: &S) -> Result<u128, Box<dyn std::error::Error>>;
            async fn fetch_paged_voter_snapshot<S: StorageTrait + 'static>(&self, storage: &S, round: u32, page: u32) -> Result<VoterSnapshotPage<MC>, Box<dyn std::error::Error>> where S: Send + Sync + 'static;
            async fn fetch_paged_target_snapshot<S: StorageTrait + 'static>(&self, storage: &S, round: u32, page: u32) -> Result<TargetSnapshotPage<MC>, Box<dyn std::error::Error>>;
            async fn get_validator_prefs<S: StorageTrait + 'static>(&self, storage: &S, validator: AccountId) -> Result<ValidatorPrefs, Box<dyn std::error::Error>>;
            async fn get_nominator<S: StorageTrait + 'static>(&self, storage: &S, nominator: AccountId) -> Result<Option<NominationsLight<AccountId>>, Box<dyn std::error::Error>>;
            async fn get_controller_from_stash<S: StorageTrait + 'static>(&self, storage: &S, stash: AccountId) -> Result<Option<AccountId>, Box<dyn std::error::Error>>;
            async fn ledger<S: StorageTrait + 'static>(&self, storage: &S, account: AccountId) -> Result<Option<StakingLedger>, Box<dyn std::error::Error>>;
        }
    }
    use subxt::utils::Yes;
    use subxt::storage::Address;
    mock! {
        #[derive(Debug, Clone)]
        pub DummyStorage {}
        
        #[async_trait::async_trait]
        impl StorageTrait for DummyStorage {
            async fn fetch<Addr>(
                &self,
                address: &Addr,
            ) -> Result<Option<<Addr as Address>::Target>, Box<dyn std::error::Error>>
            where
                Addr: Address<IsFetchable = Yes> + Sync + 'static;

            async fn fetch_or_default<Addr>(
                &self,
                address: &Addr,
            ) -> Result<<Addr as Address>::Target, Box<dyn std::error::Error>>
            where
                Addr: Address<IsFetchable = Yes, IsDefaultable = Yes> + Sync + 'static;
        }
    }

    mock! {
        #[derive(Debug, Clone)]
        pub DummyChainClient {} 

        #[async_trait::async_trait]
        impl ChainClientTrait for DummyChainClient {
            async fn get_storage(&self, block: Option<Hash>) -> Result<Storage, Box<dyn std::error::Error>>;

            async fn fetch_constant<T: serde::de::DeserializeOwned>(
                &self,
                pallet: &str,
                constant_name: &str,
            ) -> Result<T, Box<dyn std::error::Error>>
            where
                T: 'static;
        }
    }


    #[tokio::test]
    async fn test_get_staking_config() {
        let mut mock_client = MockMultiBlockClient::<MockDummyChainClient, PolkadotMinerConfig>::new();

        mock_client
            .expect_get_min_nominator_bond()
            .returning(|_storage: &MockDummyStorage| Ok(100));

        mock_client
            .expect_get_min_validator_bond()
            .returning(|_storage: &MockDummyStorage| Ok(200));

        let result = get_staking_config_from_multi_block(&mock_client, &BlockDetails::<MockDummyStorage> {
            block_hash: Some(Hash::zero()),
            phase: Phase::Snapshot(0),
            round: 1,
            n_pages: 1,
            desired_targets: 10,
            storage: MockDummyStorage::new(),
            _block_number: 100,
        }).await;

        assert!(result.is_ok());
        let config = result.unwrap();
        assert_eq!(config.min_nominator_bond, 100);
        assert_eq!(config.min_validator_bond, 200);
        assert_eq!(config.desired_validators, 10);
        assert_eq!(config.max_nominations, 16);
    }


}