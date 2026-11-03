//! The OpenAI Responses API, used for OpenAI itself (ADR-038): OpenAI's
//! newest models can call tools only through it. Requests set `store: false`
//! so OpenAI doesn't keep the conversation for later retrieval.

use std::collections::HashMap;

use serde_json::{Value, json};

use super::{Decoder, Frame, json, role, str_at, u32_at};
use crate::error::{AiError, kind_for_code};
use crate::types::{ChatEvent, ChatRequest, ModelInfo, StopReason};

pub fn chat_url(base: &str) -> String {
    format!("{base}/responses")
}

pub fn models_url(base: &str) -> String {
    format!("{base}/models")
}

pub fn chat_body(req: &ChatRequest) -> Value {
    let input: Vec<Value> = req
        .messages
        .iter()
        .map(|m| json!({ "role": role(m.role), "content": m.content }))
        .collect();
    let mut body = json!({
        "model": req.model,
        "input": input,
        "stream": true,
        "store": false,
    });
    if let Some(system) = &req.system {
        body["instructions"] = json!(system);
    }
    if let Some(n) = req.max_output_tokens {
        // The API's minimum.
        body["max_output_tokens"] = json!(n.max(16));
    }
    if let Some(t) = req.temperature {
        body["temperature"] = json!(t);
    }
    if !req.tools.is_empty() {
        body["tools"] = req
            .tools
            .iter()
            .map(|t| {
                json!({
                    "type": "function",
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.parameters,
                })
            })
            .collect();
    }
    body
}

/// Families in `/v1/models` that can't hold a conversation. The list can
/// go stale; the user can still type any model ID.
const NOT_CHAT: &[&str] = &[
    "embedding",
    "tts",
    "whisper",
    "transcribe",
    "dall-e",
    "image",
    "moderation",
    "realtime",
    "audio",
    "sora",
    "davinci",
    "babbage",
    "search",
];

