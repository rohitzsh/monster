//! Unit tests for server state management and data model transformations.

#[cfg(test)]
mod unit_tests {
    use crate::db;
    use crate::model::{CreateDeviceRequest, DeviceStatus, DeviceType, MetricPayload};
    use crate::state::AppState;
    use std::sync::Arc;
    use tempfile::TempDir;

    /// Build an `AppState` backed by a throwaway redb file plus a running db worker.
    ///
    /// Mirrors `tests/common/mod.rs`; the integration tests live in a separate
    /// crate and cannot see `#[cfg(test)]` items from here.
    fn test_state() -> (AppState, Arc<redb::Database>, TempDir) {
        let dir = TempDir::new().expect("failed to create temp dir");
        let db_path = dir.path().join("test_state.redb");
        let database = db::init_db(db_path.to_str().expect("non-utf8 temp path"))
            .expect("failed to initialize test database");

        let (db_tx, db_rx) = tokio::sync::mpsc::channel(100);
        db::start_db_worker(database.clone(), db_rx);

        let state = AppState::new(15, db_tx, database.clone());
        (state, database, dir)
    }

    /// Sample payload with every field populated.
    fn payload(device_id: &str, cpu: f64, temp: Option<f64>) -> MetricPayload {
        MetricPayload {
            device_id: device_id.to_string(),
            name: device_id.to_string(),
            ip: Some("192.168.1.50".to_string()),
            device_type: Some(DeviceType::Server),
            cpu,
            mem: 60.0,
            disk: 30.0,
            temp,
            load_avg: (0.5, 0.4, 0.3),
            net_in: 12.0,
            net_out: 8.5,
            uptime: 10000,
            log_msg: None,
        }
    }

    /// Give the background database worker a moment to drain its queue.
    async fn settle(database: &redb::Database, expected: usize) {
        for _ in 0..50 {
            if db::load_devices(database)
                .map(|d| d.len())
                .unwrap_or(usize::MAX)
                == expected
            {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        panic!(
            "database worker did not settle at {} device(s); saw {:?}",
            expected,
            db::load_devices(database).map(|d| d.len()).ok()
        );
    }

    #[tokio::test]
    async fn test_ingest_metrics_and_summary() {
        let (state, _db, _dir) = test_state();
        let mut p = payload("agent-01", 45.5, Some(55.0));
        p.name = "test-server".to_string();
        p.log_msg = Some("Service restarted successfully".to_string());

        let device = state.ingest_metrics(p).await;
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
        let (state, _db, _dir) = test_state();
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
        let (state, _db, _dir) = test_state();
        let device = state
            .ingest_metrics(payload("hot-node", 95.0, Some(90.0)))
            .await;
        assert_eq!(device.status, DeviceStatus::Critical);

        let summary = state.get_summary().await;
        assert_eq!(summary.alerts, 1);
    }

    /// A device with no temperature sensor must be judged on CPU alone, and must
    /// not have a stand-in temperature invented for it.
    #[tokio::test]
    async fn test_missing_temperature_is_not_fabricated() {
        let (state, _db, _dir) = test_state();

        let hot = state.ingest_metrics(payload("no-sensor", 95.0, None)).await;
        assert_eq!(hot.temp, None);
        assert_eq!(hot.status, DeviceStatus::Critical);
        assert_eq!(hot.history.temp, vec![None]);

        let idle = state.ingest_metrics(payload("idle-vm", 10.0, None)).await;
        assert_eq!(idle.temp, None);
        assert_eq!(idle.status, DeviceStatus::Online);
    }

    /// Regression test: a deleted device used to survive in redb and reappear on
    /// the next server start, because only the in-memory map was updated.
    #[tokio::test]
    async fn test_delete_is_persisted() {
        let (state, database, _dir) = test_state();

        let device = state
            .ingest_metrics(payload("doomed", 20.0, Some(50.0)))
            .await;
        settle(&database, 1).await;

        assert!(state.delete_device(&device.id).await);
        settle(&database, 0).await;

        let persisted = db::load_devices(&database).expect("failed to reload devices");
        assert!(
            persisted.is_empty(),
            "deleted device is still persisted: {:?}",
            persisted.iter().map(|d| &d.id).collect::<Vec<_>>()
        );
    }
}
