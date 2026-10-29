#![forbid(unsafe_code)]
use tracing::{info, warn};

/// The GPU inference worker is not implemented.
///
/// `y-shm`'s ring is heap memory, not a shared mapping, so a readiness frame
/// written here cannot be observed by another process. Rather than logging a
/// capability that does not exist, this binary reports the gap and exits
/// non-zero so any supervisor fails fast instead of waiting on a handshake that
/// can never arrive.
fn main() -> ! {
    tracing_subscriber::fmt().with_env_filter("info").init();

    info!("y-inference-worker: not implemented.");
    warn!(
        "Out-of-process GPU inference is unavailable. \
         The shared-memory transport in y-shm is in-process heap memory, so no \
         cross-process handshake is possible yet. See CODE_REVIEW_REPORT.md."
    );
    std::process::exit(1);
}
