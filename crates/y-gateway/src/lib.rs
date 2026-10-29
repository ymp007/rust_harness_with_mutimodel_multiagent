#![forbid(unsafe_code)]
pub mod cancellation;
pub mod openrouter;
pub mod protocol;
pub use protocol::{ApiEndpoint, ApiFormat};
pub mod sse;
pub mod supervisor;

pub use cancellation::StreamCancellationToken;
pub use openrouter::{
    chat_once, chat_stream, load_dotenv, OpenRouterClient, OpenRouterConfig, OPENROUTER_URL,
};
pub use sse::SseStreamParser;
pub use supervisor::{ShadowWorkerSupervisor, WorkerInstance, WorkerState};
