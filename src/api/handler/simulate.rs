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
    simulate::{self, Override},
    miner_config,
};
use pallet_election_provider_multi_block::unsigned::miner::MinerConfig;

#[derive(Deserialize)]
pub struct SimulateRequestQuery {
    pub block: Option<String>,
}

#[derive(Deserialize)]
pub struct SimulateRequestBody {
    pub algorithm: Option<Algorithm>,
    pub iterations: Option<usize>,
    pub reduce: Option<bool>,
    pub desired_validators: Option<u32>,
    pub max_nominations: Option<u32>,
    pub min_nominator_bond: Option<u128>,
    pub min_validator_bond: Option<u128>,
    pub manual_override: Option<Override>,
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
    let algorithm = body.algorithm.unwrap_or(Algorithm::SeqPhragmen);
    let iterations = body.iterations.unwrap_or(0);
    let desired_validators = body.desired_validators;
    let max_nominations = body.max_nominations;
    let apply_reduce = body.reduce.unwrap_or(false);
    let min_nominator_bond = body.min_nominator_bond;
    let min_validator_bond = body.min_validator_bond;
    let manual_override = body.manual_override;
    
    // Run simulation within task-local scope for algorithm, iterations, and max nominations
    // This ensures each concurrent request gets its own isolated value
    let result = miner_config::with_election_config(algorithm, iterations, max_nominations, async {
        simulate::simulate(
            raw_state_client,
            multi_block_client,
            block,
            desired_validators,
            apply_reduce,
            manual_override,
            min_nominator_bond,
            min_validator_bond,
        ).await
    }).await;

    let (status, response) = match result {
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

