//! Integration tests for server REST endpoints.

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use monster_server::{handlers::create_router, state::AppState};
use serde_json::Value;
use tower::ServiceExt;

#[tokio::test]
async fn test_api_devices_flow() {
    let state = AppState::new(15);
    let app = create_router(state, "design");

    // 1. Initially GET /api/v1/devices returns empty array
    let req = Request::builder()
        .uri("/api/v1/devices")
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["devices"].as_array().unwrap().len(), 0);

    // 2. Post metric payload to POST /api/v1/metrics
    let metric_json = serde_json::json!({
        "device_id": "test-agent-1",
        "name": "integration-host",
        "ip": "127.0.0.1",
        "device_type": "server",
        "cpu": 25.0,
        "mem": 40.0,
        "disk": 15.0,
        "temp": 42.0,
        "load_avg": [0.2, 0.1, 0.05],
        "net_in": 5.5,
        "net_out": 2.2,
        "uptime": 12345
    });

    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/metrics")
        .header("content-type", "application/json")
        .body(Body::from(metric_json.to_string()))
        .unwrap();

    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // 3. Verify device now exists via GET /api/v1/devices
    let req = Request::builder()
        .uri("/api/v1/devices")
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["devices"].as_array().unwrap().len(), 1);
    assert_eq!(json["devices"][0]["name"], "integration-host");
}
