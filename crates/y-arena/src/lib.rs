#![forbid(unsafe_code)]
pub mod bump;
pub mod purge;

pub use bump::{BumpArena, ARENA_ALIGNMENT, DEFAULT_ARENA_CAPACITY};
pub use purge::{HysteresisPurger, DEFAULT_IDLE_HYSTERESIS};

use parking_lot::Mutex;
use std::sync::Arc;
use std::time::Duration;

/// High-level working memory arena for Y Code sessions.
///
/// The bump arena sits behind a mutex so the background watchdog can reclaim it
/// without holding the outer arena. Locking is per turn-staging allocation, not
/// per token, so the cost is not on the hot path.
pub struct WorkingArena {
    bump: Arc<Mutex<BumpArena>>,
    purger: Arc<HysteresisPurger>,
}

impl WorkingArena {
    pub fn new(capacity: usize, idle_threshold: Duration) -> Self {
        Self {
            bump: Arc::new(Mutex::new(BumpArena::new(capacity))),
            purger: Arc::new(HysteresisPurger::new(idle_threshold)),
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(DEFAULT_ARENA_CAPACITY, DEFAULT_IDLE_HYSTERESIS)
    }

    /// Allocates a zeroed, aligned raw byte slice and marks the arena active.
    #[inline(always)]
    pub fn alloc(&mut self, size: usize) -> Option<Vec<u8>> {
        self.purger.mark_active();
        self.bump.lock().alloc_raw(size).map(|s| s.to_vec())
    }

    /// Resets the arena for the next turn.
    #[inline(always)]
    pub fn reset_turn(&mut self) {
        self.bump.lock().reset();
        self.purger.mark_active();
    }

    /// Shared handle to the bump arena, used by the idle watchdog.
    pub fn bump(&self) -> Arc<Mutex<BumpArena>> {
        self.bump.clone()
    }

    pub fn purger(&self) -> Arc<HysteresisPurger> {
        self.purger.clone()
    }

    /// Starts the idle watchdog. A no-op when no Tokio runtime is active.
    pub fn spawn_idle_watchdog(
        self: &Arc<Self>,
        check_interval: Duration,
    ) -> Option<tokio::task::JoinHandle<()>> {
        self.purger
            .clone()
            .spawn_watchdog(check_interval, self.bump.clone())
    }

    pub fn allocated_bytes(&self) -> usize {
        self.bump.lock().allocated_bytes()
    }

    pub fn capacity(&self) -> usize {
        self.bump.lock().capacity()
    }
}

impl Default for WorkingArena {
    fn default() -> Self {
        Self::with_defaults()
    }
}
