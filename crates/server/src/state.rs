//! Thread-safe state storage and real-time broadcasting manager.

use crate::db::DbCommand;
use crate::model::{
    CreateDeviceRequest, Device, DeviceStatus, DeviceType, LogEvent, MetricPayload, SummaryStats,
};
use chrono::Utc;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::{broadcast, RwLock};
use tracing::warn;

/// Event message broadcast over WebSockets to web dashboard clients.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "type", content = "data")]
pub enum WsEvent {
    /// Full device list refreshed.
    List(Vec<Device>),
    /// Single device metric updated.
    Updated(Box<Device>),
    /// Device removed.
    Deleted(String),
}

/// Internal store containing in-memory devices.
#[derive(Debug, Default)]
pub struct ServerStore {
    /// Devices mapped by device ID.
    pub devices: HashMap<String, Device>,
}

/// Shared application state wrapper.
#[derive(Clone)]
pub struct AppState {
    /// Thread-safe in-memory database of devices.
    pub store: Arc<RwLock<ServerStore>>,
    /// Read-only access to the redb database for history queries.
    pub db: Arc<redb::Database>,
    /// Broadcast channel for real-time WebSocket notifications.
    pub tx: broadcast::Sender<WsEvent>,
    /// Channel to send persistence commands to the background redb worker.
    pub db_tx: tokio::sync::mpsc::Sender<DbCommand>,
    /// Timeout threshold in seconds before an inactive device is marked offline.
    pub heartbeat_timeout_secs: u64,
}

impl AppState {
    /// Create a new application state with specified heartbeat timeout.
    pub fn new(
        heartbeat_timeout_secs: u64,
        db_tx: tokio::sync::mpsc::Sender<DbCommand>,
        db: Arc<redb::Database>,
    ) -> Self {
        let (tx, _) = broadcast::channel(100);
        Self {
            store: Arc::new(RwLock::new(ServerStore::default())),
            db,
            tx,
            db_tx,
            heartbeat_timeout_secs,
        }
    }

    /// Update or register device metrics from an agent payload.
    pub async fn ingest_metrics(&self, payload: MetricPayload) -> Device {
        let now = Utc::now().timestamp();
        let mut store = self.store.write().await;

        let mut target_id = payload.device_id.clone();
        if !store.devices.contains_key(&target_id) {
            if let Some((id, _)) = store.devices.iter().find(|(_, d)| {
                (payload.ip.is_some() && Some(&d.ip) == payload.ip.as_ref())
                    || (d.name == payload.name)
                    || (d.name == payload.device_id)
            }) {
                target_id = id.clone();
            }
        }

        let device = store.devices.entry(target_id).or_insert_with(|| Device {
            id: payload.device_id.clone(),
            name: payload.name.clone(),
            ip: payload.ip.clone().unwrap_or_else(|| "127.0.0.1".into()),
            device_type: payload.device_type.clone().unwrap_or(DeviceType::Server),
            status: DeviceStatus::Online,
            cpu: 0.0,
            mem: 0.0,
            disk: 0.0,
            temp: None,
            load1: 0.0,
            load5: 0.0,
            load15: 0.0,
            net_in: 0.0,
            net_out: 0.0,
            uptime: 0,
            last_seen_at: now,
            tags: vec!["agent".to_string()],
            history: Default::default(),
            logs: vec![LogEvent {
                ts: now,
                message: "Device registered via monitor agent".to_string(),
            }],
        });

        // Update fields
        device.name = payload.name;
        if let Some(ip) = payload.ip {
            device.ip = ip;
        }
        if let Some(dt) = payload.device_type {
            device.device_type = dt;
        }
        device.cpu = payload.cpu;
        device.mem = payload.mem;
        device.disk = payload.disk;
        device.temp = payload.temp;
        device.load1 = payload.load_avg.0;
        device.load5 = payload.load_avg.1;
        device.load15 = payload.load_avg.2;
        device.net_in = payload.net_in;
        device.net_out = payload.net_out;
        device.uptime = payload.uptime;
        device.last_seen_at = now;

        // Status rules: high load / temp -> Warning or Critical, otherwise Online.
        // A device with no temperature sensor is judged on CPU alone.
        let temp = device.temp;
        if device.cpu > 90.0 || temp.is_some_and(|t| t > 85.0) {
            device.status = DeviceStatus::Critical;
        } else if device.cpu > 75.0 || temp.is_some_and(|t| t > 75.0) {
            device.status = DeviceStatus::Warning;
        } else {
            device.status = DeviceStatus::Online;
        }

        // Push time series history (max 30 samples)
        device.history.cpu.push(device.cpu);
        if device.history.cpu.len() > 30 {
            device.history.cpu.remove(0);
        }
        device.history.mem.push(device.mem);
        if device.history.mem.len() > 30 {
            device.history.mem.remove(0);
        }
        device.history.temp.push(device.temp);
        if device.history.temp.len() > 30 {
            device.history.temp.remove(0);
        }

        if let Some(msg) = payload.log_msg {
            device.logs.insert(
                0,
                LogEvent {
                    ts: now,
                    message: msg,
                },
            );
            if device.logs.len() > 20 {
                device.logs.pop();
            }
        }

        let updated_device = device.clone();
        let _ = self
            .tx
            .send(WsEvent::Updated(Box::new(updated_device.clone())));
        self.persist(DbCommand::Upsert(Box::new(updated_device.clone())));
        updated_device
    }

