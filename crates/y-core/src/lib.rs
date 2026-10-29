#![forbid(unsafe_code)]
use std::path::Path;
use std::sync::Arc;
use tracing::info;

pub use y_arena::{WorkingArena, DEFAULT_IDLE_HYSTERESIS};
pub use y_cache::RadixPrefixCache;
pub use y_gateway::{ShadowWorkerSupervisor, SseStreamParser, StreamCancellationToken};
pub use y_mcp::{McpTool, ToolOutputSanitizer};
pub use y_mesh::ActorMesh;
pub use y_reactor::{SessionReactor, TurnFsm, TurnState};
pub use y_sandbox::{Tier1WasmSandbox, Tier2OsSandbox};
pub use y_shm::ShmRingBuffer;
pub use y_storage::StorageEngine;
pub use y_telemetry::{HardwareEnergyMeter, LebesgueDagCalculator};
pub use y_types::{AgentError, AgentLifecycleHook, Message, SessionId, TurnId, TurnSummary};

/// The unified Y Code Engine facade.
/// Orchestrates memory arenas, storage, sandboxing, and inference gateways.
pub struct YCodeEngine {
    arena: Arc<WorkingArena>,
    storage: Arc<StorageEngine>,
    cache: Arc<RadixPrefixCache>,
    mesh: Arc<ActorMesh>,
    telemetry: Arc<HardwareEnergyMeter>,
    watchdog: Option<tokio::task::JoinHandle<()>>,
}

impl YCodeEngine {
    pub fn new(db_path: impl AsRef<Path>) -> Result<Self, AgentError> {
        info!("Initializing Y Code Core Engine...");

        let arena = Arc::new(WorkingArena::with_defaults());
        let storage = Arc::new(StorageEngine::with_defaults(db_path)?);
        let cache = Arc::new(RadixPrefixCache::new());
        let mesh = Arc::new(ActorMesh::new());
        let telemetry = Arc::new(HardwareEnergyMeter::new());

        // Idle watchdog. Absent when no Tokio runtime is active, which is a
        // supported state for this synchronous constructor.
        let watchdog = arena.spawn_idle_watchdog(DEFAULT_IDLE_HYSTERESIS / 10);

        Ok(Self {
            arena,
            storage,
            cache,
            mesh,
            telemetry,
            watchdog,
        })
    }

    /// Spawns a new isolated session reactor.
    pub fn create_session(&self, session_id: SessionId) -> SessionReactor {
        SessionReactor::new(session_id, self.arena.clone(), self.storage.clone())
    }

    pub fn arena(&self) -> Arc<WorkingArena> {
        self.arena.clone()
    }

    pub fn storage(&self) -> Arc<StorageEngine> {
        self.storage.clone()
    }

    pub fn cache(&self) -> Arc<RadixPrefixCache> {
        self.cache.clone()
    }

    pub fn mesh(&self) -> Arc<ActorMesh> {
        self.mesh.clone()
    }

    pub fn telemetry(&self) -> Arc<HardwareEnergyMeter> {
        self.telemetry.clone()
    }

    pub fn shutdown(&self) {
        if let Some(handle) = &self.watchdog {
            handle.abort();
        }
        self.storage.shutdown();
        if self.storage.is_degraded() {
            tracing::error!(
                evicted_deltas = self.storage.evicted_deltas(),
                "Y Code Core Engine shut down with storage degraded; some turns may be missing."
            );
        } else {
            info!("Y Code Core Engine cleanly shut down.");
        }
    }
}
