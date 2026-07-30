//! Local persistence module using redb.

use crate::model::Device;
use redb::{Database, ReadableTable, TableDefinition};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

pub const DEVICES_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("devices");
pub const HISTORY_TABLE: TableDefinition<(&str, u64), &[u8]> = TableDefinition::new("history");

/// A single historical sample as stored in `HISTORY_TABLE`.
pub type HistoryPoint = (f64, f64, Option<f64>, f64, f64);

/// Work item sent to the background database worker.
#[derive(Debug, Clone)]
pub enum DbCommand {
    /// Persist device metadata and append a history sample.
    Upsert(Box<Device>),
    /// Remove a device and all of its historical samples.
    Delete(String),
}

/// Initialize the embedded redb database and ensure tables exist.
pub fn init_db(path: &str) -> Result<Arc<Database>, Box<dyn std::error::Error>> {
    let db = Database::create(path)?;
    let write_txn = db.begin_write()?;
    {
        let _ = write_txn.open_table(DEVICES_TABLE)?;
        let _ = write_txn.open_table(HISTORY_TABLE)?;
    }
    write_txn.commit()?;
    Ok(Arc::new(db))
}

/// Read all devices from the database on startup.
pub fn load_devices(db: &Database) -> Result<Vec<Device>, Box<dyn std::error::Error>> {
    let read_txn = db.begin_read()?;
    let table = match read_txn.open_table(DEVICES_TABLE) {
        Ok(t) => t,
        Err(_) => return Ok(vec![]),
    };

    let mut devices = Vec::new();
    for result in table.iter()? {
        let (_, value) = result?;
        // Rows written by an older build may no longer match the current `Device`
        // layout; skip them rather than refusing to start.
        match bincode::deserialize::<Device>(value.value()) {
            Ok(device) => devices.push(device),
            Err(e) => warn!("Skipping unreadable persisted device row: {}", e),
        }
    }
    Ok(devices)
}

/// Dedicated OS thread for batching database writes without blocking the Tokio runtime.
pub fn start_db_worker(db: Arc<Database>, mut rx: mpsc::Receiver<DbCommand>) {
    std::thread::spawn(move || {
        let mut batch = Vec::new();
        while let Some(cmd) = rx.blocking_recv() {
            batch.push(cmd);
            // Drain the channel of any currently pending items to batch them
            while let Ok(next) = rx.try_recv() {
                batch.push(next);
            }

            if let Err(e) = apply_batch(&db, &batch) {
                // Keep the worker alive: a failed batch must not take persistence
                // down for the remaining lifetime of the process.
                error!(
                    "Failed to apply redb batch of {} item(s): {}",
                    batch.len(),
                    e
                );
            }
            batch.clear();
        }
        info!("Database worker shutting down: command channel closed");
    });
}

/// Apply one batch of commands inside a single write transaction.
fn apply_batch(db: &Database, batch: &[DbCommand]) -> Result<(), Box<dyn std::error::Error>> {
    let write_txn = db.begin_write()?;
    {
        let mut devices_table = write_txn.open_table(DEVICES_TABLE)?;
        let mut history_table = write_txn.open_table(HISTORY_TABLE)?;

        for cmd in batch {
            match cmd {
                DbCommand::Upsert(device) => {
                    // 1. Update metadata
                    match bincode::serialize(device) {
                        Ok(bytes) => {
                            devices_table.insert(device.id.as_str(), bytes.as_slice())?;
                        }
                        Err(e) => error!("Failed to serialize device {}: {}", device.id, e),
                    }

                    // 2. Insert time-series historical data point
                    let timestamp = chrono::Utc::now().timestamp() as u64;
                    // Store: (cpu, mem, temp, net_in, net_out)
                    let data_point: HistoryPoint = (
                        device.cpu,
                        device.mem,
                        device.temp,
                        device.net_in,
                        device.net_out,
                    );

                    match bincode::serialize(&data_point) {
                        Ok(bytes) => {
                            history_table
                                .insert((device.id.as_str(), timestamp), bytes.as_slice())?;
                        }
                        Err(e) => error!("Failed to serialize history point: {}", e),
                    }
                }
                DbCommand::Delete(id) => {
                    devices_table.remove(id.as_str())?;
                    // Drain every historical sample belonging to this device.
                    let stale: Vec<u64> = history_table
                        .range((id.as_str(), 0)..=(id.as_str(), u64::MAX))?
                        .filter_map(|entry| entry.ok().map(|(key, _)| key.value().1))
                        .collect();
                    for ts in stale {
                        history_table.remove((id.as_str(), ts))?;
                    }
                }
            }
        }
    }
    write_txn.commit()?;
    Ok(())
}

/// Periodic background thread to prune historical data older than `history_days`.
pub fn start_pruner_worker(db: Arc<Database>, history_days: u32) {
    std::thread::spawn(move || {
        loop {
            // Prune on startup first, so a server that restarts more often than
            // once an hour still gets its history trimmed.
            if let Err(e) = prune_once(&db, history_days) {
                error!("Failed to prune historical data: {}", e);
            } else {
                info!("Pruned historical data older than {} days", history_days);
            }

            // Prune every hour
            std::thread::sleep(Duration::from_secs(3600));
        }
    });
}

/// Delete every history sample older than `history_days`.
fn prune_once(db: &Database, history_days: u32) -> Result<(), Box<dyn std::error::Error>> {
    let cutoff =
        (chrono::Utc::now() - chrono::Duration::days(history_days as i64)).timestamp() as u64;

    let write_txn = db.begin_write()?;
    {
        let mut history_table = write_txn.open_table(HISTORY_TABLE)?;
        let mut keys_to_delete = Vec::new();

        // Iterate to find old records. For a home monitor, a full scan hourly is perfectly fine.
        for result in history_table.iter()? {
            let (key, _) = result?;
            let (dev_id, ts) = key.value();
            if ts < cutoff {
                keys_to_delete.push((dev_id.to_string(), ts));
            }
        }

        for (dev_id, ts) in keys_to_delete {
            history_table.remove((dev_id.as_str(), ts))?;
        }
    }
    write_txn.commit()?;
    Ok(())
}
