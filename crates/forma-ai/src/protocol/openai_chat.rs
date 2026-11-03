//! The OpenAI Chat Completions protocol, used by OpenAI-compatible servers
//! and LM Studio. LM Studio's own REST API adds model details.

use std::collections::BTreeMap;
use std::collections::btree_map::Entry;

use serde_json::{Value, json};

use super::{Decoder, Frame, json, role, u32_at};
use crate::error::{AiError, kind_for_code};
use crate::types::{ChatEvent, ChatRequest, ModelCapabilities, ModelInfo, StopReason, Tri};

pub fn chat_url(base: &str) -> String {
    format!("{base}/chat/completions")
}

pub fn models_url(base: &str) -> String {
    format!("{base}/models")
}

/// LM Studio's native model list, at the server root rather than under `/v1`.
pub fn lmstudio_models_url(base: &str) -> String {
    let root = base.strip_suffix("/v1").unwrap_or(base);
    format!("{root}/api/v1/models")
}

pub fn chat_body(req: &ChatRequest) -> Value {
    let mut messages = Vec::new();
    if let Some(system) = &req.system {
        messages.push(json!({ "role": "system", "content": system }));
    }
    messages.extend(
        req.messages
            .iter()
            .map(|m| json!({ "role": role(m.role), "content": m.content })),
    );
    let mut body = json!({
        "model": req.model,
        "messages": messages,
        "stream": true,
        "stream_options": { "include_usage": true },
    });
    // `max_tokens`, not OpenAI's newer `max_completion_tokens`: compatible
    // servers understand the older name.
    if let Some(n) = req.max_output_tokens {
        body["max_tokens"] = json!(n);
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
                    "function": { "name": t.name, "description": t.description, "parameters": t.parameters },
                })
            })
            .collect();
    }
    body
}

