//! Entry point for the Monster Remote System Monitor Agent.

mod collector;
mod config;
mod power;
mod reporter;

#[cfg(test)]
mod tests;

use clap::Parser;
use collector::SystemCollector;
use config::AgentConfig;
use reporter::MetricReporter;
use std::time::Duration;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging subscriber
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config = AgentConfig::parse();

    let server_url = match config.server_url {
        Some(url) => url,
        None => {
            info!("No server URL provided, attempting to auto-discover via mDNS...");
            match discover_server().await {
                Some(url) => {
                    info!("Discovered Monster Server at {} via mDNS", url);
                    url
                }
                None => {
                    tracing::error!(
                        "Failed to auto-discover server via mDNS. Please provide --server-url"
                    );
                    std::process::exit(1);
                }
            }
        }
    };

    info!(
        "Starting Monster Agent monitoring loop reporting to {} via {}",
        server_url,
        config.protocol.to_uppercase()
    );

    let mut collector = SystemCollector::new(config.device_id, config.name, config.device_type);
    let reporter = MetricReporter::new(&server_url, &config.protocol).await;

    let mut interval = tokio::time::interval(Duration::from_secs(config.interval_secs));
    loop {
        interval.tick().await;
        let metrics = collector.sample();
        if let Err(e) = reporter.send_metrics(&metrics).await {
            tracing::warn!("Failed to report metrics to server: {}", e);
        }
    }
}

async fn discover_server() -> Option<String> {
    use mdns_sd::{ServiceDaemon, ServiceEvent};

    let mdns = ServiceDaemon::new().ok()?;
    let service_type = "_monster._tcp.local.";
    let receiver = mdns.browse(service_type).ok()?;

    let timeout = tokio::time::sleep(Duration::from_secs(10));
    tokio::pin!(timeout);

    loop {
        tokio::select! {
            _ = &mut timeout => {
                return None;
            }
            event = receiver.recv_async() => {
                if let Ok(ServiceEvent::ServiceResolved(info)) = event {
                    if let Some(addr) = info.get_addresses().iter().next() {
                        return Some(format!("http://{}:{}", addr, info.get_port()));
                    }
                }
            }
        }
    }
}
