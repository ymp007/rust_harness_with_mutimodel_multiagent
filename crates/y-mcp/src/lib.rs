#![forbid(unsafe_code)]
pub mod protocol;
pub mod sanitizer;

pub use protocol::{JsonRpcError, JsonRpcRequest, JsonRpcResponse, McpTool};
pub use sanitizer::ToolOutputSanitizer;
