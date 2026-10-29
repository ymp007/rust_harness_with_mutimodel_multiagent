use parking_lot::Mutex;
use std::sync::Arc;
use tracing::debug;
use y_types::AgentError;

pub const MAX_ACTIVE_HANDLES_CAP: usize = 128;

#[derive(Debug)]
pub struct HandleGuardTable {
    active_count: Arc<Mutex<usize>>,
    max_cap: usize,
}

impl HandleGuardTable {
    pub fn new(max_cap: usize) -> Self {
        Self {
            active_count: Arc::new(Mutex::new(0)),
            max_cap,
        }
    }

    pub fn with_default_cap() -> Self {
        Self::new(MAX_ACTIVE_HANDLES_CAP)
    }

    /// Acquires a slot in the handle guard table. Fails if cap is reached.
    pub fn acquire(&self) -> Result<HandleGuard, AgentError> {
        let mut count = self.active_count.lock();
        if *count >= self.max_cap {
            return Err(AgentError::ResourceExhausted(format!(
                "Handle guard table exceeded maximum capacity of {} active handles",
                self.max_cap
            )));
        }
        *count += 1;
        debug!("Acquired handle slot. Active: {}/{}", *count, self.max_cap);
        Ok(HandleGuard {
            table_count: self.active_count.clone(),
        })
    }

    pub fn active_handles(&self) -> usize {
        *self.active_count.lock()
    }
}

pub struct HandleGuard {
    table_count: Arc<Mutex<usize>>,
}

impl Drop for HandleGuard {
    fn drop(&mut self) {
        let mut count = self.table_count.lock();
        // Surface an imbalanced release in debug builds instead of silently
        // clamping, which would hide the accounting error.
        debug_assert!(
            *count > 0,
            "HandleGuard released without a matching acquire"
        );
        *count = count.saturating_sub(1);
        debug!("Released handle slot. Active: {}", *count);
    }
}
