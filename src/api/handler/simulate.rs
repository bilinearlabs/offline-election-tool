use axum::{
    extract::{Query, State}, http::StatusCode, response::Json
};
use serde::{Deserialize, Serialize};
use sp_core::crypto::Ss58Codec;

use crate::{
    api::routes::root::{AppState},
    api::utils,
    api::error::AppError,
    models::Algorithm,
    simulate,
    miner_config,
};
use pallet_election_provider_multi_block::unsigned::miner::MinerConfig;

#[derive(Deserialize)]
pub struct SimulateRequestQuery {
    pub block: Option<String>,
}

#[derive(Deserialize)]
pub struct SimulateRequestBody {
    pub count: Option<usize>,
    pub algorithm: Option<Algorithm>,
    pub iterations: Option<usize>,
    pub reduce: Option<bool>,
}

#[derive(Serialize)]
pub struct SimulateResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<crate::simulate::SimulationResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub async fn simulate_handler<T: MinerConfig + Send + Sync + Clone>(
    State(state): State<AppState<T>>,
    Query(params): Query<SimulateRequestQuery>,
    Json(body): Json<SimulateRequestBody>,
) -> (StatusCode, Json<SimulateResponse>)
where
    T::AccountId: Ss58Codec + Send + From<crate::primitives::AccountId>,
    T::TargetSnapshotPerBlock: Send,
    T::VoterSnapshotPerBlock: Send,
    T::Pages: Send,
    T::MaxVotesPerVoter: Send,
    T::Solution: Send,
{
    let block = match utils::parse_block(params.block) {
        Ok(block) => block,
        Err(e) => {
            return (StatusCode::BAD_REQUEST, Json(SimulateResponse {
                result: None,
                error: Some(e.to_string()),
            }));
        }
    };
    
    let raw_state_client = state.raw_state_client.as_ref();
    let multi_block_client = state.multi_block_state_client.as_ref();
    let targets_count = body.count;
    let algorithm = body.algorithm.unwrap_or(Algorithm::SeqPhragmen);
    let iterations = body.iterations.unwrap_or(0);
    let apply_reduce = body.reduce.unwrap_or(false);
    
    // Set balancing iterations from request
    miner_config::set_balancing_iterations(iterations);

    let (status, response) = match simulate::simulate(
        raw_state_client,
        multi_block_client,
        block,
        targets_count,
        algorithm,
        apply_reduce,
        None,
    ).await {
        Ok(result) => (
            StatusCode::OK,
            SimulateResponse {
                result: Some(result),
                error: None,
            }
        ),
        Err(e) => {
            if let Some(app_error) = e.downcast_ref::<AppError>() {
                match app_error {
                    AppError::NotFound(msg) => (
                        StatusCode::NOT_FOUND,
                        SimulateResponse {
                            result: None,
                            error: Some(msg.clone()),
                        }
                    ),
                    AppError::Other(msg) => (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        SimulateResponse {
                            result: None,
                            error: Some(msg.clone()),
                        }
                    ),
                }
            } else {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    SimulateResponse {
                        result: None,
                        error: Some(e.to_string()),
                    }
                )
            }
        }
    };

    (status, Json(response))
}

