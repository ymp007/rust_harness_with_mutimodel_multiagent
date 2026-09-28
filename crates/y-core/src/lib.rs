use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tracing::info;

pub use y_arena::WorkingArena;
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
}

impl YCodeEngine {
    pub fn new(db_path: impl AsRef<Path>) -> Result<Self, AgentError> {
        info!("Initializing Y Code Core Engine...");

        let arena = Arc::new(WorkingArena::with_defaults());
        let storage = Arc::new(StorageEngine::with_defaults(db_path)?);
        let cache = Arc::new(RadixPrefixCache::new());
        let mesh = Arc::new(ActorMesh::new());
        let telemetry = Arc::new(HardwareEnergyMeter::new());

        // Spawn memory hysteresis watchdog
        arena.purger().spawn_watchdog(Duration::from_millis(500));

        Ok(Self {
            arena,
            storage,
            cache,
            mesh,
            telemetry,
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
        self.storage.shutdown();
        info!("Y Code Core Engine cleanly shut down.");
    }
}
