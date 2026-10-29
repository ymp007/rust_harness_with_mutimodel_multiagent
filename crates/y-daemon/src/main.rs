#![forbid(unsafe_code)]
mod ipc;
mod web;

use std::sync::Arc;
use tracing::{error, info};
use y_arena::{WorkingArena, DEFAULT_IDLE_HYSTERESIS};
use y_reactor::SessionReactor;
use y_storage::StorageEngine;
use y_types::SessionId;

/// Resolved per-user state directory, so the database never lands in whatever
/// directory the daemon happened to be launched from.
fn state_dir() -> std::path::PathBuf {
    let base = if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
    } else {
        std::env::var_os("XDG_STATE_HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                std::env::var_os("HOME")
                    .map(|home| std::path::PathBuf::from(home).join(".local/state"))
                    .unwrap_or_else(std::env::temp_dir)
            })
    };
    base.join("y-code")
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().with_env_filter("info").init();

    y_gateway::load_dotenv(".env")?;

    if std::env::var("OPENROUTER_API_KEY").is_ok() {
        info!("OPENROUTER_API_KEY detected - live model mode enabled for /api/turn");
    } else {
        info!("No OPENROUTER_API_KEY - /api/turn will report configuration missing");
    }

    info!("Initializing Y Code Resident Background Daemon (y-daemon)...");

    let dir = state_dir();
    std::fs::create_dir_all(&dir)?;
    let db_path = dir.join("y_code_state.db");
    info!("Daemon state directory: {}", dir.display());

    let arena = Arc::new(WorkingArena::with_defaults());
    let watchdog = arena.spawn_idle_watchdog(DEFAULT_IDLE_HYSTERESIS / 10);

    let storage = Arc::new(StorageEngine::with_defaults(&db_path)?);

    let session_id = SessionId::new();
    let reactor = Arc::new(tokio::sync::Mutex::new(SessionReactor::new(
        session_id,
        arena.clone(),
        storage.clone(),
    )));

    info!(
        "Y Code Session Reactor initialized. Session ID: {}",
        session_id
    );

    #[cfg(windows)]
    let ipc = ipc::run_windows_ipc_server(reactor.clone());
    #[cfg(not(windows))]
    let ipc = ipc::run_unix_ipc_server(reactor.clone());
    let result = tokio::select! {
        result = ipc => result,
        result = web::run_web_server(4040, reactor, arena, storage.clone()) => result,
        result = tokio::signal::ctrl_c() => result.map_err(|e| e.into()),
    };
    if let Some(handle) = watchdog {
        handle.abort();
    }
    storage.shutdown();
    if storage.is_degraded() {
        error!(
            evicted_deltas = storage.evicted_deltas(),
            "Daemon shut down with storage degraded; some turns were not persisted."
        );
    }
    result
}
