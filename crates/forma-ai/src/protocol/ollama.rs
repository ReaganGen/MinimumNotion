//! Ollama's native API, with JSON lines. Forma uses it instead of Ollama's
//! OpenAI-compatible endpoint because only the native one takes `num_ctx`:
//! without it Ollama uses a small window and cuts long input silently (R9).

use serde_json::{Value, json};

use super::{Decoder, Frame, arguments_object, json, u32_at};
use crate::error::{AiError, AiErrorKind};
use crate::types::{ChatEvent, ChatRequest, ModelCapabilities, ModelInfo, StopReason, Tri, Turn};

/// The window Forma asks for when the user hasn't set one, if the model
/// allows it. Bigger windows take more memory.
pub const DEFAULT_CONTEXT_WINDOW: u32 = 8192;

pub fn chat_url(base: &str) -> String {
    format!("{base}/api/chat")
}

pub fn tags_url(base: &str) -> String {
    format!("{base}/api/tags")
}

pub fn show_url(base: &str) -> String {
    format!("{base}/api/show")
}

pub fn chat_body(req: &ChatRequest) -> Value {
    let mut messages = Vec::new();
    if let Some(system) = &req.system {
        messages.push(json!({ "role": "system", "content": system }));
    }
    messages.extend(req.messages.iter().map(message));
    let mut options = json!({
        "num_ctx": req.context_window.unwrap_or(DEFAULT_CONTEXT_WINDOW),
    });
    if let Some(n) = req.max_output_tokens {
        options["num_predict"] = json!(n);
    }
    if let Some(t) = req.temperature {
        options["temperature"] = json!(t);
    }
    let mut body = json!({
        "model": req.model,
        "messages": messages,
        "stream": true,
        "options": options,
    });
    // Ollama has no way to turn tools off but leave them defined, and
    // doesn't need them defined to read earlier calls.
    if !req.tools.is_empty() && req.allow_tool_calls {
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

/// Ollama's tool calls carry no IDs, and its arguments are objects; a
/// result names the tool it came from.
fn message(turn: &Turn) -> Value {
    match turn {
        Turn::User(text) => json!({ "role": "user", "content": text }),
        Turn::Assistant {
            text, tool_calls, ..
        } if tool_calls.is_empty() => {
            json!({ "role": "assistant", "content": text })
        }
        Turn::Assistant {
            text, tool_calls, ..
        } => json!({
            "role": "assistant",
            "content": text,
            "tool_calls": tool_calls.iter().map(|c| json!({
                "function": { "name": c.name, "arguments": arguments_object(&c.arguments) },
            })).collect::<Vec<_>>(),
        }),
        Turn::ToolResult { name, content, .. } => {
            json!({ "role": "tool", "content": content, "tool_name": name })
        }
    }
}

pub fn parse_tags(body: &Value) -> Vec<ModelInfo> {
    body["models"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| {
            let details = [
                m["details"]["parameter_size"].as_str(),
                m["details"]["quantization_level"].as_str(),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" · ");
            Some(ModelInfo {
                id: m["name"].as_str().or(m["model"].as_str())?.to_string(),
                name: None,
                details: (!details.is_empty()).then_some(details),
            })
        })
        .collect()
}

/// `POST /api/show`: `capabilities` lists "tools" for models that can call
/// them, and `model_info` holds `<architecture>.context_length`.
pub fn parse_show(body: &Value) -> ModelCapabilities {
    let tool_calling = match body["capabilities"].as_array() {
        Some(list) if list.iter().any(|c| c == "tools") => Tri::Yes,
        Some(_) => Tri::No,
        None => Tri::Unknown,
    };
    let context_window = body["model_info"]["general.architecture"]
        .as_str()
        .and_then(|arch| {
            body["model_info"][format!("{arch}.context_length")]
                .as_u64()
                .and_then(|n| u32::try_from(n).ok())
        });
    ModelCapabilities {
        tool_calling,
        context_window,
        max_output_tokens: None,
    }
}

#[derive(Default)]
pub struct OllamaDecoder {
    calls: usize,
    done: bool,
}

impl Decoder for OllamaDecoder {
    fn decode(&mut self, frame: Frame) -> Result<Vec<ChatEvent>, AiError> {
        let Frame::Line(line) = frame else {
            return Ok(Vec::new());
        };
        if line.trim().is_empty() {
            return Ok(Vec::new());
        }
        let data = json(&line)?;
        if let Some(message) = data["error"].as_str() {
            let kind = if message.contains("not found") {
                AiErrorKind::ModelNotFound
            } else {
                AiErrorKind::Provider
            };
            return Err(AiError::new(
                kind,
                format!("Ollama stopped with an error: {message}"),
            ));
        }
        let mut out = Vec::new();
        if let Some(text) = data["message"]["content"]
            .as_str()
            .filter(|t| !t.is_empty())
        {
            out.push(ChatEvent::TextDelta(text.to_string()));
        }
        // Ollama sends each tool call whole, with its arguments as an object.
        for call in data["message"]["tool_calls"]
            .as_array()
            .into_iter()
            .flatten()
        {
            let id = format!("call_{}", self.calls);
            self.calls += 1;
            out.push(ChatEvent::ToolCallStart {
                id: id.clone(),
                name: call["function"]["name"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
            });
            out.push(ChatEvent::ToolCallArgsDelta {
                id: id.clone(),
                delta: call["function"]["arguments"].to_string(),
            });
            out.push(ChatEvent::ToolCallEnd { id });
        }
        if data["done"] == true {
            self.done = true;
            out.push(ChatEvent::Usage {
                input: u32_at(&data, "/prompt_eval_count"),
                output: u32_at(&data, "/eval_count"),
            });
            out.push(ChatEvent::Done(match data["done_reason"].as_str() {
                Some("length") => StopReason::MaxTokens,
                _ if self.calls > 0 => StopReason::ToolUse,
                Some("stop") | None => StopReason::EndTurn,
                Some(_) => StopReason::Other,
            }));
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
    use crate::protocol::Framing;
    use crate::protocol::testing::{decode_every_way, text};
    use crate::types::ToolCall;

    fn decode(bytes: &[u8]) -> Result<Vec<ChatEvent>, AiError> {
        decode_every_way(OllamaDecoder::default, Framing::Lines, bytes)
    }

    #[test]
    fn streams_text_and_usage() {
        let events = decode(include_bytes!("../../fixtures/ollama-text.ndjson")).unwrap();
        assert_eq!(text(&events), "The sky is blue. 天空是蓝色的。");
        assert_eq!(
            &events[events.len() - 2..],
            &[
                ChatEvent::Usage {
                    input: Some(26),
                    output: Some(298)
                },
                ChatEvent::Done(StopReason::EndTurn)
            ]
        );
    }

    #[test]
    fn streams_whole_tool_calls() {
        let events = decode(include_bytes!("../../fixtures/ollama-tool.ndjson")).unwrap();
        assert_eq!(
            &events[..3],
            &[
                ChatEvent::ToolCallStart {
                    id: "call_0".into(),
                    name: "search_pages".into()
                },
                ChatEvent::ToolCallArgsDelta {
                    id: "call_0".into(),
                    delta: "{\"query\":\"protein design\"}".into()
                },
                ChatEvent::ToolCallEnd {
                    id: "call_0".into()
                },
            ]
        );
        assert_eq!(events.last(), Some(&ChatEvent::Done(StopReason::ToolUse)));
    }

    #[test]
    fn errors_are_named() {
        let err = decode(b"{\"error\":\"model 'llama9' not found\"}\n").unwrap_err();
        assert_eq!(err.kind, AiErrorKind::ModelNotFound);
        let cut =
            decode(b"{\"message\":{\"role\":\"assistant\",\"content\":\"The\"},\"done\":false}\n")
                .unwrap_err();
        assert_eq!(cut.kind, AiErrorKind::Interrupted);
    }

    #[test]
    fn the_body_always_sets_a_context_window() {
        let req = ChatRequest {
            model: "llama3.2".into(),
            system: None,
            messages: vec![
                Turn::Assistant {
                    text: String::new(),
                    reasoning: vec![],
                    tool_calls: vec![ToolCall {
                        id: "call_0".into(),
                        name: "search_pages".into(),
                        arguments: r#"{"query":"x"}"#.into(),
                    }],
                },
                Turn::ToolResult {
                    call_id: "call_0".into(),
                    name: "search_pages".into(),
                    content: "none".into(),
                    is_error: false,
                },
            ],
            tools: vec![],
            allow_tool_calls: true,
            max_output_tokens: Some(100),
            temperature: None,
            context_window: None,
        };
        let body = chat_body(&req);
        assert_eq!(body["options"]["num_ctx"], DEFAULT_CONTEXT_WINDOW);
        assert_eq!(body["options"]["num_predict"], 100);
        assert!(body["options"].get("temperature").is_none());
        assert_eq!(
            body["messages"],
            json!([
                {"role": "assistant", "content": "", "tool_calls": [
                    {"function": {"name": "search_pages", "arguments": {"query": "x"}}}
                ]},
                {"role": "tool", "content": "none", "tool_name": "search_pages"},
            ])
        );
        let body = chat_body(&ChatRequest {
            context_window: Some(32768),
            ..req
        });
        assert_eq!(body["options"]["num_ctx"], 32768);
    }

    #[test]
    fn models_and_capabilities_are_read() {
        let models = parse_tags(&json!({"models": [{
            "name": "llama3.2:latest", "model": "llama3.2:latest",
            "details": {"family": "llama", "parameter_size": "3.2B", "quantization_level": "Q4_K_M"}
        }]}));
        assert_eq!(models[0].id, "llama3.2:latest");
        assert_eq!(models[0].details.as_deref(), Some("3.2B · Q4_K_M"));

        let caps = parse_show(&json!({
            "capabilities": ["completion", "tools"],
            "model_info": {"general.architecture": "llama", "llama.context_length": 131072}
        }));
        assert_eq!(caps.tool_calling, Tri::Yes);
        assert_eq!(caps.context_window, Some(131_072));
        assert_eq!(
            parse_show(&json!({"capabilities": ["completion", "vision"]})).tool_calling,
            Tri::No
        );
    }
}
