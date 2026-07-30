//! Shared fixtures for the server integration tests.

use monster_server::{db, state::AppState};
use tempfile::TempDir;

/// Build an `AppState` backed by a throwaway redb file plus a running db worker.
///
/// The returned `TempDir` owns the database file, so callers must keep it alive
/// for as long as the state is in use.
pub fn test_state() -> (AppState, TempDir) {
    let dir = TempDir::new().expect("failed to create temp dir");
    let db_path = dir.path().join("test_state.redb");
    let db = db::init_db(db_path.to_str().expect("non-utf8 temp path"))
        .expect("failed to initialize test database");

    let (db_tx, db_rx) = tokio::sync::mpsc::channel(100);
    db::start_db_worker(db.clone(), db_rx);

    (AppState::new(15, db_tx, db), dir)
}
