mod ipc;

use std::sync::Arc;
use std::time::Duration;
use tracing::info;
use y_arena::WorkingArena;
use y_reactor::SessionReactor;
use y_storage::StorageEngine;
use y_types::SessionId;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .init();

    info!("Initializing Y Code Resident Background Daemon (y-daemon)...");

    let arena = Arc::new(WorkingArena::with_defaults());
    let purger = arena.purger();
    // Spawn 5.0-second decayed hysteresis watchdog
    purger.spawn_watchdog(Duration::from_millis(500));

    let db_path = "y_code_state.db";
    let storage = Arc::new(StorageEngine::with_defaults(db_path)?);

    let session_id = SessionId::new();
    let reactor = Arc::new(tokio::sync::Mutex::new(SessionReactor::new(
        session_id,
        arena.clone(),
        storage.clone(),
    )));

    info!("Y Code Session Reactor initialized. Session ID: {}", session_id);

    #[cfg(windows)]
    ipc::run_windows_ipc_server(reactor).await?;

    #[cfg(not(windows))]
    ipc::run_unix_ipc_server(reactor).await?;

    Ok(())
}
