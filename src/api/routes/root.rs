use std::sync::Arc;
use crate::storage_client::StorageClient;
use jsonrpsee_ws_client::WsClient;
use axum::{
    Router,
    routing::{IntoMakeService, get, post},
};
use tower_http::trace::TraceLayer;

use crate::api::handler::{simulate, snapshot};

#[derive(Clone)]
pub struct AppState {
    pub storage_client: Arc<StorageClient<WsClient>>,
}

pub fn routes(storage_client: Arc<StorageClient<WsClient>>) -> IntoMakeService<Router> {
    let app_state = AppState {
        storage_client,
    };
    
    let app_router = Router::new()
        .route("/simulate", post(simulate::simulate_handler))
        .route("/snapshot", get(snapshot::snapshot_handler))
        .with_state(app_state)
        .layer(TraceLayer::new_for_http());
    app_router.into_make_service()
}