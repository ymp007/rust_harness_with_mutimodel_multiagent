use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, info};

pub const DEFAULT_IDLE_HYSTERESIS: Duration = Duration::from_secs(5);

pub struct HysteresisPurger {
    last_active: Mutex<Instant>,
    idle_threshold: Duration,
    is_purged: AtomicBool,
}

impl HysteresisPurger {
    pub fn new(idle_threshold: Duration) -> Self {
        Self {
            last_active: Mutex::new(Instant::now()),
            idle_threshold,
            is_purged: AtomicBool::new(false),
        }
    }

    pub fn with_default_threshold() -> Self {
        Self::new(DEFAULT_IDLE_HYSTERESIS)
    }

    /// Mark the arena as actively in use, resetting the idle timer.
    #[inline(always)]
    pub fn mark_active(&self) {
        *self.last_active.lock() = Instant::now();
        self.is_purged.store(false, Ordering::Release);
    }

    /// Checks if the arena has been idle longer than the threshold and should be purged.
    pub fn should_purge(&self) -> bool {
        if self.is_purged.load(Ordering::Acquire) {
            return false;
        }
        let elapsed = self.last_active.lock().elapsed();
        elapsed >= self.idle_threshold
    }

    /// Performs the graceful page purge if idle.
    pub fn try_purge(&self) -> bool {
        if self.should_purge() {
            debug!("Arena idle hysteresis reached ({:?}); purging pages to OS", self.idle_threshold);
            
            // Mark as purged
            self.is_purged.store(true, Ordering::Release);
            true
        } else {
            false
        }
    }

    /// Starts an asynchronous background watchdog loop to monitor idle state.
    pub fn spawn_watchdog(self: Arc<Self>, check_interval: Duration) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(check_interval);
            loop {
                interval.tick().await;
                if self.try_purge() {
                    info!("Hysteresis watchdog completed idle memory trim.");
                }
            }
        })
    }
}
