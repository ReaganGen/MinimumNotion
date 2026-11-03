//! What Forma sends to a model and what comes back, independent of any
//! provider's wire format (ARCHITECTURE §9.1).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Role {
    User,
    Assistant,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

/// A tool call the model made. The arguments are JSON text, as streamed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

/// One step of the conversation sent to a model. Each protocol writes
/// these in its own shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Turn {
    User(String),
    Assistant {
        text: String,
        tool_calls: Vec<ToolCall>,
        /// Blocks the provider wants back unchanged with the results of
        /// these calls (Anthropic's thinking blocks).
        reasoning: Vec<serde_json::Value>,
    },
    /// What a tool returned for the call `call_id`.
    ToolResult {
        call_id: String,
        name: String,
        content: String,
        is_error: bool,
    },
}

impl From<Message> for Turn {
    fn from(message: Message) -> Self {
        match message.role {
            Role::User => Turn::User(message.content),
            Role::Assistant => Turn::Assistant {
                text: message.content,
                tool_calls: Vec::new(),
                reasoning: Vec::new(),
            },
        }
    }
}

/// A tool the model may call, with a JSON Schema for its arguments.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChatRequest {
    pub model: String,
    pub system: Option<String>,
    pub messages: Vec<Turn>,
    pub tools: Vec<ToolSpec>,
    /// False on the last round of a tool-using answer: the tools stay
    /// defined, since the conversation already uses them, but the model
    /// must answer.
    pub allow_tool_calls: bool,
    pub max_output_tokens: Option<u32>,
    /// Sent only when set; many models reject it or ignore it.
    pub temperature: Option<f64>,
    /// The context window to allocate. Only Ollama takes one (`num_ctx`).
    pub context_window: Option<u32>,
}

/// One step of a streamed answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatEvent {
    TextDelta(String),
    ToolCallStart {
        id: String,
        name: String,
    },
    ToolCallArgsDelta {
        id: String,
        delta: String,
    },
    ToolCallEnd {
        id: String,
    },
    /// A block to send back with the next request of a tool-using answer.
    Reasoning(serde_json::Value),
    Usage {
        input: Option<u32>,
        output: Option<u32>,
    },
    Done(StopReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum StopReason {
    EndTurn,
    MaxTokens,
    ToolUse,
    ContentFilter,
    Other,
}

/// `Yes`, `No` or `Unknown`. Unknown counts as No for anything that depends
/// on it (ADR-019).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Tri {
    Yes,
    No,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct ModelInfo {
    pub id: String,
    /// A friendlier name, when the provider has one.
    pub name: Option<String>,
    /// Size and quantization for local models, for example "8B · Q4_K_M".
    pub details: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct ModelCapabilities {
    pub tool_calling: Tri,
    /// The most input the model accepts, in tokens, when the provider says.
    pub context_window: Option<u32>,
    pub max_output_tokens: Option<u32>,
}

impl ModelCapabilities {
    pub const UNKNOWN: ModelCapabilities = ModelCapabilities {
        tool_calling: Tri::Unknown,
        context_window: None,
        max_output_tokens: None,
    };
}
