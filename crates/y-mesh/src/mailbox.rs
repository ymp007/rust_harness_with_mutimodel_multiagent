use parking_lot::Mutex;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tracing::warn;
use y_types::AgentError;

pub const DEFAULT_MAILBOX_CAPACITY: usize = 1000;

pub struct MeshMessage<T> {
    pub sender_id: String,
    pub payload: T,
}

/// Credit-Based Backpressure Mailbox.
/// Automatically suspends producers at Whigh (80%) and resumes at Wlow (20%).
pub struct BoundedMailbox<T> {
    queue: Mutex<VecDeque<MeshMessage<T>>>,
    capacity: usize,
    high_watermark: usize,
    low_watermark: usize,
    backpressure_active: Arc<AtomicBool>,
}

impl<T> BoundedMailbox<T> {
    pub fn new(capacity: usize) -> Self {
        let high_watermark = (capacity * 80) / 100;
        let low_watermark = (capacity * 20) / 100;

        Self {
            queue: Mutex::new(VecDeque::with_capacity(capacity)),
            capacity,
            high_watermark,
            low_watermark,
            backpressure_active: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn with_default_capacity() -> Self {
        Self::new(DEFAULT_MAILBOX_CAPACITY)
    }

    /// Enqueues a message, applying credit backpressure if high watermark is reached.
    pub fn send(&self, sender_id: impl Into<String>, payload: T) -> Result<(), AgentError> {
        let mut queue = self.queue.lock();
        if queue.len() >= self.capacity {
            return Err(AgentError::ResourceExhausted(format!(
                "Actor mailbox overflow! Max capacity {} reached.",
                self.capacity
            )));
        }

        queue.push_back(MeshMessage {
            sender_id: sender_id.into(),
            payload,
        });

        if queue.len() >= self.high_watermark && !self.backpressure_active.load(Ordering::Acquire) {
            self.backpressure_active.store(true, Ordering::Release);
            warn!(
                "Mailbox reached high watermark (80% / {} msgs). Backpressure signaled.",
                queue.len()
            );
        }

        Ok(())
    }

    /// Dequeues a message, releasing backpressure when queue drains below low watermark (20%).
    pub fn recv(&self) -> Option<MeshMessage<T>> {
        let mut queue = self.queue.lock();
        let msg = queue.pop_front()?;

        if queue.len() <= self.low_watermark && self.backpressure_active.load(Ordering::Acquire) {
            self.backpressure_active.store(false, Ordering::Release);
        }

        Some(msg)
    }

    pub fn is_backpressured(&self) -> bool {
        self.backpressure_active.load(Ordering::Acquire)
    }

    pub fn len(&self) -> usize {
        self.queue.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.queue.lock().is_empty()
    }
}
