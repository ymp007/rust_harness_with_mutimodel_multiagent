pub mod bump;
pub mod purge;

pub use bump::{BumpArena, DEFAULT_ARENA_CAPACITY};
pub use purge::{HysteresisPurger, DEFAULT_IDLE_HYSTERESIS};

use std::sync::Arc;
use std::time::Duration;

/// High-level working memory arena for Y Code sessions.
pub struct WorkingArena {
    bump: BumpArena,
    purger: Arc<HysteresisPurger>,
}

impl WorkingArena {
    pub fn new(capacity: usize, idle_threshold: Duration) -> Self {
        Self {
            bump: BumpArena::new(capacity),
            purger: Arc::new(HysteresisPurger::new(idle_threshold)),
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(DEFAULT_ARENA_CAPACITY, DEFAULT_IDLE_HYSTERESIS)
    }

    /// Allocates raw byte slice and marks the arena active.
    #[inline(always)]
    pub fn alloc(&self, size: usize) -> Option<&mut [u8]> {
        self.purger.mark_active();
        self.bump.alloc_raw(size)
    }

    /// Resets the bump pointer for the next turn.
    #[inline(always)]
    pub fn reset_turn(&self) {
        self.bump.reset();
        self.purger.mark_active();
    }

    pub fn purger(&self) -> Arc<HysteresisPurger> {
        self.purger.clone()
    }

    pub fn allocated_bytes(&self) -> usize {
        self.bump.allocated_bytes()
    }

    pub fn capacity(&self) -> usize {
        self.bump.capacity()
    }
}

impl Default for WorkingArena {
    fn default() -> Self {
        Self::with_defaults()
    }
}
