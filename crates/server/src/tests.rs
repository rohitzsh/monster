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
            power_source: None,
            throttled: None,
            under_voltage: None,
        }
    }

    /// Sample payload with a power-health source reporting the given throttle state.
    fn payload_with_power(device_id: &str, throttled: bool, under_voltage: bool) -> MetricPayload {
        let mut p = payload(device_id, 20.0, Some(50.0));
        p.power_source = Some("raspberry-pi".to_string());
        p.throttled = Some(throttled);
        p.under_voltage = Some(under_voltage);
        p
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

    /// A device reporting active power throttling must be surfaced as at least
    /// Warning even when CPU/temp look fine, and a log entry must appear exactly
    /// once on the false->true edge, not on every sample.
    #[tokio::test]
    async fn test_throttling_forces_warning_and_logs_once() {
        let (state, _db, _dir) = test_state();

        let first = state
            .ingest_metrics(payload_with_power("pi-01", true, true))
            .await;
        assert_eq!(first.status, DeviceStatus::Warning);
        assert_eq!(first.throttled, Some(true));
        assert_eq!(first.under_voltage, Some(true));
        assert_eq!(first.power_source.as_deref(), Some("raspberry-pi"));
        assert_eq!(first.history.throttled, vec![Some(true)]);
        let throttle_logs = first
            .logs
            .iter()
            .filter(|l| l.message.contains("Power throttling detected"))
            .count();
        assert_eq!(throttle_logs, 1);

        // Still throttled on the next sample: no duplicate log entry.
        let second = state
            .ingest_metrics(payload_with_power("pi-01", true, true))
            .await;
        let throttle_logs = second
            .logs
            .iter()
            .filter(|l| l.message.contains("Power throttling detected"))
            .count();
        assert_eq!(throttle_logs, 1);
        assert_eq!(second.history.throttled, vec![Some(true), Some(true)]);

        // Clears: status recovers and a "cleared" entry is logged once.
        let cleared = state
            .ingest_metrics(payload_with_power("pi-01", false, false))
            .await;
        assert_eq!(cleared.status, DeviceStatus::Online);
        assert!(cleared
            .logs
            .iter()
            .any(|l| l.message == "Power throttling cleared"));
    }

    /// A device with no power-health source must report `None` throughout,
    /// never a fabricated `false`.
    #[tokio::test]
    async fn test_no_power_source_is_not_fabricated() {
        let (state, _db, _dir) = test_state();
        let device = state.ingest_metrics(payload("plain-vm", 20.0, None)).await;
        assert_eq!(device.power_source, None);
        assert_eq!(device.throttled, None);
        assert_eq!(device.under_voltage, None);
        assert_eq!(device.status, DeviceStatus::Online);
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
