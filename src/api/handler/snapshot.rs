use axum::{
    extract::{Query, State}, http::StatusCode, response::Json
};
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::{
    api::{error::AppError, routes::root::AppState, utils}, primitives::AccountId, snapshot
};
use pallet_election_provider_multi_block::unsigned::miner::MinerConfig;

#[derive(Deserialize)]
pub struct SnapshotRequest {
    pub block: Option<String>,
}

#[derive(Serialize)]
pub struct SnapshotResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<crate::models::Snapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}
pub async fn snapshot_handler<T: MinerConfig + Send + Sync + Clone>(
    State(state): State<AppState<T>>,
    Query(params): Query<SnapshotRequest>,
) -> (StatusCode, Json<SnapshotResponse>)
where
    T: MinerConfig<AccountId = AccountId> + Send,
    T::TargetSnapshotPerBlock: Send,
    T::VoterSnapshotPerBlock: Send,
    T::Pages: Send,
    T::MaxVotesPerVoter: Send,
{
    let block = match utils::parse_block(params.block) {
        Ok(block) => block,
        Err(e) => {
            return (StatusCode::BAD_REQUEST, Json(SnapshotResponse {
                result: None,
                error: Some(e.to_string()),
            }));
        }
    };

    info!("Block: {:?}", block);

    let build_result = snapshot::build(
        &state.multi_block_state_client, &state.raw_state_client, block).await;

    let (status, response) = match build_result {
        Ok(result) => (
            StatusCode::OK,
            SnapshotResponse {
                result: Some(result),
                error: None,
            }
        ),
        Err(e) => {
            if let Some(app_error) = e.downcast_ref::<AppError>() {
                match app_error {
                    AppError::NotFound(msg) => (
                        StatusCode::NOT_FOUND,
                        SnapshotResponse {
                            result: None,
                            error: Some(msg.clone()),
                        }
                    ),
                    AppError::Other(msg) => (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        SnapshotResponse {
                            result: None,
                            error: Some(msg.clone()),
                        }
                    ),
                }
            } else {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    SnapshotResponse {
                        result: None,
                        error: Some(e.to_string()),
                    }
                )
            }
        }
    };

    (status, Json(response))
}

