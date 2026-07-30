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
    /// Number of days to keep historical metrics data (default: 7).
    pub history_days: u32,
    /// Path to the embedded redb database file (default: "monster_state.redb").
    pub db_path: String,
}

/// Read an environment variable, falling back to `default` when unset or unparseable.
fn env_or<T: std::str::FromStr>(key: &str, default: T) -> T {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: env_or("HOST", "0.0.0.0".to_string()),
            port: env_or("PORT", 3000),
            mdns_port: std::env::var("MDNS_PORT").ok().and_then(|p| p.parse().ok()),
            static_dir: env_or("STATIC_DIR", "design".to_string()),
            heartbeat_timeout_secs: env_or("HEARTBEAT_TIMEOUT_SECS", 15),
            history_days: env_or("HISTORY_DAYS", 7),
            db_path: env_or("DB_PATH", "monster_state.redb".to_string()),
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
