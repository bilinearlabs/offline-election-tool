use axum::{
    extract::{Query, State}, http::StatusCode, response::Json
};
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::{
    api::routes::root::{AppState},
    api::utils,
    error::AppError,
    snapshot,
};

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

pub async fn snapshot_handler(
    State(state): State<AppState>,
    Query(params): Query<SnapshotRequest>,
) -> (StatusCode, Json<SnapshotResponse>) {
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

    let build_result = snapshot::build(state.subxt_client.as_ref(), block).await;

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

