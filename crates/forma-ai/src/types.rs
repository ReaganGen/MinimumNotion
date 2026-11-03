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
    pub messages: Vec<Message>,
    pub tools: Vec<ToolSpec>,
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
