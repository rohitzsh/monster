//! Client module for transmitting system metrics to Monster Server.

use crate::collector::CollectedMetrics;
use reqwest::Client;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::UdpSocket;
use tracing::{error, info};

pub enum ReporterProtocol {
    Http {
        client: Client,
        endpoint: String,
    },
    Udp {
        socket: Arc<UdpSocket>,
        target: String,
    },
}

/// Reporter client communicating with Monster Server.
pub struct MetricReporter {
    protocol: ReporterProtocol,
}

impl MetricReporter {
    /// Construct a new reporter.
    pub async fn new(server_base_url: &str, protocol: &str) -> Self {
        if protocol.eq_ignore_ascii_case("http") {
            let client = Client::builder()
                .timeout(Duration::from_secs(5))
                .build()
                .unwrap_or_default();
            let endpoint = format!("{}/api/v1/metrics", server_base_url.trim_end_matches('/'));
            Self {
                protocol: ReporterProtocol::Http { client, endpoint },
            }
        } else {
            // Extract host:port from http://host:port (e.g. from mDNS)
            let target = server_base_url
                .strip_prefix("http://")
                .or_else(|| server_base_url.strip_prefix("https://"))
                .unwrap_or(server_base_url)
                .to_string();

            let socket = UdpSocket::bind("0.0.0.0:0")
                .await
                .expect("Failed to bind UDP socket");
            Self {
                protocol: ReporterProtocol::Udp {
                    socket: Arc::new(socket),
                    target,
                },
            }
        }
    }

    /// Transmit a collected metrics sample payload to the server.
    pub async fn send_metrics(
        &self,
        metrics: &CollectedMetrics,
    ) -> Result<(), Box<dyn std::error::Error>> {
        match &self.protocol {
            ReporterProtocol::Http { client, endpoint } => {
                let response = client.post(endpoint).json(metrics).send().await?;

                if response.status().is_success() {
                    info!(
                        "Successfully reported metrics via HTTP for '{}' (CPU: {:.1}%, Mem: {:.1}%)",
                        metrics.name, metrics.cpu, metrics.mem
                    );
                    Ok(())
                } else {
                    let status = response.status();
                    error!("Server returned error status: {}", status);
                    Err(format!("Server returned HTTP {}", status).into())
                }
            }
            ReporterProtocol::Udp { socket, target } => {
                let encoded = bincode::serialize(metrics)?;
                socket.send_to(&encoded, target).await?;
                info!(
                    "Successfully reported metrics via UDP for '{}' (CPU: {:.1}%, Mem: {:.1}%)",
                    metrics.name, metrics.cpu, metrics.mem
                );
                Ok(())
            }
        }
    }
}
