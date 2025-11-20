use std::{marker::PhantomData, sync::Arc};
use mockall::automock;
use sp_core::H256;
use crate::{
    multi_block_state_client::MultiBlockClient,
    primitives::{AccountId, Storage},
    raw_state_client::{RawClient, RawClientTrait, RpcClient},
    simulate::{self, Override, SimulationResult},
    snapshot,
    subxt_client::Client,
};
use crate::models::Snapshot;
use jsonrpsee_ws_client::WsClient;
use pallet_election_provider_multi_block::unsigned::miner::MinerConfig;

#[automock]
pub trait SimulateService: Send + Sync {
    fn simulate(
        &self,
        block: Option<H256>,
        desired_validators: Option<u32>,
        apply_reduce: bool,
        manual_override: Option<Override>,
        min_nominator_bond: Option<u128>,
        min_validator_bond: Option<u128>,
    ) -> impl std::future::Future<Output = Result<SimulationResult, Box<dyn std::error::Error>>> + std::marker::Send;
}

#[automock]
pub trait SnapshotService: Send + Sync {
    fn build(
        &self,
        block: Option<H256>,
    ) -> impl std::future::Future<Output = Result<Snapshot, Box<dyn std::error::Error>>> + std::marker::Send;
}

pub struct SimulateServiceImpl<T: MinerConfig + Send + Sync + Clone + 'static>
where
    T: MinerConfig<AccountId = AccountId> + Send,
    T::TargetSnapshotPerBlock: Send,
    T::VoterSnapshotPerBlock: Send,
    T::Pages: Send,
    T::MaxVotesPerVoter: Send,
    T::Solution: Send,
    T::MaxBackersPerWinner: Send,
    T::MaxWinnersPerPage: Send,
{
    pub raw_state_client: Arc<RawClient<WsClient>>,
    pub multi_block_state_client: Arc<MultiBlockClient<Client, T, Storage>>,
}

pub struct SnapshotServiceImpl<T: MinerConfig + Send + Sync + Clone + 'static,
>
where
    T: MinerConfig<AccountId = AccountId> + Send,
    T::TargetSnapshotPerBlock: Send,
    T::VoterSnapshotPerBlock: Send,
    T::Pages: Send,
    T::MaxVotesPerVoter: Send,
{
    pub raw_state_client: Arc<RawClient<WsClient>>,
    pub multi_block_state_client: Arc<MultiBlockClient<Client, T, Storage>>,
}

#[automock]
impl<T: MinerConfig + Send + Sync + Clone + 'static> SimulateService for SimulateServiceImpl<T>
where
    T: MinerConfig<AccountId = AccountId> + Send,
    T::TargetSnapshotPerBlock: Send,
    T::VoterSnapshotPerBlock: Send,
    T::Pages: Send,
    T::MaxVotesPerVoter: Send,
    T::Solution: Send,
    T::MaxWinnersPerPage: Send,
    T::TargetSnapshotPerBlock: Send,
    T::MaxBackersPerWinner: Send,
{
    fn simulate(
        &self,
        block: Option<H256>,
        desired_validators: Option<u32>,
        apply_reduce: bool,
        manual_override: Option<Override>,
        min_nominator_bond: Option<u128>,
        min_validator_bond: Option<u128>,
    ) -> impl std::future::Future<Output = Result<SimulationResult, Box<dyn std::error::Error>>> + std::marker::Send + std::marker::Send {
        simulate::simulate(
            self.multi_block_state_client.as_ref(),
            self.raw_state_client.as_ref(),
            block,
            desired_validators,
            apply_reduce,
            manual_override,
            min_nominator_bond,
            min_validator_bond,
        )
    }
}

#[automock]
impl<T: MinerConfig + Send + Sync + Clone + 'static> SnapshotService for SnapshotServiceImpl<T>
where
    T: MinerConfig<AccountId = AccountId> + Send,
    T::TargetSnapshotPerBlock: Send,
    T::VoterSnapshotPerBlock: Send,
    T::Pages: Send,
    T::MaxVotesPerVoter: Send,
{
    async fn build(
        &self,
        block: Option<H256>,
    ) -> Result<Snapshot, Box<dyn std::error::Error>> {
        snapshot::build(
            self.multi_block_state_client.as_ref(),
            self.raw_state_client.as_ref(),
            block,
        )
        .await
    }
}
