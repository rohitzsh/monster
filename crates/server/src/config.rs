//! Server configuration definitions and environment loading.

use std::net::SocketAddr;

/// Configuration settings for the Monster Monitoring Server.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Interface address to bind the server on (default: 0.0.0.0).
    pub host: String,
    /// Port number for HTTP/WebSocket traffic (default: 3000).
    pub port: u16,
    /// Port to advertise over mDNS (defaults to the listen port if not specified).
    pub mdns_port: Option<u16>,
    /// Path to static frontend files directory (default: "design").
    pub static_dir: String,
    /// Timeout in seconds before a device missing metrics is marked offline (default: 15s).
    pub heartbeat_timeout_secs: u64,
}

impl Default for ServerConfig {
    fn default() -> Self {
        let port = std::env::var("PORT")
            .unwrap_or_else(|_| "3000".into())
            .parse()
            .unwrap_or(3000);
        let mdns_port = std::env::var("MDNS_PORT").ok().and_then(|p| p.parse().ok());
        let host = std::env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());

        Self {
            host,
            port,
            mdns_port,
            static_dir: "design".to_string(),
            heartbeat_timeout_secs: 15,
        }
    }
}

impl ServerConfig {
    /// Returns the socket address formed by combining `host` and `port`.
    pub fn socket_addr(&self) -> SocketAddr {
        format!("{}:{}", self.host, self.port)
            .parse()
            .unwrap_or_else(|_| SocketAddr::from(([0, 0, 0, 0], self.port)))
    }
}
