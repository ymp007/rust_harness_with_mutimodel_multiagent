use parking_lot::Mutex;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
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

/// Tier 0 In-Memory RAM Staging Ring for bounded state delta recording.
///
/// The ring never grows past `capacity`. Overwrites are counted rather than
/// silently discarded, so callers can observe that data was dropped.
pub struct StagingRing {
    queue: Mutex<VecDeque<StagedDelta>>,
    capacity: usize,
    evicted: AtomicUsize,
}

impl StagingRing {
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "StagingRing capacity must be > 0");
        Self {
            queue: Mutex::new(VecDeque::with_capacity(capacity)),
            capacity,
            evicted: AtomicUsize::new(0),
        }
    }

    pub fn with_default_capacity() -> Self {
        Self::new(DEFAULT_STAGING_CAPACITY)
    }

    /// Stages a turn summary into the in-memory RAM buffer.
    /// No disk I/O; overwrites the oldest delta when full and counts the loss.
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
            queue.pop_front();
            self.evicted.fetch_add(1, Ordering::Relaxed);
        }
        queue.push_back(delta);
    }

    /// Drains all currently staged deltas.
    pub fn drain_all(&self) -> Vec<StagedDelta> {
        let mut queue = self.queue.lock();
        queue.drain(..).collect()
    }

    /// Number of deltas dropped because the ring was full.
    pub fn evicted(&self) -> usize {
        self.evicted.load(Ordering::Relaxed)
    }

    pub fn len(&self) -> usize {
        self.queue.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.queue.lock().is_empty()
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary() -> TurnSummary {
        TurnSummary {
            turn_id: TurnId::new(),
            session_id: SessionId::new(),
            prompt_tokens: 1,
            completion_tokens: 1,
            wall_time_ns: 1,
            harness_overhead_ns: 1,
            tool_invocations: 0,
        }
    }

    #[test]
    fn zero_capacity_is_rejected() {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let result = std::panic::catch_unwind(|| StagingRing::new(0));
        std::panic::set_hook(previous);
        assert!(result.is_err());
    }

    #[test]
    fn overwrites_are_counted_not_silent() {
        let ring = StagingRing::new(2);
        for _ in 0..5 {
            ring.stage(summary());
        }
        assert_eq!(ring.len(), 2, "ring must stay bounded");
        assert_eq!(ring.evicted(), 3);
    }
}