pub fn parse_models(body: &Value) -> Vec<ModelInfo> {
    let mut models: Vec<ModelInfo> = body["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| m["id"].as_str())
        .filter(|id| !NOT_CHAT.iter().any(|f| id.contains(f)))
        .map(|id| ModelInfo {
            id: id.to_string(),
            name: None,
            details: None,
        })
        .collect();
    models.sort_by(|a, b| a.id.cmp(&b.id));
    models
}

#[derive(Default)]
pub struct ResponsesDecoder {
    /// Output item ID → the call ID the model will use in the tool result.
    calls: HashMap<String, String>,
    called_tools: bool,
    done: bool,
}

fn usage(response: &Value) -> ChatEvent {
    ChatEvent::Usage {
        input: u32_at(response, "/usage/input_tokens"),
        output: u32_at(response, "/usage/output_tokens"),
    }
}

fn failure(code: Option<&str>, message: Option<&str>) -> AiError {
    AiError::new(
        kind_for_code(code.unwrap_or_default()),
        format!(
            "OpenAI stopped with an error: {}",
            message.unwrap_or("Unknown error")
        ),
    )
}

impl Decoder for ResponsesDecoder {
    fn decode(&mut self, frame: Frame) -> Result<Vec<ChatEvent>, AiError> {
        let Frame::Sse(event) = frame else {
            return Ok(Vec::new());
        };
        let data = json(&event.data)?;
        let mut out = Vec::new();
        match data["type"].as_str().unwrap_or_default() {
            "response.output_text.delta" | "response.refusal.delta" => {
                if let Some(text) = data["delta"].as_str() {
                    out.push(ChatEvent::TextDelta(text.to_string()));
                }
            }
            "response.output_item.added" if data["item"]["type"] == "function_call" => {
                let item = &data["item"];
                let call = item["call_id"].as_str().unwrap_or_default().to_string();
                if let Some(item_id) = item["id"].as_str() {
                    self.calls.insert(item_id.to_string(), call.clone());
                }
                self.called_tools = true;
                out.push(ChatEvent::ToolCallStart {
                    id: call,
                    name: item["name"].as_str().unwrap_or_default().to_string(),
                });
            }
            "response.function_call_arguments.delta" => {
                let item = data["item_id"].as_str().unwrap_or_default();
                if let Some(call) = self.calls.get(item) {
                    out.push(ChatEvent::ToolCallArgsDelta {
                        id: call.clone(),
                        delta: data["delta"].as_str().unwrap_or_default().to_string(),
                    });
                }
            }
            "response.output_item.done" if data["item"]["type"] == "function_call" => {
                if let Some(call) = data["item"]["call_id"].as_str() {
                    out.push(ChatEvent::ToolCallEnd {
                        id: call.to_string(),
                    });
                }
            }
            "response.completed" => {
                self.done = true;
                out.push(usage(&data["response"]));
                out.push(ChatEvent::Done(if self.called_tools {
                    StopReason::ToolUse
                } else {
                    StopReason::EndTurn
                }));
            }
            "response.incomplete" => {
                self.done = true;
                out.push(usage(&data["response"]));
                let reason = str_at(&data, "/response/incomplete_details/reason");
                out.push(ChatEvent::Done(match reason {
                    Some("max_output_tokens") => StopReason::MaxTokens,
                    Some("content_filter") => StopReason::ContentFilter,
                    _ => StopReason::Other,
                }));
            }
            "response.failed" => {
                return Err(failure(
                    str_at(&data, "/response/error/code"),
                    str_at(&data, "/response/error/message"),
                ));
            }
            "error" => {
                return Err(failure(data["code"].as_str(), data["message"].as_str()));
            }
            _ => {}
        }
        Ok(out)
    }

    fn is_done(&self) -> bool {
        self.done
    }

    fn finish(&mut self) -> Result<Vec<ChatEvent>, AiError> {
        Err(AiError::interrupted())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::AiErrorKind;
    use crate::protocol::Framing;
    use crate::protocol::testing::{decode_every_way, text};
    use crate::types::{Message, Role};

    fn decode(bytes: &[u8]) -> Result<Vec<ChatEvent>, AiError> {
        decode_every_way(ResponsesDecoder::default, Framing::Sse, bytes)
    }

    #[test]
    fn streams_text_and_usage() {
        let events = decode(include_bytes!("../../fixtures/openai-responses-text.sse")).unwrap();
        assert_eq!(text(&events), "Hi there! 你好");
        assert_eq!(
            &events[events.len() - 2..],
            &[
                ChatEvent::Usage {
                    input: Some(36),
                    output: Some(87)
                },
                ChatEvent::Done(StopReason::EndTurn)
            ]
        );
    }

    #[test]
    fn streams_tool_calls() {
        let events = decode(include_bytes!("../../fixtures/openai-responses-tool.sse")).unwrap();
        assert_eq!(
            events,
            vec![
                ChatEvent::ToolCallStart {
                    id: "call_k3q".into(),
                    name: "search_pages".into()
                },
                ChatEvent::ToolCallArgsDelta {
                    id: "call_k3q".into(),
                    delta: "{\"query\":".into()
                },
                ChatEvent::ToolCallArgsDelta {
                    id: "call_k3q".into(),
                    delta: "\"protein design\"}".into()
                },
                ChatEvent::ToolCallEnd {
                    id: "call_k3q".into()
                },
                ChatEvent::Usage {
                    input: Some(80),
                    output: Some(19)
                },
                ChatEvent::Done(StopReason::ToolUse),
            ]
        );
    }

    #[test]
    fn errors_and_truncation() {
        let err = decode(include_bytes!("../../fixtures/openai-responses-failed.sse")).unwrap_err();
        assert_eq!(err.kind, AiErrorKind::RateLimited);
        assert!(err.message.contains("Rate limit reached"));

        let events = decode(include_bytes!(
            "../../fixtures/openai-responses-incomplete.sse"
        ))
        .unwrap();
        assert_eq!(events.last(), Some(&ChatEvent::Done(StopReason::MaxTokens)));
        assert_eq!(text(&events), "The answer is");
    }

    #[test]
    fn the_body_never_asks_openai_to_store_the_conversation() {
        let body = chat_body(&ChatRequest {
            model: "gpt-x".into(),
            system: Some("Be brief.".into()),
            messages: vec![Message {
                role: Role::User,
                content: "Hi".into(),
            }],
            tools: vec![],
            max_output_tokens: Some(4),
            temperature: None,
            context_window: None,
        });
        assert_eq!(body["store"], false);
        assert_eq!(body["instructions"], "Be brief.");
        assert_eq!(body["max_output_tokens"], 16);
        assert!(body.get("temperature").is_none());
        assert!(body.get("tools").is_none());
    }

    #[test]
    fn models_skip_families_that_cant_chat() {
        let models = parse_models(&json!({"object": "list", "data": [
            {"id": "gpt-6-astra", "object": "model", "owned_by": "openai"},
            {"id": "text-embedding-3-small", "object": "model"},
            {"id": "gpt-4o-mini-tts", "object": "model"},
            {"id": "gpt-5.5", "object": "model"}
        ]}));
        let ids: Vec<_> = models.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, ["gpt-5.5", "gpt-6-astra"]);
    }
}
