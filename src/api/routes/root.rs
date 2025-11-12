use std::sync::Arc;
use crate::{models::Chain, multi_block_state_client::MultiBlockClient, primitives::AccountId, raw_state_client::{RawClient}, subxt_client::Client};
use jsonrpsee_ws_client::WsClient;
use axum::{
    Router,
    routing::{IntoMakeService, get, post},
};
use pallet_election_provider_multi_block::unsigned::miner::MinerConfig;
use tower_http::trace::TraceLayer;

use crate::api::handler::{simulate, snapshot};

pub struct AppState<T: MinerConfig + Send + Sync + Clone> 
where
    T: MinerConfig<AccountId = AccountId> + Send,
    <T as MinerConfig>::TargetSnapshotPerBlock: Send,
    <T as MinerConfig>::VoterSnapshotPerBlock: Send,
    <T as MinerConfig>::Pages: Send,
    <T as MinerConfig>::MaxVotesPerVoter: Send,
{
    pub raw_state_client: Arc<RawClient<WsClient>>,
    pub multi_block_state_client: Arc<MultiBlockClient<Client, T>>,
    pub chain: Chain,
}

impl<T: MinerConfig + Send + Sync + Clone> Clone for AppState<T>
where
    T: MinerConfig<AccountId = AccountId> + Send,
    <T as MinerConfig>::TargetSnapshotPerBlock: Send,
    <T as MinerConfig>::VoterSnapshotPerBlock: Send,
    <T as MinerConfig>::Pages: Send,
    <T as MinerConfig>::MaxVotesPerVoter: Send,
{
    fn clone(&self) -> Self {
        Self {
            raw_state_client: self.raw_state_client.clone(),
            multi_block_state_client: self.multi_block_state_client.clone(),
            chain: self.chain.clone(),
        }
    }
}

pub fn routes<T: MinerConfig + Send + Sync + Clone + 'static>(raw_state_client: Arc<RawClient<WsClient>>, multi_block_state_client: Arc<MultiBlockClient<Client, T>>, chain: Chain) -> IntoMakeService<Router>
where
    T: MinerConfig<AccountId = AccountId> + Send,
    T::TargetSnapshotPerBlock: Send + 'static,
    T::VoterSnapshotPerBlock: Send + 'static,
    T::Pages: Send + 'static,
    T::MaxVotesPerVoter: Send + 'static,
    T::Solution: Send + 'static,
{
    let app_state = AppState {
        raw_state_client,
        multi_block_state_client,
        chain,
    };
    
    let app_router = Router::new()
        .route("/simulate", post(simulate::simulate_handler::<T>))
        .route("/snapshot", get(snapshot::snapshot_handler::<T>))
        .with_state(app_state)
        .layer(TraceLayer::new_for_http());
    app_router.into_make_service()
}