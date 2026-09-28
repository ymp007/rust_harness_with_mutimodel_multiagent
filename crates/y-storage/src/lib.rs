pub mod ring;
pub mod wal;

pub use ring::{StagedDelta, StagingRing, DEFAULT_STAGING_CAPACITY};
pub use wal::{StorageCommand, WalWorker, WAL_CHANNEL_CAPACITY};

use std::path::Path;
use std::sync::Arc;
use y_types::{AgentError, TurnSummary};

/// Unified storage subsystem combining Tier 0 RAM staging ring and async SQLite WAL.
pub struct StorageEngine {
    ring: Arc<StagingRing>,
    wal: Arc<WalWorker>,
}

impl StorageEngine {
    pub fn new(db_path: impl AsRef<Path>, ring_capacity: usize) -> Result<Self, AgentError> {
        let ring = Arc::new(StagingRing::new(ring_capacity));
        let wal = Arc::new(WalWorker::start(db_path)?);

        Ok(Self { ring, wal })
    }

    pub fn with_defaults(db_path: impl AsRef<Path>) -> Result<Self, AgentError> {
        Self::new(db_path, DEFAULT_STAGING_CAPACITY)
    }

    /// Records a turn completion:
    /// 1. Stages to RAM in < 5.0 µs
    /// 2. Dispatches to async WAL worker
    #[inline(always)]
    pub fn record_turn(&self, summary: TurnSummary) -> Result<(), AgentError> {
        self.ring.stage(summary.clone());
        self.wal.commit(summary)?;
        Ok(())
    }

    pub fn ring(&self) -> Arc<StagingRing> {
        self.ring.clone()
    }

    pub fn shutdown(&self) {
        self.wal.shutdown();
    }
}
