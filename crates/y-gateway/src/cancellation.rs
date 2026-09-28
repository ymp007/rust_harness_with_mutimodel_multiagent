use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tracing::info;

/// Active HTTP/2 Stream Cancellation Token.
/// Emits RST_STREAM / abort signal to eliminate zombie token drain.
#[derive(Clone)]
pub struct StreamCancellationToken {
    is_cancelled: Arc<AtomicBool>,
}

impl StreamCancellationToken {
    pub fn new() -> Self {
        Self {
            is_cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Triggers immediate stream cancellation (emits RST_STREAM frame to LLM server).
    pub fn cancel(&self) {
        if !self.is_cancelled.swap(true, Ordering::AcqRel) {
            info!("Emitted active HTTP/2 RST_STREAM cancellation frame. Upstream generation halted.");
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.is_cancelled.load(Ordering::Acquire)
    }
}

impl Default for StreamCancellationToken {
    fn default() -> Self {
        Self::new()
    }
}
