//! Provider failures, named so the UI can say exactly what went wrong
//! (ARCHITECTURE §9.1). Messages are shown to the user as they are.

use serde::Serialize;
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum AiErrorKind {
    /// AI is turned off in Settings.
    Disabled,
    /// No provider, no model, a missing key, or cloud use not yet agreed to.
    NotConfigured,
    Auth,
    /// The server couldn't be reached at all.
    Unreachable,
    Tls,
    Timeout,
    RateLimited,
    ModelNotFound,
    ContextOverflow,
    /// The provider answered with an error of its own.
    Provider,
    /// The answer stopped before it was complete.
    Interrupted,
    /// The provider sent something Forma couldn't read.
    InvalidResponse,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS, thiserror::Error)]
#[error("{message}")]
#[ts(export)]
pub struct AiError {
    pub kind: AiErrorKind,
    pub message: String,
    /// HTTP status, when the provider sent one.
    pub status: Option<u16>,
    /// Seconds to wait before trying again (rate limits).
    pub retry_after_secs: Option<u32>,
}

impl AiError {
    pub fn new(kind: AiErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            status: None,
            retry_after_secs: None,
        }
    }

    pub fn cancelled() -> Self {
        Self::new(AiErrorKind::Cancelled, "Stopped.")
    }

    pub fn interrupted() -> Self {
        Self::new(
            AiErrorKind::Interrupted,
            "The connection closed before the answer was complete.",
        )
    }

    pub fn invalid_response(detail: impl std::fmt::Display) -> Self {
        Self::new(
            AiErrorKind::InvalidResponse,
            format!("The provider sent a response Forma couldn't read ({detail})."),
        )
    }

    /// Removes every occurrence of `secret` from the message. Providers
    /// sometimes echo part of a rejected key; Forma never passes it on.
    pub fn redacted(mut self, secret: Option<&str>) -> Self {
        if let Some(secret) = secret.filter(|s| s.len() >= 4) {
            self.message = self.message.replace(secret, "[key removed]");
        }
        self
    }
}

/// Maps an HTTP error status and body to an error. `provider` names who
/// answered, for the message.
pub(crate) fn from_status(
    provider: &str,
    status: u16,
    retry_after: Option<u32>,
    body: &str,
) -> AiError {
    let detail = error_message(body);
    let with = |text: &str| match &detail {
        Some(d) => format!("{text} {provider} said: {d}"),
        None => text.to_string(),
    };
    let lower = detail.as_deref().unwrap_or_default().to_lowercase();
    let mentions_context = (lower.contains("context")
        && (lower.contains("length") || lower.contains("window") || lower.contains("exceed")))
        || lower.contains("too long")
        || lower.contains("too many tokens");
    let (kind, message) = match status {
        401 | 403 => (
            AiErrorKind::Auth,
            with(&format!("{provider} didn't accept the API key.")),
        ),
        404 if lower.contains("model") => (
            AiErrorKind::ModelNotFound,
            with(&format!("{provider} doesn't have this model.")),
        ),
        404 => (
            AiErrorKind::Provider,
            with("Nothing answered at this address (404). Check the base URL."),
        ),
        408 | 504 => (
            AiErrorKind::Timeout,
            with(&format!("{provider} took too long to answer.")),
        ),
        413 => (
            AiErrorKind::ContextOverflow,
            with("The request is too large for this model."),
        ),
        429 => (
            AiErrorKind::RateLimited,
            with(&format!("{provider} is limiting requests.")),
        ),
        400 | 422 if mentions_context => (
            AiErrorKind::ContextOverflow,
            with("The conversation is too long for this model."),
        ),
        s if s >= 500 => (
            AiErrorKind::Provider,
            with(&format!(
                "{provider} had a problem ({s}). Try again in a moment."
            )),
        ),
        s => (
            AiErrorKind::Provider,
            with(&format!("{provider} refused the request ({s}).")),
        ),
    };
    AiError {
        kind,
        message,
        status: Some(status),
        retry_after_secs: if kind == AiErrorKind::RateLimited {
            retry_after
        } else {
            None
        },
    }
}

