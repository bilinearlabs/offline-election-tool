use axum::{
    extract::{Query, State}, http::StatusCode, response::Json
};

use serde::{Deserialize, Serialize};
use tracing::info;

use crate::{
    api::{routes::root::AppState, utils},
    snapshot::{SnapshotService},
    simulate::{SimulateService},
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
pub async fn snapshot_handler<
Sim: SimulateService + Send + Sync + 'static,
Snap: SnapshotService + Send + Sync + 'static,
>(
    State(state): State<AppState<
        Sim,
        Snap,
    >>,
    Query(params): Query<SnapshotRequest>,
) -> (StatusCode, Json<SnapshotResponse>)
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

    let build_result = state.snapshot_service.build(block).await;

    let (status, response) = match build_result {
        Ok(result) => (
            StatusCode::OK,
            SnapshotResponse {
                result: Some(result),
                error: None,
            }
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            SnapshotResponse {
                result: None,
                error: Some(e.to_string()),
            }
        ),
    };

    (status, Json(response))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::MockSnapshotService;
    use crate::models::Chain;
    use crate::simulate::MockSimulateService;
    use crate::models::{Snapshot, StakingConfig};
    use std::sync::Arc;

    #[tokio::test]
    async fn test_snapshot_handler() {
        let mut snapshot_service = MockSnapshotService::new();
        snapshot_service.expect_build().returning(move |_| {
            Ok(Snapshot {
                validators: vec![],
                nominators: vec![],
                config: StakingConfig {
                    desired_validators: 0,
                    max_nominations: 0,
                    min_nominator_bond: 0,
                    min_validator_bond: 0,
                },
            })
        });
        let app_state = AppState {
            simulate_service: Arc::new(MockSimulateService::new()),
            snapshot_service: Arc::new(snapshot_service),
            chain: Chain::Polkadot,
        };
        let app_state_extract = State(app_state);
        let result = snapshot_handler(app_state_extract, Query(SnapshotRequest { block: None })).await;
        assert_eq!(result.0, StatusCode::OK);
    }  

    #[tokio::test]
    async fn test_snapshot_handler_invalid_block() {
        let app_state = AppState {
            simulate_service: Arc::new(MockSimulateService::new()),
            snapshot_service: Arc::new(MockSnapshotService::new()),
            chain: Chain::Polkadot,
        };
        let app_state_extract = State(app_state);
        let result = snapshot_handler(app_state_extract, Query(SnapshotRequest { block: Some("invalid".to_string()) })).await;
        assert_eq!(result.0, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_snapshot_handler_error() {
        let mut snapshot_service = MockSnapshotService::new();
        snapshot_service.expect_build().returning(move |_| {
            Err(Box::new(
                std::io::Error::new(std::io::ErrorKind::Other, "Error")
            ))
        });
        let app_state = AppState {
            simulate_service: Arc::new(MockSimulateService::new()),
            snapshot_service: Arc::new(snapshot_service),
            chain: Chain::Polkadot,
        };
        let app_state_extract = State(app_state);
        let result = snapshot_handler(app_state_extract, Query(SnapshotRequest { block: None })).await;
        assert_eq!(result.0, StatusCode::INTERNAL_SERVER_ERROR);
    }
}