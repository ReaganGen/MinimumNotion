//! The Anthropic Messages API with server-sent events.
//!
//! API version 2026-10-21 (the latest on 2026-11-03) removed `temperature`,
//! `top_p` and `top_k` for every model, so Diaspar never sends them (ADR-038).

use std::collections::HashMap;

use serde_json::{Value, json};

use super::{Decoder, Frame, arguments_object, json, str_at, u32_at};
use crate::error::{AiError, kind_for_code};
use crate::types::{ChatEvent, ChatRequest, ModelCapabilities, ModelInfo, StopReason, Tri, Turn};

pub const API_VERSION: &str = "2026-10-21";
/// `max_tokens` is required; this is used when the user hasn't set one.
pub const DEFAULT_MAX_TOKENS: u32 = 4096;

pub fn chat_url(base: &str) -> String {
    format!("{base}/v1/messages")
}

pub fn models_url(base: &str, after: Option<&str>) -> String {
    match after {
        Some(id) => format!("{base}/v1/models?limit=1000&after_id={}", encode(id)),
        None => format!("{base}/v1/models?limit=1000"),
    }
}

pub fn model_url(base: &str, model: &str) -> String {
    format!("{base}/v1/models/{}", encode(model))
}

/// Model IDs go into the URL; the user may type anything.
fn encode(part: &str) -> String {
    url::form_urlencoded::byte_serialize(part.as_bytes()).collect()
}

/// Tool calls are `tool_use` blocks in the assistant's message; their
/// results are `tool_result` blocks, all in one user message.
fn messages(turns: &[Turn]) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::new();
    for turn in turns {
        match turn {
            Turn::User(text) => out.push(json!({ "role": "user", "content": text })),
            Turn::Assistant {
                text, tool_calls, ..
            } if tool_calls.is_empty() => {
                out.push(json!({ "role": "assistant", "content": text }));
            }
            Turn::Assistant {
                text,
                tool_calls,
                reasoning,
            } => {
                // Thinking first, as the model produced it.
                let mut blocks = reasoning.clone();
                if !text.is_empty() {
                    blocks.push(json!({ "type": "text", "text": text }));
                }
                blocks.extend(tool_calls.iter().map(|c| {
                    json!({ "type": "tool_use", "id": c.id, "name": c.name,
                            "input": arguments_object(&c.arguments) })
                }));
                out.push(json!({ "role": "assistant", "content": blocks }));
            }
            Turn::ToolResult {
                call_id,
                content,
                is_error,
                ..
            } => {
                let block = json!({ "type": "tool_result", "tool_use_id": call_id,
                                    "content": content, "is_error": is_error });
                match out.last_mut() {
                    Some(last) if last["role"] == "user" && last["content"].is_array() => {
                        if let Some(blocks) = last["content"].as_array_mut() {
                            blocks.push(block);
                        }
                    }
                    _ => out.push(json!({ "role": "user", "content": [block] })),
                }
            }
        }
    }
    out
}

pub fn chat_body(req: &ChatRequest) -> Value {
    let messages = messages(&req.messages);
    let mut body = json!({
        "model": req.model,
        "max_tokens": req.max_output_tokens.unwrap_or(DEFAULT_MAX_TOKENS),
        "stream": true,
        "messages": messages,
    });
    if let Some(system) = &req.system {
        body["system"] = json!(system);
    }
    if !req.tools.is_empty() {
        body["tools"] = req
            .tools
            .iter()
            .map(|t| json!({ "name": t.name, "description": t.description, "input_schema": t.parameters }))
            .collect();
        if !req.allow_tool_calls {
            body["tool_choice"] = json!({ "type": "none" });
        }
    }
    body
}

/// One page of `GET /v1/models`: the models and the cursor for the next page.
pub fn parse_models(body: &Value) -> (Vec<ModelInfo>, Option<String>) {
    let models = body["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| {
            Some(ModelInfo {
                id: m["id"].as_str()?.to_string(),
                name: m["display_name"].as_str().map(str::to_owned),
                details: None,
            })
        })
        .collect();
    let next = (body["has_more"].as_bool() == Some(true))
        .then(|| body["last_id"].as_str().map(str::to_owned))
        .flatten();
    (models, next)
}

