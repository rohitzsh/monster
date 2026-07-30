//! Entry point for the Monster System Monitor Server.

mod config;
mod handlers;
mod model;
mod state;

#[cfg(test)]
mod tests;

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
    let state = AppState::new(config.heartbeat_timeout_secs);

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

    let udp_state = state.clone();
    let udp_addr = addr;
    tokio::spawn(async move {
        let socket = match tokio::net::UdpSocket::bind(udp_addr).await {
            Ok(s) => s,
            Err(e) => {
                tracing::error!("Failed to bind UDP socket: {}", e);
                return;
            }
        };
        info!("Monster UDP Metrics Listener starting on {}", udp_addr);
        let mut buf = [0; 4096];
        loop {
            match socket.recv_from(&mut buf).await {
                Ok((len, _src)) => {
                    if let Ok(payload) =
                        bincode::deserialize::<crate::model::MetricPayload>(&buf[..len])
                    {
                        udp_state.ingest_metrics(payload).await;
                    }
                }
                Err(e) => tracing::warn!("UDP recv error: {}", e),
            }
        }
    });

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

fn start_mdns_broadcast(port: u16) -> Result<ServiceDaemon, Box<dyn std::error::Error>> {
    let mdns = ServiceDaemon::new()?;
    let service_type = "_monster._tcp.local.";
    let instance_name = "monster-server";
    let host_name = "monster.local.";

    // Detect local IP
    let ip = std::net::UdpSocket::bind("0.0.0.0:0")
        .and_then(|s| {
            s.connect("8.8.8.8:80")?;
            s.local_addr()
        })
        .map(|a| a.ip().to_string())
        .unwrap_or_else(|_| "127.0.0.1".to_string());

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
