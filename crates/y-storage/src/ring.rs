use parking_lot::Mutex;
use std::collections::VecDeque;
use std::time::Instant;
use y_types::{SessionId, TurnId, TurnSummary};

pub const DEFAULT_STAGING_CAPACITY: usize = 1024;

#[derive(Debug, Clone)]
pub struct StagedDelta {
    pub session_id: SessionId,
    pub turn_id: TurnId,
    pub staged_at: Instant,
    pub summary: TurnSummary,
}

/// Tier 0 In-Memory RAM Staging Ring for microsecond state delta recording.
pub struct StagingRing {
    queue: Mutex<VecDeque<StagedDelta>>,
    capacity: usize,
}

impl StagingRing {
    pub fn new(capacity: usize) -> Self {
        Self {
            queue: Mutex::new(VecDeque::with_capacity(capacity)),
            capacity,
        }
    }

    pub fn with_default_capacity() -> Self {
        Self::new(DEFAULT_STAGING_CAPACITY)
    }

    /// Stages a turn summary into the in-memory RAM buffer.
    /// Executes in < 5.0 µs without any disk I/O.
    #[inline(always)]
    pub fn stage(&self, summary: TurnSummary) {
        let delta = StagedDelta {
            session_id: summary.session_id,
            turn_id: summary.turn_id,
            staged_at: Instant::now(),
            summary,
        };

        let mut queue = self.queue.lock();
        if queue.len() >= self.capacity {
            queue.pop_front(); // Evict oldest if buffer is full
        }
        queue.push_back(delta);
    }

    /// Drains all currently staged deltas.
    pub fn drain_all(&self) -> Vec<StagedDelta> {
        let mut queue = self.queue.lock();
        queue.drain(..).collect()
    }

    pub fn len(&self) -> usize {
        self.queue.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.queue.lock().is_empty()
    }
}
