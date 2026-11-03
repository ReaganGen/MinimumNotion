//! Splitting a byte stream into server-sent events (OpenAI, Anthropic) or
//! JSON lines (Ollama). Network chunks can end anywhere, even inside a UTF-8
//! character, so bytes are buffered until a line is complete. Whatever is
//! unfinished when the stream ends is dropped, as the event-stream rules
//! say: a stream cut off mid-line was interrupted, not malformed.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SseEvent {
    /// The `event:` field, when the server sends one.
    pub event: Option<String>,
    pub data: String,
}

/// Complete lines from a byte stream, without their `\n` or `\r\n`.
#[derive(Debug, Default)]
pub struct LineSplitter {
    pending: Vec<u8>,
}

impl LineSplitter {
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<String> {
        self.pending.extend_from_slice(bytes);
        let mut lines = Vec::new();
        let mut start = 0;
        while let Some(offset) = self.pending[start..].iter().position(|&b| b == b'\n') {
            let end = start + offset;
            let line = &self.pending[start..end];
            let line = line.strip_suffix(b"\r").unwrap_or(line);
            lines.push(String::from_utf8_lossy(line).into_owned());
            start = end + 1;
        }
        self.pending.drain(..start);
        lines
    }
}

/// Server-sent events, following the WHATWG event-stream rules: `data:` lines
/// join with newlines, a blank line ends an event, `:` lines are comments.
#[derive(Debug, Default)]
pub struct SseParser {
    lines: LineSplitter,
    event: Option<String>,
    data: Vec<String>,
}

impl SseParser {
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<SseEvent> {
        let lines = self.lines.feed(bytes);
        lines.into_iter().filter_map(|l| self.line(&l)).collect()
    }

    fn line(&mut self, line: &str) -> Option<SseEvent> {
        if line.is_empty() {
            return self.dispatch();
        }
        if line.starts_with(':') {
            return None;
        }
        let (field, value) = match line.split_once(':') {
            Some((field, value)) => (field, value.strip_prefix(' ').unwrap_or(value)),
            None => (line, ""),
        };
        match field {
            "event" => self.event = Some(value.to_string()),
            "data" => self.data.push(value.to_string()),
            _ => {}
        }
        None
    }

    fn dispatch(&mut self) -> Option<SseEvent> {
        let event = self.event.take();
        if self.data.is_empty() {
            return None;
        }
        let data = std::mem::take(&mut self.data).join("\n");
        Some(SseEvent { event, data })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sse_in_pieces(input: &[u8], piece: usize) -> Vec<SseEvent> {
        let mut parser = SseParser::default();
        let mut out = Vec::new();
        for chunk in input.chunks(piece) {
            out.extend(parser.feed(chunk));
        }
        out
    }

    #[test]
    fn events_survive_any_chunking() {
        let input = "event: message_start\ndata: {\"a\":1}\n\n: keep-alive\n\ndata: 你好\r\ndata: line two\r\n\r\ndata: [DONE]\n\n".as_bytes();
        let expected = vec![
            SseEvent {
                event: Some("message_start".into()),
                data: "{\"a\":1}".into(),
            },
            SseEvent {
                event: None,
                data: "你好\nline two".into(),
            },
            SseEvent {
                event: None,
                data: "[DONE]".into(),
            },
        ];
        for piece in [1, 2, 3, 7, input.len()] {
            assert_eq!(sse_in_pieces(input, piece), expected, "pieces of {piece}");
        }
    }

    #[test]
    fn an_unterminated_last_event_is_dropped() {
        assert!(sse_in_pieces(b"data: tail", 4).is_empty());
        assert!(sse_in_pieces(b"data: tail\n", 4).is_empty());
    }

    #[test]
    fn lines_split_inside_a_character() {
        let mut lines = LineSplitter::default();
        let bytes = "{\"t\":\"注意\"}\n{\"t\":2}".as_bytes();
        let (a, b) = bytes.split_at(8); // inside 注
        assert!(lines.feed(a).is_empty());
        assert_eq!(lines.feed(b), vec!["{\"t\":\"注意\"}".to_string()]);
        assert!(lines.feed(b"").is_empty(), "the unfinished line waits");
    }
}
