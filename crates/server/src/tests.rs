//! Unit tests for server state management and data model transformations.

#[cfg(test)]
mod unit_tests {
    use crate::model::{CreateDeviceRequest, DeviceStatus, DeviceType, MetricPayload};
    use crate::state::AppState;

    #[tokio::test]
    async fn test_ingest_metrics_and_summary() {
        let state = AppState::new(15);
        let payload = MetricPayload {
            device_id: "agent-01".to_string(),
            name: "test-server".to_string(),
            ip: Some("192.168.1.50".to_string()),
            device_type: Some(DeviceType::Server),
            cpu: 45.5,
            mem: 60.0,
            disk: 30.0,
            temp: Some(55.0),
            load_avg: (0.5, 0.4, 0.3),
            net_in: 12.0,
            net_out: 8.5,
            uptime: 10000,
            log_msg: Some("Service restarted successfully".to_string()),
        };

        let device = state.ingest_metrics(payload).await;
        assert_eq!(device.id, "agent-01");
        assert_eq!(device.name, "test-server");
        assert_eq!(device.status, DeviceStatus::Online);
        assert_eq!(device.cpu, 45.5);

        let summary = state.get_summary().await;
        assert_eq!(summary.total_devices, 1);
        assert_eq!(summary.online, 1);
        assert_eq!(summary.alerts, 0);
        assert_eq!(summary.avg_cpu, 45.5);
    }

    #[tokio::test]
    async fn test_create_and_delete_device() {
        let state = AppState::new(15);
        let req = CreateDeviceRequest {
            name: "router-gw".to_string(),
            ip: "10.0.0.1".to_string(),
            device_type: Some(DeviceType::Network),
            tags: Some(vec!["gateway".to_string()]),
        };

        let device = state.create_device(req).await;
        assert_eq!(device.name, "router-gw");
        assert_eq!(device.ip, "10.0.0.1");

        let fetched = state.get_device(&device.id).await;
        assert!(fetched.is_some());

        let deleted = state.delete_device(&device.id).await;
        assert!(deleted);

        let after_delete = state.get_device(&device.id).await;
        assert!(after_delete.is_none());
    }

    #[tokio::test]
    async fn test_critical_thresholds() {
        let state = AppState::new(15);
        let payload = MetricPayload {
            device_id: "hot-node".to_string(),
            name: "overheated-node".to_string(),
            ip: None,
            device_type: None,
            cpu: 95.0,
            mem: 80.0,
            disk: 50.0,
            temp: Some(90.0),
            load_avg: (10.0, 8.0, 5.0),
            net_in: 0.0,
            net_out: 0.0,
            uptime: 500,
            log_msg: None,
        };

        let device = state.ingest_metrics(payload).await;
        assert_eq!(device.status, DeviceStatus::Critical);

        let summary = state.get_summary().await;
        assert_eq!(summary.alerts, 1);
    }
}
