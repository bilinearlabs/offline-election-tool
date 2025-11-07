use std::sync::Arc;
use crate::{multi_block_storage_client::MultiBlockClient, storage_client::StorageClient, subxt_client::Client, models::Chain, primitives::AccountId};
use jsonrpsee_ws_client::WsClient;
use axum::{
    Router,
    routing::{IntoMakeService, get, post},
};
use pallet_election_provider_multi_block::unsigned::miner::MinerConfig;
use sp_core::crypto::Ss58Codec;
use tower_http::trace::TraceLayer;

use crate::api::handler::{simulate, snapshot};

pub struct AppState<T: MinerConfig + Send + Sync + Clone> 
where
    <T as MinerConfig>::AccountId: Ss58Codec + Send,
    <T as MinerConfig>::TargetSnapshotPerBlock: Send,
    <T as MinerConfig>::VoterSnapshotPerBlock: Send,
    <T as MinerConfig>::Pages: Send,
    <T as MinerConfig>::MaxVotesPerVoter: Send,
{
    pub storage_client: Arc<StorageClient<WsClient>>,
    pub multi_block_storage_client: Arc<MultiBlockClient<Client, T>>,
    pub chain: Chain,
}

impl<T: MinerConfig + Send + Sync + Clone> Clone for AppState<T>
where
    <T as MinerConfig>::AccountId: Ss58Codec + Send,
    <T as MinerConfig>::TargetSnapshotPerBlock: Send,
    <T as MinerConfig>::VoterSnapshotPerBlock: Send,
    <T as MinerConfig>::Pages: Send,
    <T as MinerConfig>::MaxVotesPerVoter: Send,
{
    fn clone(&self) -> Self {
        Self {
            storage_client: self.storage_client.clone(),
            multi_block_storage_client: self.multi_block_storage_client.clone(),
            chain: self.chain.clone(),
        }
    }
}

pub fn routes<T: MinerConfig + Send + Sync + Clone + 'static>(storage_client: Arc<StorageClient<WsClient>>, multi_block_storage_client: Arc<MultiBlockClient<Client, T>>, chain: Chain) -> IntoMakeService<Router>
where
    T::AccountId: From<AccountId> + Clone + Ss58Codec + Send + 'static,
    T::TargetSnapshotPerBlock: Send + 'static,
    T::VoterSnapshotPerBlock: Send + 'static,
    T::Pages: Send + 'static,
    T::MaxVotesPerVoter: Send + 'static,
{
    let app_state = AppState {
        storage_client,
        multi_block_storage_client,
        chain,
    };
    
    let app_router = Router::new()
        .route("/simulate", post(simulate::simulate_handler::<T>))
        .route("/snapshot", get(snapshot::snapshot_handler::<T>))
        .with_state(app_state)
        .layer(TraceLayer::new_for_http());
    app_router.into_make_service()
}