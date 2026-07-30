//! Hardware and operating system metrics collector module using `sysinfo`.

use serde::{Deserialize, Serialize};
use std::time::Instant;
use sysinfo::{Components, CpuRefreshKind, Disks, Networks, System};

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

/// Metric payload structure emitted by the agent collector.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectedMetrics {
    pub device_id: String,
    pub name: String,
    pub ip: Option<String>,
    pub device_type: Option<DeviceType>,
    pub cpu: f64,
    pub mem: f64,
    pub disk: f64,
    pub temp: Option<f64>,
    pub load_avg: (f64, f64, f64),
    pub net_in: f64,
    pub net_out: f64,
    pub uptime: u64,
    pub log_msg: Option<String>,
}

/// Sum received/transmitted byte counters across every interface.
fn total_bytes(networks: &Networks) -> (u64, u64) {
    networks
        .iter()
        .fold((0u64, 0u64), |(rx, tx), (_name, net)| {
            (
                rx.saturating_add(net.total_received()),
                tx.saturating_add(net.total_transmitted()),
            )
        })
}

/// Best-effort detection of this host's LAN-facing address.
///
/// Opens an unconnected UDP socket towards a public address; no packet is sent,
/// but the kernel picks the outbound interface, which is the address to report.
fn local_ip() -> Option<String> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    Some(socket.local_addr().ok()?.ip().to_string())
}

/// System metrics harvester wrapper around `sysinfo::System`.
pub struct SystemCollector {
    sys: System,
    disks: Disks,
    networks: Networks,
    components: Components,
    device_id: String,
    name: String,
    device_type: DeviceType,
    prev_rx_bytes: u64,
    prev_tx_bytes: u64,
    last_sample: Instant,
}

impl SystemCollector {
    /// Initialize a new metrics collector instance.
    pub fn new(
        device_id_opt: Option<String>,
        name_opt: Option<String>,
        device_type: String,
    ) -> Self {
        let mut sys = System::new_all();
        sys.refresh_all();

        let hostname = System::host_name().unwrap_or_else(|| "unknown-host".to_string());
        let device_id = device_id_opt.unwrap_or_else(|| hostname.clone());
        let name = name_opt.unwrap_or(hostname);

        let disks = Disks::new_with_refreshed_list();
        let networks = Networks::new_with_refreshed_list();
        let components = Components::new_with_refreshed_list();

        // Seed the byte counters from the current totals. Starting from zero would
        // make the first sample report all traffic since boot as a rate spike.
        let (prev_rx_bytes, prev_tx_bytes) = total_bytes(&networks);

        let parsed_device_type = match device_type.to_lowercase().as_str() {
            "network" => DeviceType::Network,
            "iot" => DeviceType::Iot,
            "workstation" => DeviceType::Workstation,
            "vm" => DeviceType::Vm,
            _ => DeviceType::Server,
        };

        Self {
            sys,
            disks,
            networks,
            components,
            device_id,
            name,
            device_type: parsed_device_type,
            prev_rx_bytes,
            prev_tx_bytes,
            last_sample: Instant::now(),
        }
    }

    /// Sample current system telemetry.
    pub fn sample(&mut self) -> CollectedMetrics {
        self.sys.refresh_cpu_specifics(CpuRefreshKind::everything());
        self.sys.refresh_memory();
        self.disks.refresh_list();
        self.networks.refresh_list();
        self.networks.refresh();
        self.components.refresh_list();
        self.components.refresh();

        // Calculate CPU usage percentage
        let global_cpu = self.sys.global_cpu_info().cpu_usage() as f64;

        // Calculate Memory usage percentage
        let total_mem = self.sys.total_memory() as f64;
        let used_mem = self.sys.used_memory() as f64;
        let mem_pct = if total_mem > 0.0 {
            (used_mem / total_mem) * 100.0
        } else {
            0.0
        };

        // Calculate Disk usage percentage
        let mut total_disk_bytes = 0u64;
        let mut available_disk_bytes = 0u64;
        for disk in self.disks.list() {
            total_disk_bytes += disk.total_space();
            available_disk_bytes += disk.available_space();
        }
        let disk_pct = if total_disk_bytes > 0 {
            let used = total_disk_bytes.saturating_sub(available_disk_bytes);
            (used as f64 / total_disk_bytes as f64) * 100.0
        } else {
            0.0
        };

        // Load average
        let load_avg = System::load_average();
        let load_tuple = (load_avg.one, load_avg.five, load_avg.fifteen);

        // Network calculation (Mbps estimate)
        let (total_rx, total_tx) = total_bytes(&self.networks);

        let rx_delta = total_rx.saturating_sub(self.prev_rx_bytes);
        let tx_delta = total_tx.saturating_sub(self.prev_tx_bytes);
        self.prev_rx_bytes = total_rx;
        self.prev_tx_bytes = total_tx;

        // Convert bytes to Mbps over the time actually elapsed since the previous
        // sample, which is not necessarily the configured interval.
        let elapsed = self.last_sample.elapsed().as_secs_f64().max(0.001);
        self.last_sample = Instant::now();
        let net_in_mbps = (rx_delta * 8) as f64 / 1_000_000.0 / elapsed;
        let net_out_mbps = (tx_delta * 8) as f64 / 1_000_000.0 / elapsed;

        // System uptime
        let uptime = System::uptime();

        // Highest component temperature, or None when the host exposes no sensor
        // (common on VMs and many SBCs). Reporting a plausible-looking number here
        // would make the dashboard lie.
        let temp = self
            .components
            .iter()
            .map(|comp| comp.temperature())
            .filter(|t| *t > 0.0 && t.is_finite())
            .fold(None::<f32>, |acc, t| Some(acc.map_or(t, |m: f32| m.max(t))))
            .map(|t| t as f64);

        let ip = local_ip();

        CollectedMetrics {
            device_id: self.device_id.clone(),
            name: self.name.clone(),
            ip,
            device_type: Some(self.device_type.clone()),
            cpu: global_cpu,
            mem: mem_pct,
            disk: disk_pct,
            temp,
            load_avg: load_tuple,
            net_in: (net_in_mbps * 100.0).round() / 100.0,
            net_out: (net_out_mbps * 100.0).round() / 100.0,
            uptime,
            log_msg: Some("Periodic metric telemetry heartbeat".to_string()),
        }
    }
}
