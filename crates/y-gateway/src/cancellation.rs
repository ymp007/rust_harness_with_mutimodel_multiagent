use parking_lot::Mutex;
use std::future::poll_fn;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll, Waker};
use tracing::debug;

/// Cooperative cancellation for a streaming request.
///
/// `cancel()` marks the flag and wakes any task awaiting `cancelled()`. The
/// token does not emit an HTTP/2 frame itself: dropping the in-flight `reqwest`
/// response body is what actually stops token generation, so a caller must
/// combine `cancel()` with abandoning the stream.
#[derive(Clone)]
pub struct StreamCancellationToken {
    is_cancelled: Arc<AtomicBool>,
    waker: Arc<Mutex<Option<Waker>>>,
}

impl StreamCancellationToken {
    pub fn new() -> Self {
        Self {
            is_cancelled: Arc::new(AtomicBool::new(false)),
            waker: Arc::new(Mutex::new(None)),
        }
    }

    /// Marks the stream cancelled and wakes a pending `cancelled()` await.
    pub fn cancel(&self) {
        if !self.is_cancelled.swap(true, Ordering::AcqRel) {
            if let Some(waker) = self.waker.lock().take() {
                waker.wake();
            }
            debug!("Cancellation requested; drop the response body to halt generation.");
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.is_cancelled.load(Ordering::Acquire)
    }

    /// Resolves as soon as `cancel()` is called.
    pub async fn cancelled(&self) {
        poll_fn(|cx: &mut Context<'_>| {
            if self.is_cancelled() {
                return Poll::Ready(());
            }
            *self.waker.lock() = Some(cx.waker().clone());
            // Re-check after registering to close the race with `cancel()`.
            if self.is_cancelled() {
                return Poll::Ready(());
            }
            Poll::Pending
        })
        .await
    }
}

impl Default for StreamCancellationToken {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use std::time::Duration;

    #[tokio::test]
    async fn cancelled_resolves_after_cancel() {
        let token = StreamCancellationToken::new();
        let observed = Arc::new(AtomicUsize::new(0));

        let observer = {
            let token = token.clone();
            let observed = observed.clone();
            tokio::spawn(async move {
                token.cancelled().await;
                observed.fetch_add(1, Ordering::SeqCst);
            })
        };

        // Give the observer a chance to register its waker.
        tokio::task::yield_now().await;
        assert!(!token.is_cancelled());
        token.cancel();
        tokio::time::timeout(Duration::from_secs(5), observer)
            .await
            .expect("cancelled() never resolved")
            .unwrap();
        assert_eq!(observed.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn cancel_before_await_still_resolves() {
        let token = StreamCancellationToken::new();
        token.cancel();
        tokio::time::timeout(Duration::from_secs(5), token.cancelled())
            .await
            .expect("cancelled() never resolved for an already-cancelled token");
    }
}
