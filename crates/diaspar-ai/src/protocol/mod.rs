//! Wire protocols. Each one builds request bodies and decodes its stream
//! into [`ChatEvent`]s. Decoders are pure, so recorded streams test them.

pub mod anthropic;
pub mod ollama;
pub mod openai_chat;
pub mod openai_responses;

use serde_json::Value;

use crate::error::AiError;
use crate::framing::SseEvent;
use crate::types::ChatEvent;

/// A tool call's arguments as a JSON object, for protocols that want one.
/// Arguments that don't parse become `{}`; the tool then reports what's missing.
pub(crate) fn arguments_object(arguments: &str) -> Value {
    match serde_json::from_str::<Value>(arguments) {
        Ok(value @ Value::Object(_)) => value,
        _ => Value::Object(Default::default()),
    }
}

pub enum Frame {
    Sse(SseEvent),
    Line(String),
}

#[derive(Debug, Clone, Copy)]
pub enum Framing {
    /// Server-sent events.
    Sse,
    /// One JSON object per line.
    Lines,
}

pub trait Decoder: Send {
    fn decode(&mut self, frame: Frame) -> Result<Vec<ChatEvent>, AiError>;
    /// True once the answer is complete; the rest of the stream is ignored.
    fn is_done(&self) -> bool;
    /// Called when the stream ends. Fails if the answer never completed.
    fn finish(&mut self) -> Result<Vec<ChatEvent>, AiError>;
}

pub(crate) fn json(data: &str) -> Result<Value, AiError> {
    serde_json::from_str(data).map_err(AiError::invalid_response)
}

pub(crate) fn u32_at(value: &Value, pointer: &str) -> Option<u32> {
    value
        .pointer(pointer)
        .and_then(Value::as_u64)
        .and_then(|n| u32::try_from(n).ok())
}

pub(crate) fn str_at<'a>(value: &'a Value, pointer: &str) -> Option<&'a str> {
    value.pointer(pointer).and_then(Value::as_str)
}

#[cfg(test)]
pub(crate) mod testing {
    use super::*;
    use crate::framing::{LineSplitter, SseParser};

    /// Runs a recorded stream through `decoder` in chunks of `piece` bytes,
    /// the way it would arrive from the network.
    pub fn decode(
        mut decoder: impl Decoder,
        framing: Framing,
        bytes: &[u8],
        piece: usize,
    ) -> Result<Vec<ChatEvent>, AiError> {
        let mut sse = SseParser::default();
        let mut lines = LineSplitter::default();
        let mut events = Vec::new();
        for chunk in bytes.chunks(piece) {
            let frames: Vec<Frame> = match framing {
                Framing::Sse => sse.feed(chunk).into_iter().map(Frame::Sse).collect(),
                Framing::Lines => lines.feed(chunk).into_iter().map(Frame::Line).collect(),
            };
            for frame in frames {
                events.extend(decoder.decode(frame)?);
            }
            if decoder.is_done() {
                return Ok(events);
            }
        }
        if !decoder.is_done() {
            events.extend(decoder.finish()?);
        }
        Ok(events)
    }

    /// The text of every `TextDelta`, joined.
    pub fn text(events: &[ChatEvent]) -> String {
        events
            .iter()
            .filter_map(|e| match e {
                ChatEvent::TextDelta(t) => Some(t.as_str()),
                _ => None,
            })
            .collect()
    }

    /// The same events for every chunk size, so splitting never changes the result.
    pub fn decode_every_way<D: Decoder>(
        new: impl Fn() -> D,
        framing: Framing,
        bytes: &[u8],
    ) -> Result<Vec<ChatEvent>, AiError> {
        let whole = decode(new(), framing, bytes, bytes.len().max(1));
        for piece in [1, 3, 17] {
            assert_eq!(
                decode(new(), framing, bytes, piece),
                whole,
                "chunks of {piece}"
            );
        }
        whole
    }
}
