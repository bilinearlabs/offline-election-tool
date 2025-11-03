use axum::{
    extract::{Query, State}, http::StatusCode, response::Json
};
use serde::{Deserialize, Serialize};

use crate::{
    api::routes::root::{AppState},
    api::utils,
    error::AppError,
    models::Algorithm,
    simulate
};

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
    pub result: Option<crate::simulate::SimulationResult>,
    pub error: Option<String>,
}

pub async fn simulate_handler(
    State(state): State<AppState>,
    Query(params): Query<SimulateRequestQuery>,
    Json(body): Json<SimulateRequestBody>,
) -> (StatusCode, Json<SimulateResponse>) {
    let block = match utils::parse_block(params.block) {
        Ok(block) => block,
        Err(e) => {
            return (StatusCode::BAD_REQUEST, Json(SimulateResponse {
                result: None,
                error: Some(e.to_string()),
            }));
        }
    };
    
    let storage_client = state.storage_client.as_ref();
    let targets_count = body.count;
    let algorithm = body.algorithm.unwrap_or(Algorithm::SeqPhragmen);
    let iterations = body.iterations.unwrap_or(0);
    let apply_reduce = body.reduce.unwrap_or(false);

    let (status, response) = match simulate::simulate(storage_client, block, targets_count, algorithm, iterations, apply_reduce).await {
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

