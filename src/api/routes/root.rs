use std::sync::Arc;
use crate::{models::Chain};
use axum::{
    Router,
    routing::{IntoMakeService, get, post},
};
use tower_http::trace::TraceLayer;

use crate::api::handler::{simulate, snapshot};
use crate::simulate::{SimulateService};
use crate::snapshot::{SnapshotService};

pub struct AppState<
    Sim: SimulateService + Send + Sync + 'static,
    Snap: SnapshotService + Send + Sync + 'static,
> {
    pub simulate_service: Arc<Sim>,
    pub snapshot_service: Arc<Snap>,
    pub chain: Chain,
}

impl<Sim: SimulateService + Send + Sync + 'static, Snap: SnapshotService + Send + Sync + 'static> Clone for AppState<Sim, Snap> {
    fn clone(&self) -> Self {
        Self {
            simulate_service: self.simulate_service.clone(),
            snapshot_service: self.snapshot_service.clone(),
            chain: self.chain.clone(),
        }
    }
}

pub fn routes<
Sim: SimulateService + Send + Sync + 'static,
Snap: SnapshotService + Send + Sync + 'static,
>(
    simulate_service: Arc<Sim>,
    snapshot_service: Arc<Snap>,
    chain: Chain,
) -> IntoMakeService<Router>
{

    
    let app_state = AppState {
        simulate_service,
        snapshot_service,
        chain,
    };
    
    let app_router = Router::new()
        .route("/simulate", post(simulate::simulate_handler))
        .route("/snapshot", get(snapshot::snapshot_handler::<Sim, Snap>))
        .with_state(app_state)
        .layer(TraceLayer::new_for_http());
    app_router.into_make_service()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum_test::TestServer;
    use crate::miner_config::initialize_runtime_constants;
    use crate::snapshot::MockSnapshotService;
    use crate::simulate::MockSimulateService;

    #[tokio::test]
    async fn test_routes() {
        initialize_runtime_constants();
        let simulate_service = Arc::new(MockSimulateService::new());
        let snapshot_service = Arc::new(MockSnapshotService::new());
        let app_service = routes(
            simulate_service,
            snapshot_service,
            Chain::Polkadot,
        );
        let client = TestServer::new(app_service);
        assert!(client.is_ok());
        let client = client.unwrap();
        assert!(client.is_running());
    }
}