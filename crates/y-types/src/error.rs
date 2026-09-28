use thiserror::Error;

#[derive(Debug, Error)]
pub enum AgentError {
    #[error("Context limit exceeded: {0} tokens requested > {1} limit")]
    ContextLimitExceeded(usize, usize),

    #[error("Model provider error: {0}")]
    ProviderError(String),

    #[error("Tool execution failed: {0}")]
    ToolExecutionError(String),

    #[error("Internal runtime panic: {0}")]
    InternalError(String),

    #[error("Storage persistence error: {0}")]
    StorageError(String),

    #[error("Daemon IPC error: {0}")]
    IpcError(String),

    #[error("Security violation: {0}")]
    SecurityViolation(String),

    #[error("Resource exhausted: {0}")]
    ResourceExhausted(String),

    #[error("Timeout after {0} ms")]
    Timeout(u64),
}

#[derive(Debug, Error)]
pub enum SecurityError {
    #[error("Untrusted path traversal attempted: {0}")]
    PathTraversalAttempt(String),

    #[error("Memory tampering detected in shared buffer")]
    MemoryTampering,

    #[error("Unauthorized IPC access from SID: {0}")]
    UnauthorizedIpcAccess(String),

    #[error("Command execution restricted by sandbox policy: {0}")]
    CommandRestricted(String),

    #[error("Payload bounds check failed: offset {0} > capacity {1}")]
    OutOfBounds(usize, usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryStrategy {
    RetryWithExponentialBackoff,
    FallbackToLocalModel,
    CompactContextAndRetry,
    TerminateSession,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorResolution {
    Handled,
    Propagate,
}
