use parking_lot::Mutex;
use std::sync::Arc;
use tracing::debug;
use y_types::AgentError;

pub const MAX_ACTIVE_HANDLES_CAP: usize = 128;

#[derive(Debug)]
pub struct HandleGuardTable {
    active_count: Mutex<usize>,
    max_cap: usize,
}

impl HandleGuardTable {
    pub fn new(max_cap: usize) -> Self {
        Self {
            active_count: Mutex::new(0),
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
            table_count: Arc::new(self.active_count_ptr()),
        })
    }

    pub fn active_handles(&self) -> usize {
        *self.active_count.lock()
    }

    fn active_count_ptr(&self) -> HandleCounter {
        HandleCounter {
            count: &self.active_count as *const Mutex<usize> as usize,
        }
    }
}

// Internal pointer wrapper to decrement count on drop safely
#[derive(Clone)]
struct HandleCounter {
    count: usize,
}

unsafe impl Send for HandleCounter {}
unsafe impl Sync for HandleCounter {}

pub struct HandleGuard {
    table_count: Arc<HandleCounter>,
}

impl Drop for HandleGuard {
    fn drop(&mut self) {
        let ptr = self.table_count.count as *const Mutex<usize>;
        unsafe {
            let mutex = &*ptr;
            let mut count = mutex.lock();
            if *count > 0 {
                *count -= 1;
                debug!("Released handle slot. Active: {}", *count);
            }
        }
    }
}
