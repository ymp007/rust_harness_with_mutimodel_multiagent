#![forbid(unsafe_code)]
pub mod error;
pub mod hook;
pub mod id;
pub mod message;

pub use error::{AgentError, ErrorResolution, RecoveryStrategy, SecurityError};
pub use hook::{AgentLifecycleHook, ChunkAction, NoOpLifecycleHook, ToolAction, TurnAction};
pub use id::{SessionId, TurnId};
pub use message::{Message, Role, SessionState, TokenChunk, TurnSummary};
