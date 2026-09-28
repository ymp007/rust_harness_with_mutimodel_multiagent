use crate::error::AgentError;
use crate::id::{SessionId, TurnId};
use crate::message::{Message, SessionState, TokenChunk, TurnSummary};
use std::future::Future;
use std::pin::Pin;

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChunkAction {
    Forward,
    Filter,
}

#[derive(Debug, Clone)]
pub enum TurnAction {
    Proceed,
    Abort(String),
    InjectSystemMessage(String),
}

#[derive(Debug, Clone)]
pub enum ToolAction {
    Allow,
    Deny(String),
    MutateArguments(String),
}

/// 10-Point Zero-Allocation Lifecycle Hook Trait (Dyn-Compatible)
pub trait AgentLifecycleHook: Send + Sync {
    /// 1. Fired during session creation
    fn on_agent_init<'a>(&'a self, _session_id: &'a SessionId) -> BoxFuture<'a, Result<(), AgentError>> {
        Box::pin(async { Ok(()) })
    }

    /// 2. Fired immediately before turn execution begins
    fn on_turn_start<'a>(
        &'a self,
        _turn_id: &'a TurnId,
        _session: &'a SessionState,
    ) -> BoxFuture<'a, Result<TurnAction, AgentError>> {
        Box::pin(async { Ok(TurnAction::Proceed) })
    }

    /// 3. Fired during context window packing & prefix alignment
    fn on_context_assemble<'a>(
        &'a self,
        _messages: &'a mut Vec<Message>,
        _token_budget: usize,
    ) -> BoxFuture<'a, Result<(), AgentError>> {
        Box::pin(async { Ok(()) })
    }

    /// 4. Fired immediately before dispatching request to inference gateway
    fn on_model_request<'a>(
        &'a self,
        _prompt_tokens: &'a [u32],
    ) -> BoxFuture<'a, Result<(), AgentError>> {
        Box::pin(async { Ok(()) })
    }

    /// 5. Synchronous Inline Token Streaming Hook (< 40 ns target)
    fn on_token_chunk(&self, _chunk: &TokenChunk) -> ChunkAction {
        ChunkAction::Forward
    }

    /// 6. Fired when model response completes but before tool routing
    fn on_model_response<'a>(
        &'a self,
        _completion_text: &'a str,
    ) -> BoxFuture<'a, Result<(), AgentError>> {
        Box::pin(async { Ok(()) })
    }

    /// 7. Fired before invoking a tool
    fn on_tool_call<'a>(
        &'a self,
        _tool_name: &'a str,
        _arguments_json: &'a str,
    ) -> BoxFuture<'a, Result<ToolAction, AgentError>> {
        Box::pin(async { Ok(ToolAction::Allow) })
    }

    /// 8. Fired immediately after tool execution completes
    fn on_tool_result<'a>(
        &'a self,
        _tool_name: &'a str,
        _result_output: &'a str,
        _execution_time_ns: u64,
    ) -> BoxFuture<'a, Result<(), AgentError>> {
        Box::pin(async { Ok(()) })
    }

    /// 9. Fired when an unhandled error occurs
    fn on_error<'a>(&'a self, _error: &'a AgentError) -> BoxFuture<'a, Result<(), AgentError>> {
        Box::pin(async { Ok(()) })
    }

    /// 10. Fired upon turn completion after state persist
    fn on_turn_end<'a>(
        &'a self,
        _summary: &'a TurnSummary,
    ) -> BoxFuture<'a, Result<(), AgentError>> {
        Box::pin(async { Ok(()) })
    }
}

/// A default no-op lifecycle hook implementation
#[derive(Debug, Default, Clone, Copy)]
pub struct NoOpLifecycleHook;

impl AgentLifecycleHook for NoOpLifecycleHook {}
