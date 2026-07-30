//! Axum router configuration and endpoint assembly.

pub mod devices;
pub mod metrics;
pub mod ws;

use crate::state::AppState;
use axum::{
    routing::{get, post},
    Router,
};
use tower_http::{
    cors::CorsLayer,
    services::{ServeDir, ServeFile},
};

/// Assemble the complete Axum router with REST APIs, WebSockets, and static frontend file serving.
pub fn create_router(state: AppState, static_dir: &str) -> Router {
    let index_file = format!("{}/index.dc.html", static_dir);

    let api_routes = Router::new()
        .route(
            "/devices",
            get(devices::list_devices).post(devices::create_device),
        )
        .route(
            "/devices/:id",
            get(devices::get_device).delete(devices::delete_device),
        )
        .route("/devices/:id/history", get(devices::get_device_history))
        .route("/summary", get(devices::get_summary))
        .route("/metrics", post(metrics::ingest_metrics));

    Router::new()
        .nest("/api/v1", api_routes)
        .route("/ws", get(ws::ws_handler))
        .route_service("/", ServeFile::new(index_file))
        .fallback_service(ServeDir::new(static_dir))
        .layer(CorsLayer::permissive())
        .with_state(state)
}