pub fn parse_models(body: &Value) -> Vec<ModelInfo> {
    let mut models: Vec<ModelInfo> = body["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| m["id"].as_str())
        .map(|id| ModelInfo {
            id: id.to_string(),
            name: None,
            details: None,
        })
        .collect();
    models.sort_by(|a, b| a.id.cmp(&b.id));
    models
}

/// LM Studio's `GET /api/v1/models`: language models only.
pub fn parse_lmstudio_models(body: &Value) -> Vec<ModelInfo> {
    body["models"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|m| m["type"] == "llm")
        .filter_map(|m| {
            let details = [
                m["params_string"].as_str(),
                m["quantization"]["name"].as_str(),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" · ");
            Some(ModelInfo {
                id: m["key"].as_str()?.to_string(),
                name: m["display_name"].as_str().map(str::to_owned),
                details: (!details.is_empty()).then_some(details),
            })
        })
        .collect()
}

/// The loaded context length if the model is loaded, else its maximum.
pub fn parse_lmstudio_capabilities(body: &Value, model: &str) -> ModelCapabilities {
    let Some(entry) = body["models"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|m| m["key"] == model)
    else {
        return ModelCapabilities::UNKNOWN;
    };
    let loaded = u32_at(entry, "/loaded_instances/0/config/context_length");
    ModelCapabilities {
        tool_calling: match entry["capabilities"]["trained_for_tool_use"].as_bool() {
            Some(true) => Tri::Yes,
            Some(false) => Tri::No,
            None => Tri::Unknown,
        },
        context_window: loaded.or_else(|| u32_at(entry, "/max_context_length")),
        max_output_tokens: None,
    }
}

#[derive(Default)]
pub struct ChatDecoder {
    /// Tool-call index → call ID, in the order the calls started.
    calls: BTreeMap<u64, String>,
    finish_reason: Option<String>,
    input: Option<u32>,
    output: Option<u32>,
    done: bool,
}

impl ChatDecoder {
    fn close(&mut self) -> Vec<ChatEvent> {
        self.done = true;
        let mut out: Vec<ChatEvent> = std::mem::take(&mut self.calls)
            .into_values()
            .map(|id| ChatEvent::ToolCallEnd { id })
            .collect();
        out.push(ChatEvent::Usage {
            input: self.input,
            output: self.output,
        });
        out.push(ChatEvent::Done(match self.finish_reason.as_deref() {
            Some("stop") | None => StopReason::EndTurn,
            Some("length") => StopReason::MaxTokens,
            Some("tool_calls" | "function_call") => StopReason::ToolUse,
            Some("content_filter") => StopReason::ContentFilter,
            Some(_) => StopReason::Other,
        }));
        out
    }
}

impl Decoder for ChatDecoder {
    fn decode(&mut self, frame: Frame) -> Result<Vec<ChatEvent>, AiError> {
        let Frame::Sse(event) = frame else {
            return Ok(Vec::new());
        };
        if event.data.trim() == "[DONE]" {
            return Ok(self.close());
        }
        let data = json(&event.data)?;
        if let Some(error) = data.get("error").filter(|e| !e.is_null()) {
            let message = error["message"]
                .as_str()
                .or(error.as_str())
                .unwrap_or("Unknown error");
            return Err(AiError::new(
                kind_for_code(error["code"].as_str().unwrap_or_default()),
                format!("The server stopped with an error: {message}"),
            ));
        }
        if let Some(n) = u32_at(&data, "/usage/prompt_tokens") {
            self.input = Some(n);
        }
        if let Some(n) = u32_at(&data, "/usage/completion_tokens") {
            self.output = Some(n);
        }
        let mut out = Vec::new();
        let Some(choice) = data["choices"].as_array().and_then(|c| c.first()) else {
            return Ok(out);
        };
        let delta = &choice["delta"];
        if let Some(text) = delta["content"].as_str().filter(|t| !t.is_empty()) {
            out.push(ChatEvent::TextDelta(text.to_string()));
        }
        for call in delta["tool_calls"].as_array().into_iter().flatten() {
            let index = call["index"].as_u64().unwrap_or_default();
            let id = match self.calls.entry(index) {
                Entry::Occupied(known) => known.get().clone(),
                Entry::Vacant(slot) => {
                    let id = call["id"]
                        .as_str()
                        .map_or_else(|| format!("call_{index}"), str::to_owned);
                    out.push(ChatEvent::ToolCallStart {
                        id: id.clone(),
                        name: call["function"]["name"]
                            .as_str()
                            .unwrap_or_default()
                            .to_string(),
                    });
                    slot.insert(id).clone()
                }
            };
            if let Some(args) = call["function"]["arguments"]
                .as_str()
                .filter(|a| !a.is_empty())
            {
                out.push(ChatEvent::ToolCallArgsDelta {
                    id,
                    delta: args.to_string(),
                });
            }
        }
        if let Some(reason) = choice["finish_reason"].as_str() {
            self.finish_reason = Some(reason.to_string());
        }
        Ok(out)
    }

    fn is_done(&self) -> bool {
        self.done
    }

    /// Some servers close the stream after the last choice without `[DONE]`.
    fn finish(&mut self) -> Result<Vec<ChatEvent>, AiError> {
        if self.finish_reason.is_some() {
            Ok(self.close())
        } else {
            Err(AiError::interrupted())
        }
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
        decode_every_way(ChatDecoder::default, Framing::Sse, bytes)
    }

    #[test]
    fn streams_text_and_usage() {
        let events = decode(include_bytes!("../../fixtures/openai-chat-text.sse")).unwrap();
        assert_eq!(text(&events), "Hello! 你好");
        assert_eq!(
            &events[events.len() - 2..],
            &[
                ChatEvent::Usage {
                    input: Some(19),
                    output: Some(10)
                },
                ChatEvent::Done(StopReason::EndTurn)
            ]
        );
    }

    #[test]
    fn streams_parallel_tool_calls_by_index() {
        let events = decode(include_bytes!("../../fixtures/openai-chat-tools.sse")).unwrap();
        let args = |id: &str| -> String {
            events
                .iter()
                .filter_map(|e| match e {
                    ChatEvent::ToolCallArgsDelta { id: i, delta } if i == id => {
                        Some(delta.as_str())
                    }
                    _ => None,
                })
                .collect()
        };
        assert_eq!(args("call_a"), "{\"query\":\"design\"}");
        assert_eq!(args("call_b"), "{\"page_id\":\"p1\"}");
        assert!(events.contains(&ChatEvent::ToolCallEnd {
            id: "call_b".into()
        }));
        assert_eq!(events.last(), Some(&ChatEvent::Done(StopReason::ToolUse)));
    }

    #[test]
    fn a_stream_without_done_still_finishes_after_a_finish_reason() {
        let full = include_bytes!("../../fixtures/openai-chat-text.sse");
        let text_end = std::str::from_utf8(full)
            .unwrap()
            .find("data: [DONE]")
            .unwrap();
        let events = decode(&full[..text_end]).unwrap();
        assert_eq!(events.last(), Some(&ChatEvent::Done(StopReason::EndTurn)));
        // Without a finish reason, it was cut off.
        let err = decode(b"data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Hi\"},\"finish_reason\":null}]}\n\n").unwrap_err();
        assert_eq!(err.kind, AiErrorKind::Interrupted);
    }

    #[test]
    fn an_error_in_the_stream_fails_it() {
        let err = decode(b"data: {\"error\":{\"message\":\"Model unloaded\",\"code\":null}}\n\n")
            .unwrap_err();
        assert!(err.message.contains("Model unloaded"));
    }

    #[test]
    fn the_body_puts_the_system_prompt_first() {
        let body = chat_body(&ChatRequest {
            model: "qwen".into(),
            system: Some("Be brief.".into()),
            messages: vec![Message {
                role: Role::User,
                content: "Hi".into(),
            }],
            tools: vec![],
            max_output_tokens: Some(256),
            temperature: Some(0.2),
            context_window: None,
        });
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["messages"][1]["content"], "Hi");
        assert_eq!(body["max_tokens"], 256);
        assert_eq!(body["stream_options"]["include_usage"], true);
        assert!((body["temperature"].as_f64().unwrap() - 0.2).abs() < 1e-6);
    }

    #[test]
    fn lm_studio_models_and_capabilities() {
        let body = json!({"models": [
            {"type": "llm", "key": "qwen3-8b", "display_name": "Qwen3 8B", "params_string": "8B",
             "quantization": {"name": "Q4_K_M", "bits_per_weight": 4},
             "max_context_length": 40960,
             "loaded_instances": [{"id": "qwen3-8b", "config": {"context_length": 8192}}],
             "capabilities": {"vision": false, "trained_for_tool_use": true}},
            {"type": "embedding", "key": "nomic-embed", "display_name": "Nomic", "max_context_length": 2048}
        ]});
        let models = parse_lmstudio_models(&body);
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].details.as_deref(), Some("8B · Q4_K_M"));
        let caps = parse_lmstudio_capabilities(&body, "qwen3-8b");
        assert_eq!(caps.tool_calling, Tri::Yes);
        assert_eq!(caps.context_window, Some(8192));
        assert_eq!(
            parse_lmstudio_capabilities(&body, "missing"),
            ModelCapabilities::UNKNOWN
        );
        assert_eq!(
            lmstudio_models_url("http://localhost:1234/v1"),
            "http://localhost:1234/api/v1/models"
        );
    }
}
