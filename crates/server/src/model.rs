//! Data models and domain types for system monitoring.

use serde::{Deserialize, Serialize};

/// Categorization of monitored device hardware/software environment.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum DeviceType {
    #[default]
    Server,
    Network,
    Iot,
    Workstation,
    Vm,
}

/// Current status of a monitored device.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DeviceStatus {
    Online,
    Offline,
    Warning,
    Critical,
}

/// Log event entry associated with a device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEvent {
    /// Timestamp of log entry (epoch seconds).
    pub ts: i64,
    /// Message content describing the event.
    pub message: String,
}

/// Metric history point for chart rendering.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MetricHistory {
    /// Historical CPU percentage data points.
    pub cpu: Vec<f64>,
    /// Historical memory percentage data points.
    pub mem: Vec<f64>,
    /// Historical temperature data points. `None` where the device reports no sensor.
    pub temp: Vec<Option<f64>>,
}

/// Full record of a monitored system device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    /// Unique identifier for the device.
    pub id: String,
    /// Human-readable device name.
    pub name: String,
    /// Primary IP address of the device.
    pub ip: String,
    /// Type of device (server, iot, etc.).
    pub device_type: DeviceType,
    /// Operational status of the device.
    pub status: DeviceStatus,
    /// Current CPU usage percentage (0-100%).
    pub cpu: f64,
    /// Current memory usage percentage (0-100%).
    pub mem: f64,
    /// Current disk usage percentage (0-100%).
    pub disk: f64,
    /// Current temperature in Celsius, or `None` if the device exposes no sensor.
    pub temp: Option<f64>,
    /// 1-minute load average.
    pub load1: f64,
    /// 5-minute load average.
    pub load5: f64,
    /// 15-minute load average.
    pub load15: f64,
    /// Network inbound rate in Mbps.
    pub net_in: f64,
    /// Network outbound rate in Mbps.
    pub net_out: f64,
    /// System uptime in seconds.
    pub uptime: u64,
    /// Last timestamp when metrics were received (epoch seconds).
    pub last_seen_at: i64,
    /// Associated search/filter tags.
    pub tags: Vec<String>,
    /// Time-series metric history.
    pub history: MetricHistory,
    /// Recent log events.
    pub logs: Vec<LogEvent>,
}

/// Incoming metric payload posted by remote monitor agents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricPayload {
    /// Device identifier or name.
    pub device_id: String,
    /// Human-readable device name.
    pub name: String,
    /// Device IP address.
    pub ip: Option<String>,
    /// Device type.
    pub device_type: Option<DeviceType>,
    /// CPU usage percentage.
    pub cpu: f64,
    /// Memory usage percentage.
    pub mem: f64,
    /// Disk usage percentage.
    pub disk: f64,
    /// Temperature in Celsius.
    pub temp: Option<f64>,
    /// Load averages: [1m, 5m, 15m].
    pub load_avg: (f64, f64, f64),
    /// Network traffic inbound (Mbps).
    pub net_in: f64,
    /// Network traffic outbound (Mbps).
    pub net_out: f64,
    /// System uptime in seconds.
    pub uptime: u64,
    /// Optional log message to report.
    pub log_msg: Option<String>,
}

/// Request payload to manually register a device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateDeviceRequest {
    /// Device name.
    pub name: String,
    /// Device IP address.
    pub ip: String,
    /// Device classification.
    pub device_type: Option<DeviceType>,
    /// Device tags.
    pub tags: Option<Vec<String>>,
}

/// Summary aggregate stats for dashboard header.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SummaryStats {
    /// Total number of registered devices.
    pub total_devices: usize,
    /// Number of online devices.
    pub online: usize,
    /// Number of offline devices.
    pub offline: usize,
    /// Number of devices in warning/critical state.
    pub alerts: usize,
    /// Average CPU utilization among online devices.
    pub avg_cpu: f64,
}