    /// Hand a command to the background database worker, logging if it cannot be queued.
    fn persist(&self, cmd: DbCommand) {
        match self.db_tx.try_send(cmd) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => {
                warn!("Database worker queue is full; dropped a persistence command")
            }
            Err(TrySendError::Closed(_)) => {
                warn!("Database worker is gone; persistence is no longer running")
            }
        }
    }

    /// Manually register a new device via REST API.
    pub async fn create_device(&self, req: CreateDeviceRequest) -> Device {
        let now = Utc::now().timestamp();
        let id = format!("dev-{}", uuid::Uuid::new_v4().simple());
        let device = Device {
            id: id.clone(),
            name: req.name,
            ip: req.ip,
            device_type: req.device_type.unwrap_or(DeviceType::Server),
            status: DeviceStatus::Offline,
            cpu: 0.0,
            mem: 0.0,
            disk: 0.0,
            temp: None,
            load1: 0.0,
            load5: 0.0,
            load15: 0.0,
            net_in: 0.0,
            net_out: 0.0,
            uptime: 0,
            last_seen_at: now,
            tags: req.tags.unwrap_or_default(),
            history: Default::default(),
            logs: vec![LogEvent {
                ts: now,
                message: "Manually created via web console, waiting for agent...".to_string(),
            }],
        };

        let mut store = self.store.write().await;
        store.devices.insert(id.clone(), device.clone());
        let _ = self.tx.send(WsEvent::Updated(Box::new(device.clone())));
        self.persist(DbCommand::Upsert(Box::new(device.clone())));
        device
    }

    /// Retrieve list of all current devices.
    pub async fn get_devices(&self) -> Vec<Device> {
        let store = self.store.read().await;
        let mut list: Vec<Device> = store.devices.values().cloned().collect();
        list.sort_by(|a, b| a.name.cmp(&b.name));
        list
    }

    /// Retrieve a single device by identifier.
    pub async fn get_device(&self, id: &str) -> Option<Device> {
        let store = self.store.read().await;
        store.devices.get(id).cloned()
    }

    /// Remove a device from storage by identifier.
    pub async fn delete_device(&self, id: &str) -> bool {
        let mut store = self.store.write().await;
        let removed = store.devices.remove(id).is_some();
        if removed {
            let _ = self.tx.send(WsEvent::Deleted(id.to_string()));
            // Without this the device is resurrected from redb on the next restart.
            self.persist(DbCommand::Delete(id.to_string()));
        }
        removed
    }

    /// Calculate summary stats across all registered devices.
    pub async fn get_summary(&self) -> SummaryStats {
        let store = self.store.read().await;
        let total = store.devices.len();
        let mut online = 0;
        let mut offline = 0;
        let mut alerts = 0;
        let mut cpu_sum = 0.0;

        for device in store.devices.values() {
            match device.status {
                DeviceStatus::Online => online += 1,
                DeviceStatus::Offline => offline += 1,
                DeviceStatus::Warning | DeviceStatus::Critical => {
                    alerts += 1;
                    online += 1;
                }
            }
            if device.status != DeviceStatus::Offline {
                cpu_sum += device.cpu;
            }
        }

        let online_count = online;
        let avg_cpu = if online_count > 0 {
            cpu_sum / online_count as f64
        } else {
            0.0
        };

        SummaryStats {
            total_devices: total,
            online,
            offline,
            alerts,
            avg_cpu,
        }
    }

    /// Periodically evaluate device heartbeats and transition stale devices to offline.
    pub async fn check_heartbeats(&self) {
        let now = Utc::now().timestamp();
        let timeout = self.heartbeat_timeout_secs as i64;
        let mut store = self.store.write().await;

        for device in store.devices.values_mut() {
            if device.status != DeviceStatus::Offline && (now - device.last_seen_at) > timeout {
                device.status = DeviceStatus::Offline;
                device.logs.insert(
                    0,
                    LogEvent {
                        ts: now,
                        message: "Heartbeat timeout: marked offline".to_string(),
                    },
                );
                let _ = self.tx.send(WsEvent::Updated(Box::new(device.clone())));
            }
        }
    }
}
