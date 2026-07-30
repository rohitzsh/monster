//! REST API handlers for managing monitored devices.

use crate::model::CreateDeviceRequest;
use crate::state::AppState;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct HistoryQuery {
    pub days: Option<u32>,
}

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

/// Endpoint to fetch historical device data (`GET /api/v1/devices/:id/history`).
pub async fn get_device_history(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<HistoryQuery>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let days = query.days.unwrap_or(7);
    let cutoff = (chrono::Utc::now() - chrono::Duration::days(days as i64)).timestamp() as u64;

    let read_txn = state
        .db
        .begin_read()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let table = read_txn
        .open_table(crate::db::HISTORY_TABLE)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut history_points = Vec::new();

    let start_key = (id.as_str(), cutoff);
    let end_key = (id.as_str(), u64::MAX);

    let iter = table
        .range(start_key..=end_key)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    for entry in iter.flatten() {
        let (key, value) = entry;
        let (dev_id, ts) = key.value();
        if dev_id != id {
            continue;
        }
        if let Ok((cpu, mem, temp, net_in, net_out, throttled)) =
            bincode::deserialize::<crate::db::HistoryPoint>(value.value())
        {
            history_points.push(serde_json::json!({
                "timestamp": ts,
                "cpu": cpu,
                "mem": mem,
                "temp": temp,
                "net_in": net_in,
                "net_out": net_out,
                "throttled": throttled
            }));
        }
    }

    Ok(Json(serde_json::json!({ "history": history_points })))
}
