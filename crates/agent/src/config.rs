//! CLI configuration flags and parameters for the remote monitor agent.

use clap::Parser;

/// Remote system monitor agent command line interface arguments.
#[derive(Parser, Debug, Clone)]
#[command(author, version, about = "Remote system monitor agent for Monster Server", long_about = None)]
pub struct AgentConfig {
    /// URL of the target Monster Server ingest endpoint.
    #[arg(short, long)]
    pub server_url: Option<String>,

    /// Unique identifier for this device.
    #[arg(short, long)]
    pub device_id: Option<String>,

    /// Human readable name for this device.
    #[arg(short, long)]
    pub name: Option<String>,

    /// Device classification type (server, workstation, iot, network, vm).
    #[arg(short = 't', long, default_value = "server")]
    pub device_type: String,

    /// Sampling and report interval in seconds.
    #[arg(short, long, default_value_t = 3)]
    pub interval_secs: u64,

    /// Communication protocol to use (udp or http).
    #[arg(short = 'p', long, default_value = "udp")]
    pub protocol: String,
}
