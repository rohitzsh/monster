//! Entry point for the Monster System Monitor Server.

mod config;
mod db;
mod handlers;
mod model;
mod state;

#[cfg(test)]
mod tests;

use bincode::Options;
use config::ServerConfig;
use mdns_sd::{ServiceDaemon, ServiceInfo};
use state::AppState;
use std::collections::HashMap;
use std::time::Duration;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging subscriber
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "info".into())
        .add_directive("mdns_sd=off".parse().unwrap());

    tracing_subscriber::registry()
        .with(env_filter)
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config = ServerConfig::default();

    // Initialize Database
    let db = db::init_db(&config.db_path)?;

    let (db_tx, db_rx) = tokio::sync::mpsc::channel(1000);

    let state = AppState::new(config.heartbeat_timeout_secs, db_tx, db.clone());

    // Load persisted devices
    if let Ok(loaded_devices) = db::load_devices(&db) {
        let mut store = state.store.write().await;
        for dev in loaded_devices {
            store.devices.insert(dev.id.clone(), dev);
        }
    }

    // Start database workers
    db::start_db_worker(db.clone(), db_rx);
    db::start_pruner_worker(db.clone(), config.history_days);

    // Spawn background task for checking device offline timeouts
    let heartbeat_state = state.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        loop {
            interval.tick().await;
            heartbeat_state.check_heartbeats().await;
        }
    });

    let app = handlers::create_router(state.clone(), &config.static_dir);
    let addr = config.socket_addr();

    info!("Monster Server starting on http://{}", addr);

    // Register mDNS service
    let broadcast_port = config.mdns_port.unwrap_or(config.port);
    let _mdns_keepalive = match start_mdns_broadcast(broadcast_port) {
        Ok(mdns) => {
            info!("mDNS auto-discovery broadcast active");
            Some(mdns)
        }
        Err(e) => {
            tracing::warn!("Failed to start mDNS broadcast: {}", e);
            None
        }
    };

    tokio::spawn(udp_listener(state.clone(), addr));

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

/// Largest datagram we accept. Anything larger has been truncated by `recv_from`.
const MAX_DATAGRAM: usize = 4096;

/// Ingest bincode-encoded metric payloads sent by agents over UDP.
async fn udp_listener(state: AppState, addr: std::net::SocketAddr) {
    let socket = match tokio::net::UdpSocket::bind(addr).await {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("Failed to bind UDP socket: {}", e);
            return;
        }
    };
    info!("Monster UDP Metrics Listener starting on {}", addr);

    // Datagrams are unauthenticated, so cap what a length prefix inside the
    // payload is allowed to make us allocate. The options must otherwise match
    // the agent's `bincode::serialize` (fixint, little endian).
    let opts = bincode::DefaultOptions::new()
        .with_fixint_encoding()
        .allow_trailing_bytes()
        .with_limit(MAX_DATAGRAM as u64);

    let mut buf = [0; MAX_DATAGRAM];
    loop {
        match socket.recv_from(&mut buf).await {
            Ok((len, src)) => {
                if len == MAX_DATAGRAM {
                    tracing::warn!("Truncated {}-byte datagram from {}; ignoring", len, src);
                    continue;
                }
                match opts.deserialize::<crate::model::MetricPayload>(&buf[..len]) {
                    Ok(payload) => {
                        state.ingest_metrics(payload).await;
                    }
                    Err(e) => {
                        tracing::debug!("Discarding malformed datagram from {}: {}", src, e)
                    }
                }
            }
            Err(e) => tracing::warn!("UDP recv error: {}", e),
        }
    }
}

/// Best-effort detection of the address other machines on the LAN can reach us on.
///
/// Opens an unconnected UDP socket towards a public address; no packet is sent,
/// but the kernel picks the outbound interface, which is what we want to advertise.
fn local_ip() -> Option<String> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    Some(socket.local_addr().ok()?.ip().to_string())
}

fn start_mdns_broadcast(port: u16) -> Result<ServiceDaemon, Box<dyn std::error::Error>> {
    let mdns = ServiceDaemon::new()?;
    let service_type = "_monster._tcp.local.";
    let instance_name = "monster-server";
    let host_name = "monster.local.";

    let ip = local_ip().unwrap_or_else(|| "127.0.0.1".to_string());

    let mut props = HashMap::new();
    props.insert("version".to_string(), "0.1.0".to_string());

    let service = ServiceInfo::new(
        service_type,
        instance_name,
        host_name,
        ip.as_str(),
        port,
        Some(props),
    )?;

    mdns.register(service)?;
    Ok(mdns)
}
