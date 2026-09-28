use crate::id::{SessionId, TurnId};
use bytes::Bytes;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
    pub name: Option<String>,
    pub tool_call_id: Option<String>,
}

impl Message {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: Role::System,
            content: content.into(),
            name: None,
            tool_call_id: None,
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: content.into(),
            name: None,
            tool_call_id: None,
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: content.into(),
            name: None,
            tool_call_id: None,
        }
    }

    pub fn tool(content: impl Into<String>, tool_call_id: impl Into<String>) -> Self {
        Self {
            role: Role::Tool,
            content: content.into(),
            name: None,
            tool_call_id: Some(tool_call_id.into()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TokenChunk {
    pub text: Bytes,
    pub token_id: u32,
    pub is_first_token: bool,
    pub timestamp_ns: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionState {
    pub session_id: SessionId,
    pub turn_count: u64,
    pub total_prompt_tokens: u64,
    pub total_completion_tokens: u64,
    pub is_active: bool,
}

impl SessionState {
    pub fn new(session_id: SessionId) -> Self {
        Self {
            session_id,
            turn_count: 0,
            total_prompt_tokens: 0,
            total_completion_tokens: 0,
            is_active: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnSummary {
    pub turn_id: TurnId,
    pub session_id: SessionId,
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub wall_time_ns: u64,
    pub harness_overhead_ns: u64,
    pub tool_invocations: u32,
}
