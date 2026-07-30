//! Metric ingestion endpoint handler for remote monitor tools.

use crate::model::MetricPayload;
use crate::state::AppState;
use axum::{extract::State, http::StatusCode, Json};

/// Endpoint for remote monitor agents to post telemetry samples (`POST /api/v1/metrics`).
pub async fn ingest_metrics(
    State(state): State<AppState>,
    Json(payload): Json<MetricPayload>,
) -> (StatusCode, Json<serde_json::Value>) {
    let updated_device = state.ingest_metrics(payload).await;
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "success",
            "device": updated_device
        })),
    )
}
