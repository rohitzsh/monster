# Monster 

Monster is a lightweight, remote system monitoring tool written in Rust. It's designed to be simple to set up on a home network, allowing you to track the health of all your machines—like Raspberry Pis, workstations, and servers—from a single dashboard. 

It's split into two parts: an **agent** that runs on your devices to collect hardware stats, and a **server** that receives this data and hosts the web interface. 

## Features

- **Low Overhead:** Written in Rust so the agent uses barely any CPU or memory while running in the background. 
- **Auto-Discovery:** You don't have to manually configure IP addresses. The agent uses mDNS to automatically find the server on your local network. 
- **Efficient Networking:** Metrics are packed into a compact binary format (`bincode`) and sent over UDP by default to keep network traffic as small as possible.
- **HTTP Fallback:** If UDP doesn't work for your setup, you can easily switch the agent back to standard HTTP/JSON.
- **Real-Time Dashboard:** The server runs a web UI that updates instantly to show CPU, memory, disk usage, temperatures, and network traffic for all connected devices.

## Getting Started

Make sure you have Rust installed, then clone the repository and run the components using Cargo. 

### 1. Start the Server

Run the server on the machine where you want to view the dashboard. 

```bash
cargo run --bin monster-server
```
By default, the web dashboard will be available at `http://127.0.0.1:3000`. 

### 2. Start the Agent

Run the agent on any machine you want to monitor. It will automatically look for the server on your local network.

```bash
cargo run --bin monster-agent
```

If the agent has trouble finding the server automatically, you can manually point it to the server's address:
```bash
cargo run --bin monster-agent -- --server-url http://192.168.1.50:3000
```

### Changing the Protocol

UDP is used by default because it's fast and keeps packet sizes tiny. If you need to use the HTTP API instead, just pass the protocol flag to the agent:
```bash
cargo run --bin monster-agent -- -p http
```

### Configuring the Server

The server is configured entirely through environment variables:

| Variable | Default | Purpose |
| --- | --- | --- |
| `HOST` | `0.0.0.0` | Interface to bind (HTTP and UDP). |
| `PORT` | `3000` | Port for HTTP, WebSocket, and UDP metrics. |
| `MDNS_PORT` | same as `PORT` | Port to advertise over mDNS, if it differs. |
| `STATIC_DIR` | `design` | Directory holding the dashboard files. |
| `HEARTBEAT_TIMEOUT_SECS` | `15` | Silence before a device is marked offline. |
| `HISTORY_DAYS` | `7` | How long historical samples are kept. |
| `DB_PATH` | `monster_state.redb` | Location of the embedded database file. |

## Architecture

- **`monster-agent`**: Uses the `sysinfo` crate to gather system hardware telemetry. It bundles this into a payload and ships it off to the server every few seconds. 
- **`monster-server`**: An `axum` based web server. It runs a background UDP listener for ingesting high-throughput metrics, serves a REST API for management, and hosts the static frontend files. Device state and history are persisted to an embedded `redb` database, so the dashboard survives a restart.

### Unavailable Readings

Not every machine exposes every sensor — VMs and many single-board computers
report no temperature at all. Monster reports these as *unknown* (`null` over the
API, `—` on the dashboard) rather than substituting a plausible-looking number.

### Frontend Dependencies

The dashboard is plain HTML/JS served from `design/` with no build step. It uses
a vendored copy of [Chart.js](https://www.chartjs.org) v4.5.1 at
`design/chart.umd.js`, which is committed so the dashboard works on an offline
LAN. Web fonts are loaded non-blocking from Google Fonts and fall back to system
fonts when there is no internet access.

## Security

Monster has no authentication and is intended for a trusted home network only.
The UDP ingest port accepts unauthenticated datagrams, and the REST API allows
anyone who can reach it to create and delete devices. Do not expose the server
to the internet.

## License
MIT
