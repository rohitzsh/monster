//! REST API handlers for managing monitored devices.

use crate::model::CreateDeviceRequest;
use crate::state::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};

/// Endpoint to list all registered devices (`GET /api/v1/devices`).
pub async fn list_devices(State(state): State<AppState>) -> Json<serde_json::Value> {
    let devices = state.get_devices().await;
    Json(serde_json::json!({
        "devices": devices
    }))
}

/// Endpoint to fetch a single device detail (`GET /api/v1/devices/:id`).
pub async fn get_device(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    match state.get_device(&id).await {
        Some(device) => Ok(Json(serde_json::json!({ "device": device }))),
        None => Err(StatusCode::NOT_FOUND),
    }
}

/// Endpoint to manually register a device (`POST /api/v1/devices`).
pub async fn create_device(
    State(state): State<AppState>,
    Json(payload): Json<CreateDeviceRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    let device = state.create_device(payload).await;
    (
        StatusCode::CREATED,
        Json(serde_json::json!({ "device": device })),
    )
}

/// Endpoint to delete a device (`DELETE /api/v1/devices/:id`).
pub async fn delete_device(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    if state.delete_device(&id).await {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

/// Endpoint to get system summary metrics (`GET /api/v1/summary`).
pub async fn get_summary(State(state): State<AppState>) -> Json<serde_json::Value> {
    let summary = state.get_summary().await;
    Json(serde_json::json!({ "summary": summary }))
}