/// `GET /v1/models/{id}` reports the context window and output limit.
/// Every model on the Messages API can call tools.
pub fn parse_capabilities(body: &Value) -> ModelCapabilities {
    ModelCapabilities {
        tool_calling: Tri::Yes,
        context_window: u32_at(body, "/max_input_tokens").filter(|&n| n > 0),
        max_output_tokens: u32_at(body, "/max_tokens").filter(|&n| n > 0),
    }
}

enum Block {
    Text,
    Tool(String),
    /// Thinking is kept, with its signature, because a tool-using answer
    /// must send it back unchanged.
    Thinking {
        thinking: String,
        signature: String,
    },
    Redacted(Value),
    Other,
}

#[derive(Default)]
pub struct AnthropicDecoder {
    blocks: HashMap<u64, Block>,
    input_tokens: Option<u32>,
    output_tokens: Option<u32>,
    stop: Option<StopReason>,
    done: bool,
}

fn stop_reason(reason: &str) -> StopReason {
    match reason {
        "end_turn" | "stop_sequence" => StopReason::EndTurn,
        "max_tokens" => StopReason::MaxTokens,
        "tool_use" => StopReason::ToolUse,
        "refusal" => StopReason::ContentFilter,
        _ => StopReason::Other,
    }
}

impl Decoder for AnthropicDecoder {
    fn decode(&mut self, frame: Frame) -> Result<Vec<ChatEvent>, AiError> {
        let Frame::Sse(event) = frame else {
            return Ok(Vec::new());
        };
        let data = json(&event.data)?;
        let index = data["index"].as_u64().unwrap_or_default();
        let mut out = Vec::new();
        match data["type"].as_str().unwrap_or_default() {
            "message_start" => {
                self.input_tokens = u32_at(&data, "/message/usage/input_tokens");
            }
            "content_block_start" => {
                let block = &data["content_block"];
                let kind = match block["type"].as_str() {
                    Some("text") => {
                        if let Some(text) = block["text"].as_str().filter(|t| !t.is_empty()) {
                            out.push(ChatEvent::TextDelta(text.to_string()));
                        }
                        Block::Text
                    }
                    Some("tool_use") => {
                        let id = block["id"].as_str().unwrap_or_default().to_string();
                        out.push(ChatEvent::ToolCallStart {
                            id: id.clone(),
                            name: block["name"].as_str().unwrap_or_default().to_string(),
                        });
                        Block::Tool(id)
                    }
                    Some("thinking") => Block::Thinking {
                        thinking: block["thinking"].as_str().unwrap_or_default().to_string(),
                        signature: block["signature"].as_str().unwrap_or_default().to_string(),
                    },
                    Some("redacted_thinking") => Block::Redacted(block.clone()),
                    // Server-tool blocks aren't used.
                    _ => Block::Other,
                };
                self.blocks.insert(index, kind);
            }
            "content_block_delta" => {
                let delta = &data["delta"];
                match (delta["type"].as_str(), self.blocks.get_mut(&index)) {
                    (Some("thinking_delta"), Some(Block::Thinking { thinking, .. })) => {
                        thinking.push_str(delta["thinking"].as_str().unwrap_or_default());
                    }
                    (Some("signature_delta"), Some(Block::Thinking { signature, .. })) => {
                        signature.push_str(delta["signature"].as_str().unwrap_or_default());
                    }
                    (Some("text_delta"), _) => {
                        if let Some(text) = delta["text"].as_str() {
                            out.push(ChatEvent::TextDelta(text.to_string()));
                        }
                    }
                    (Some("input_json_delta"), Some(Block::Tool(id))) => {
                        out.push(ChatEvent::ToolCallArgsDelta {
                            id: id.clone(),
                            delta: delta["partial_json"]
                                .as_str()
                                .unwrap_or_default()
                                .to_string(),
                        });
                    }
                    _ => {}
                }
            }
            "content_block_stop" => match self.blocks.remove(&index) {
                Some(Block::Tool(id)) => out.push(ChatEvent::ToolCallEnd { id }),
                Some(Block::Thinking {
                    thinking,
                    signature,
                }) => out.push(ChatEvent::Reasoning(json!({
                    "type": "thinking", "thinking": thinking, "signature": signature,
                }))),
                Some(Block::Redacted(block)) => out.push(ChatEvent::Reasoning(block)),
                _ => {}
            },
            "message_delta" => {
                if let Some(reason) = str_at(&data, "/delta/stop_reason") {
                    self.stop = Some(stop_reason(reason));
                }
                // Usage in message_delta is cumulative.
                if let Some(n) = u32_at(&data, "/usage/output_tokens") {
                    self.output_tokens = Some(n);
                }
                if let Some(n) = u32_at(&data, "/usage/input_tokens") {
                    self.input_tokens = Some(n);
                }
            }
            "message_stop" => {
                self.done = true;
                out.push(ChatEvent::Usage {
                    input: self.input_tokens,
                    output: self.output_tokens,
                });
                out.push(ChatEvent::Done(self.stop.unwrap_or(StopReason::EndTurn)));
            }
            "error" => {
                let code = str_at(&data, "/error/type").unwrap_or_default();
                let message = str_at(&data, "/error/message").unwrap_or("Unknown error");
                return Err(AiError::new(
                    kind_for_code(code),
                    format!("Anthropic stopped with an error: {message}"),
                ));
            }
            // `ping`, and event types added after this was written.
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
    use crate::types::{ToolCall, ToolSpec};

    fn decode(bytes: &[u8]) -> Result<Vec<ChatEvent>, AiError> {
        decode_every_way(AnthropicDecoder::default, Framing::Sse, bytes)
    }

    #[test]
    fn streams_text_and_usage() {
        let events = decode(include_bytes!("../../fixtures/anthropic-text.sse")).unwrap();
        assert_eq!(text(&events), "Hello! 你好。");
        assert_eq!(
            &events[events.len() - 2..],
            &[
                ChatEvent::Usage {
                    input: Some(25),
                    output: Some(15)
                },
                ChatEvent::Done(StopReason::EndTurn)
            ]
        );
    }

    #[test]
    fn streams_tool_calls() {
        let events = decode(include_bytes!("../../fixtures/anthropic-tool.sse")).unwrap();
        let id = "toolu_01T1x1fJ34qAmk2tNTrN7Up6".to_string();
        assert_eq!(text(&events), "Let me search.");
        assert!(events.contains(&ChatEvent::ToolCallStart {
            id: id.clone(),
            name: "search_pages".into()
        }));
        let args: String = events
            .iter()
            .filter_map(|e| match e {
                ChatEvent::ToolCallArgsDelta { id: i, delta } if *i == id => Some(delta.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(
            serde_json::from_str::<Value>(&args).unwrap(),
            json!({"query": "protein design"})
        );
        assert!(events.contains(&ChatEvent::ToolCallEnd { id }));
        // The thinking block, signature and all, goes back with the result.
        assert!(events.contains(&ChatEvent::Reasoning(json!({
            "type": "thinking",
            "thinking": "The user wants notes about protein design.",
            "signature": "EqQBCgIYAhIM1gbcDa9GJwZA2b3hGgxBdjrkzLoky3dl1pkiMOYds",
        }))));
        assert_eq!(events.last(), Some(&ChatEvent::Done(StopReason::ToolUse)));
    }

    #[test]
    fn an_error_event_fails_the_stream() {
        let err = decode(include_bytes!("../../fixtures/anthropic-error.sse")).unwrap_err();
        assert_eq!(err.kind, AiErrorKind::Provider);
        assert!(err.message.contains("Overloaded"));
    }

    #[test]
    fn a_stream_that_stops_early_is_interrupted() {
        let full = include_bytes!("../../fixtures/anthropic-text.sse");
        let cut = &full[..full.len() / 2];
        assert_eq!(decode(cut).unwrap_err().kind, AiErrorKind::Interrupted);
    }

    #[test]
    fn the_body_has_no_temperature_and_a_max_tokens() {
        let body = chat_body(&ChatRequest {
            model: "claude-x".into(),
            system: Some("Be brief.".into()),
            messages: vec![
                Turn::User("Hi".into()),
                Turn::Assistant {
                    text: "Let me look.".into(),
                    reasoning: vec![
                        json!({"type": "thinking", "thinking": "Hmm.", "signature": "sig"}),
                    ],
                    tool_calls: vec![
                        ToolCall {
                            id: "toolu_1".into(),
                            name: "search_pages".into(),
                            arguments: r#"{"query":"x"}"#.into(),
                        },
                        ToolCall {
                            id: "toolu_2".into(),
                            name: "list_pages".into(),
                            arguments: "not json".into(),
                        },
                    ],
                },
                Turn::ToolResult {
                    call_id: "toolu_1".into(),
                    name: "search_pages".into(),
                    content: "found".into(),
                    is_error: false,
                },
                Turn::ToolResult {
                    call_id: "toolu_2".into(),
                    name: "list_pages".into(),
                    content: "bad".into(),
                    is_error: true,
                },
            ],
            tools: vec![ToolSpec {
                name: "t".into(),
                description: "d".into(),
                parameters: json!({"type": "object"}),
            }],
            allow_tool_calls: false,
            max_output_tokens: None,
            temperature: Some(0.5),
            context_window: None,
        });
        assert_eq!(body["max_tokens"], DEFAULT_MAX_TOKENS);
        assert_eq!(body["system"], "Be brief.");
        assert_eq!(
            body["messages"][0],
            json!({"role": "user", "content": "Hi"})
        );
        assert_eq!(
            body["messages"][1]["content"],
            json!([
                {"type": "thinking", "thinking": "Hmm.", "signature": "sig"},
                {"type": "text", "text": "Let me look."},
                {"type": "tool_use", "id": "toolu_1", "name": "search_pages", "input": {"query": "x"}},
                {"type": "tool_use", "id": "toolu_2", "name": "list_pages", "input": {}},
            ])
        );
        // Both results go back in one user message.
        assert_eq!(body["messages"].as_array().unwrap().len(), 3);
        assert_eq!(
            body["messages"][2],
            json!({"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "toolu_1", "content": "found", "is_error": false},
                {"type": "tool_result", "tool_use_id": "toolu_2", "content": "bad", "is_error": true},
            ]})
        );
        assert_eq!(body["tools"][0]["input_schema"], json!({"type": "object"}));
        // The last round keeps the tools defined but lets the model only answer.
        assert_eq!(body["tool_choice"], json!({"type": "none"}));
        assert!(body.get("temperature").is_none());
    }

    #[test]
    fn models_and_capabilities_are_read() {
        let (models, next) = parse_models(&json!({
            "data": [{"type": "model", "id": "claude-a", "display_name": "Claude A"}],
            "has_more": true, "first_id": "claude-a", "last_id": "claude-a"
        }));
        assert_eq!(models[0].name.as_deref(), Some("Claude A"));
        assert_eq!(next.as_deref(), Some("claude-a"));
        let caps = parse_capabilities(
            &json!({"id": "claude-a", "max_input_tokens": 200000, "max_tokens": 64000}),
        );
        assert_eq!(caps.context_window, Some(200_000));
        assert_eq!(caps.max_output_tokens, Some(64_000));
        assert_eq!(
            model_url("https://api.anthropic.com", "../admin?x"),
            "https://api.anthropic.com/v1/models/..%2Fadmin%3Fx"
        );
    }
}
