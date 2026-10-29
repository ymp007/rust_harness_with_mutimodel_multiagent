use parking_lot::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, info};

use crate::BumpArena;

pub const DEFAULT_IDLE_HYSTERESIS: Duration = Duration::from_secs(5);

/// Tracks idle time and releases the arena's buffer back to the allocator.
///
/// The purge itself lives in `BumpArena::reset_and_reclaim`; this type only
/// decides *when*. Check and act happen under one lock so a concurrent
/// `mark_active` cannot be overtaken between the two.
pub struct HysteresisPurger {
    state: Mutex<PurgeState>,
    idle_threshold: Duration,
    purged_rounds: AtomicUsize,
}

struct PurgeState {
    last_active: Instant,
    is_purged: bool,
}

impl HysteresisPurger {
    pub fn new(idle_threshold: Duration) -> Self {
        Self {
            state: Mutex::new(PurgeState {
                last_active: Instant::now(),
                is_purged: false,
            }),
            idle_threshold,
            purged_rounds: AtomicUsize::new(0),
        }
    }

    pub fn with_default_threshold() -> Self {
        Self::new(DEFAULT_IDLE_HYSTERESIS)
    }

    /// Marks the arena as actively in use, resetting the idle timer.
    #[inline(always)]
    pub fn mark_active(&self) {
        let mut state = self.state.lock();
        state.last_active = Instant::now();
        state.is_purged = false;
    }

    /// True when the arena has been idle at least as long as the threshold.
    pub fn should_purge(&self) -> bool {
        let state = self.state.lock();
        !state.is_purged && state.last_active.elapsed() >= self.idle_threshold
    }

    /// Number of times the arena has actually been reclaimed.
    pub fn purge_count(&self) -> usize {
        self.purged_rounds.load(Ordering::Relaxed)
    }

    /// Reclaims the arena buffer if idle. Check and act are one critical
    /// section, so a live arena cannot be purged underneath a caller.
    pub fn try_purge(&self, arena: &parking_lot::Mutex<BumpArena>) -> bool {
        {
            let mut state = self.state.lock();
            if state.is_purged || state.last_active.elapsed() < self.idle_threshold {
                return false;
            }
            state.is_purged = true;
        }
        // Reset outside the state lock: the arena owns its own cursor.
        arena.lock().reset_and_reclaim();
        let rounds = self.purged_rounds.fetch_add(1, Ordering::Relaxed) + 1;
        info!("Hysteresis watchdog released arena pages to the OS (round {rounds})");
        true
    }

    /// Starts the background watchdog loop. Returns `None` when no Tokio runtime
    /// is active or the interval is invalid, rather than panicking, because
    /// `YCodeEngine::new` is a synchronous constructor.
    pub fn spawn_watchdog(
        self: Arc<Self>,
        check_interval: Duration,
        arena: Arc<parking_lot::Mutex<BumpArena>>,
    ) -> Option<tokio::task::JoinHandle<()>> {
        if check_interval.is_zero() {
            debug!("Hysteresis watchdog not started: interval must be greater than zero.");
            return None;
        }
        // Check for a runtime explicitly: tokio::spawn panics without one.
        let handle = tokio::runtime::Handle::try_current().ok()?;
        Some(handle.spawn(async move {
            let mut interval = tokio::time::interval(check_interval);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                interval.tick().await;
                // A reclaim leaves the arena idle again, so the next purge waits
                // for a fresh idle window.
                self.try_purge(&arena);
            }
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    fn arena(capacity: usize) -> Arc<parking_lot::Mutex<BumpArena>> {
        Arc::new(parking_lot::Mutex::new(BumpArena::new(capacity)))
    }

    #[test]
    fn try_purge_reclaims_an_idle_arena() {
        let arena = arena(1024);
        arena.lock().alloc_raw(64).unwrap();
        let purger = HysteresisPurger::new(Duration::from_millis(0));
        assert!(purger.try_purge(&arena), "idle arena must be reclaimed");
        assert_eq!(purger.purge_count(), 1);
        assert_eq!(arena.lock().allocated_bytes(), 0);
        // A second pass is a no-op until activity is recorded again.
        assert!(!purger.try_purge(&arena));
    }

    #[test]
    fn active_arena_is_not_purged() {
        let arena = arena(1024);
        let purger = HysteresisPurger::new(Duration::from_secs(3600));
        purger.mark_active();
        assert!(!purger.try_purge(&arena));
        assert_eq!(purger.purge_count(), 0);
    }

    #[test]
    fn spawn_watchdog_outside_a_runtime_does_not_panic() {
        // A synchronous constructor must not abort the process.
        let purger = Arc::new(HysteresisPurger::with_default_threshold());
        let arena = arena(1024);
        let started = thread::spawn(move || {
            purger.spawn_watchdog(Duration::from_millis(100), arena);
        });
        assert!(
            started.join().is_ok(),
            "spawn_watchdog panicked off-runtime"
        );
    }

    #[tokio::test]
    async fn zero_interval_is_rejected_without_panicking() {
        let purger = Arc::new(HysteresisPurger::with_default_threshold());
        // Must not panic or spawn.
        let _ = purger.spawn_watchdog(Duration::ZERO, arena(1024));
    }
}