/// The human-readable part of a provider's error body: `error.message`
/// (OpenAI, Anthropic), `error` as a string (Ollama), or `message`.
pub(crate) fn error_message(body: &str) -> Option<String> {
    let text = match serde_json::from_str::<serde_json::Value>(body) {
        Ok(json) => json
            .pointer("/error/message")
            .or_else(|| json.get("error").filter(|e| e.is_string()))
            .or_else(|| json.get("message"))
            .and_then(|v| v.as_str())
            .map(str::to_owned),
        // Plain-text bodies: keep a short one, never a whole HTML page.
        Err(_) => {
            let trimmed = body.trim();
            (!trimmed.is_empty() && !trimmed.starts_with('<')).then(|| trimmed.to_string())
        }
    }?;
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    Some(if text.chars().count() > 300 {
        format!("{}…", text.chars().take(300).collect::<String>())
    } else {
        text.to_string()
    })
}

/// Maps a provider's error code inside a stream (OpenAI `code`, Anthropic
/// `error.type`) to a kind.
pub(crate) fn kind_for_code(code: &str) -> AiErrorKind {
    match code {
        "rate_limit_exceeded" | "rate_limit_error" | "insufficient_quota" => {
            AiErrorKind::RateLimited
        }
        "context_length_exceeded" => AiErrorKind::ContextOverflow,
        "invalid_api_key" | "authentication_error" | "permission_error" => AiErrorKind::Auth,
        "model_not_found" | "not_found_error" => AiErrorKind::ModelNotFound,
        _ => AiErrorKind::Provider,
    }
}

/// Describes a failure to talk to `host` at all.
pub(crate) fn from_transport(provider: &str, host: &str, err: &reqwest::Error) -> AiError {
    if err.is_timeout() {
        return AiError::new(
            AiErrorKind::Timeout,
            format!("{provider} at {host} didn't answer in time."),
        );
    }
    // The TLS details are only in the error's sources.
    let mut chain = String::new();
    let mut source: Option<&dyn std::error::Error> = Some(err);
    while let Some(e) = source {
        chain.push_str(&e.to_string().to_lowercase());
        chain.push(' ');
        source = e.source();
    }
    if ["certificate", "tls", "ssl", "handshake"]
        .iter()
        .any(|w| chain.contains(w))
    {
        return AiError::new(
            AiErrorKind::Tls,
            format!(
                "Couldn't make a secure connection to {host}. The server's certificate wasn't accepted."
            ),
        );
    }
    if err.is_connect() {
        return AiError::new(
            AiErrorKind::Unreachable,
            format!(
                "Couldn't connect to {provider} at {host}. Check the address and that the server is running."
            ),
        );
    }
    if err.is_body() || err.is_decode() {
        return AiError::interrupted();
    }
    AiError::new(
        AiErrorKind::Unreachable,
        format!("Couldn't reach {provider} at {host}."),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statuses_map_to_kinds_with_the_providers_message() {
        let auth = from_status(
            "OpenAI",
            401,
            None,
            r#"{"error":{"message":"Incorrect API key provided","type":"invalid_request_error"}}"#,
        );
        assert_eq!(auth.kind, AiErrorKind::Auth);
        assert!(auth.message.contains("Incorrect API key provided"));
        assert_eq!(auth.status, Some(401));

        let limited = from_status("Anthropic", 429, Some(20), "{}");
        assert_eq!(limited.kind, AiErrorKind::RateLimited);
        assert_eq!(limited.retry_after_secs, Some(20));

        let missing = from_status(
            "Ollama",
            404,
            None,
            r#"{"error":"model 'llama9' not found"}"#,
        );
        assert_eq!(missing.kind, AiErrorKind::ModelNotFound);
        assert!(missing.message.contains("llama9"));

        let wrong_url = from_status("Local server", 404, None, "<html>nope</html>");
        assert_eq!(wrong_url.kind, AiErrorKind::Provider);
        assert!(wrong_url.message.contains("base URL"));
        assert!(!wrong_url.message.contains("html"));

        let long = from_status(
            "OpenAI",
            400,
            None,
            r#"{"error":{"message":"This model's maximum context length is 8192 tokens"}}"#,
        );
        assert_eq!(long.kind, AiErrorKind::ContextOverflow);
        assert_eq!(
            from_status("Anthropic", 529, None, "").kind,
            AiErrorKind::Provider
        );
    }

    #[test]
    fn keys_are_removed_from_messages() {
        let err = from_status(
            "OpenAI",
            401,
            None,
            r#"{"error":{"message":"Incorrect API key provided: sk-live-SECRET123"}}"#,
        )
        .redacted(Some("sk-live-SECRET123"));
        assert!(!err.message.contains("SECRET123"), "{}", err.message);
        assert!(err.message.contains("[key removed]"));
    }
}
