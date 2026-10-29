#![forbid(unsafe_code)]
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
    /// 1. Stages to the bounded RAM ring first, so the delta survives WAL pressure
    /// 2. Dispatches to the async WAL worker
    #[inline(always)]
    pub fn record_turn(&self, summary: TurnSummary) -> Result<(), AgentError> {
        self.ring.stage(summary.clone());
        self.wal.commit(summary)
    }

    /// True once any persist has failed or the worker has stopped.
    pub fn is_degraded(&self) -> bool {
        !self.wal.is_running() || self.wal.has_error()
    }

    /// True when the worker has stopped, whether or not it stopped cleanly.
    /// Use this to assert that every acknowledged turn reached disk.
    pub fn persist_failed(&self) -> bool {
        self.wal.has_error()
    }

    pub fn evicted_deltas(&self) -> usize {
        self.ring.evicted()
    }

    /// Number of turns currently staged in the RAM fallback ring.
    pub fn staged_deltas(&self) -> usize {
        self.ring.len()
    }

    pub fn ring(&self) -> Arc<StagingRing> {
        self.ring.clone()
    }

    pub fn shutdown(&self) {
        self.wal.shutdown();
    }
}
