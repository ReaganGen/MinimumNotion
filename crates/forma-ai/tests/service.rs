//! The AI service against a local HTTP server: what each protocol sends,
//! how answers stream, and how every kind of failure is reported.

mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use forma_ai::secrets::MemoryStore;
use forma_ai::service::SYSTEM_PROMPT;
use forma_ai::types::{Message, Role, StopReason};
use forma_ai::{AiErrorKind, AiService, ChatStreamEvent};
use forma_core::settings::{AiSettings, ProviderKind};
use serde_json::json;

fn service() -> Arc<AiService> {
    Arc::new(AiService::new(
        forma_ai::provider::http_client().unwrap(),
        Arc::new(MemoryStore::default()),
    ))
}

fn user(text: &str) -> Vec<Message> {
    vec![Message {
        role: Role::User,
        content: text.into(),
    }]
}

async fn chat(service: &AiService, ai: &AiSettings, text: &str) -> Vec<ChatStreamEvent> {
    let mut events = Vec::new();
    service
        .chat(ai, "req-1", user(text), |e| events.push(e))
        .await;
    events
}

#[tokio::test]
async fn anthropic_streams_and_sends_what_the_api_expects() {
    let server = serve(vec![
        (
            "/v1/models/claude-test",
            json(
                200,
                json!({"id": "claude-test", "max_input_tokens": 200000, "max_tokens": 64000}),
            ),
        ),
        (
            "/v1/messages",
            sse(include_bytes!("../fixtures/anthropic-text.sse")),
        ),
    ])
    .await;
    let service = service();
    service.set_key("provider-1", "sk-ant-test").await.unwrap();
    let mut config = provider(ProviderKind::Anthropic, &server.url, "claude-test");
    config.has_credential = true;
    config.options.temperature = Some(0.7);
    let events = chat(&service, &enabled(config), "Hi").await;

    assert_eq!(text_of(&events), "Hello! 你好。");
    assert_eq!(
        events.last(),
        Some(&ChatStreamEvent::Done {
            stop_reason: StopReason::EndTurn
        })
    );
    assert!(events.contains(&ChatStreamEvent::Usage {
        input_tokens: Some(25),
        output_tokens: Some(15)
    }));
    let sent = server.request_to("/v1/messages");
    assert_eq!(sent.header("authorization"), Some("Bearer sk-ant-test"));
    assert_eq!(sent.header("anthropic-version"), Some("2026-10-21"));
    let body = sent.json();
    assert_eq!(body["model"], "claude-test");
    assert_eq!(body["stream"], true);
    assert_eq!(body["system"], SYSTEM_PROMPT);
    assert_eq!(body["messages"], json!([{"role": "user", "content": "Hi"}]));
    assert!(
        body.get("temperature").is_none(),
        "removed in this API version"
    );
}

#[tokio::test]
async fn openai_uses_the_responses_api_without_storage() {
    let server = serve(vec![(
        "/v1/responses",
        sse(include_bytes!("../fixtures/openai-responses-text.sse")),
    )])
    .await;
    let service = service();
    service.set_key("provider-1", "sk-proj-test").await.unwrap();
    let mut config = provider(
        ProviderKind::OpenAi,
        &format!("{}/v1", server.url),
        "gpt-6-astra",
    );
    config.has_credential = true;
    let events = chat(&service, &enabled(config), "Hi").await;

    assert_eq!(text_of(&events), "Hi there! 你好");
    let sent = server.request_to("/v1/responses");
    assert_eq!(sent.header("authorization"), Some("Bearer sk-proj-test"));
    assert_eq!(sent.json()["store"], false);
    assert_eq!(sent.json()["instructions"], SYSTEM_PROMPT);
}

#[tokio::test]
async fn lm_studio_streams_chat_completions_without_a_key() {
    let server = serve(vec![
        (
            "/api/v1/models",
            json(200, json!({"models": [{"type": "llm", "key": "qwen3-8b", "max_context_length": 32768}]})),
        ),
        ("/v1/chat/completions", sse(include_bytes!("../fixtures/openai-chat-text.sse"))),
    ])
    .await;
    let config = provider(
        ProviderKind::LmStudio,
        &format!("{}/v1", server.url),
        "qwen3-8b",
    );
    let events = chat(&service(), &enabled(config), "Hi").await;
    assert_eq!(text_of(&events), "Hello! 你好");
    let sent = server.request_to("/v1/chat/completions");
    assert_eq!(sent.header("authorization"), None);
    assert_eq!(sent.json()["messages"][0]["role"], "system");
}

#[tokio::test]
async fn ollama_always_asks_for_a_context_window() {
    let server = serve(vec![
        (
            "/api/show",
            json(
                200,
                json!({
                    "capabilities": ["completion"],
                    "model_info": {"general.architecture": "llama", "llama.context_length": 4096}
                }),
            ),
        ),
        (
            "/api/chat",
            ndjson(include_bytes!("../fixtures/ollama-text.ndjson"), false),
        ),
    ])
    .await;
    let config = provider(ProviderKind::Ollama, &server.url, "llama3.2");
    let events = chat(&service(), &enabled(config), "Why is the sky blue?").await;
    assert_eq!(text_of(&events), "The sky is blue. 天空是蓝色的。");
    // The model's own window, because it's smaller than Forma's default.
    assert_eq!(
        server.request_to("/api/show").json(),
        json!({"model": "llama3.2"})
    );
    assert_eq!(
        server.request_to("/api/chat").json()["options"]["num_ctx"],
        4096
    );
}

#[tokio::test]
async fn stopping_mid_answer_cancels_the_request() {
    let first_line = b"{\"model\":\"m\",\"message\":{\"role\":\"assistant\",\"content\":\"Once\"},\"done\":false}\n";
    let server = serve(vec![
        ("/api/show", json(200, json!({}))),
        ("/api/chat", ndjson(first_line, true)),
    ])
    .await;
    let service = service();
    let ai = enabled(provider(ProviderKind::Ollama, &server.url, "m"));
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let running = Arc::clone(&service);
    let task = tokio::spawn(async move {
        running
            .chat(&ai, "req-stop", user("Tell me a story"), |e| {
                let _ = tx.send(e);
            })
            .await;
    });

    let first = tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .unwrap();
    assert_eq!(
        first,
        Some(ChatStreamEvent::Text {
            text: "Once".into()
        })
    );
    service.cancel("req-stop");
    let next = tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("cancelling stops a stalled stream at once");
    match next {
        Some(ChatStreamEvent::Failed { error }) => assert_eq!(error.kind, AiErrorKind::Cancelled),
        other => panic!("expected a cancellation, got {other:?}"),
    }
    tokio::time::timeout(Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn a_stop_that_arrives_first_still_stops_the_chat() {
    let service = service();
    service.cancel("req-early");
    let ai = enabled(provider(
        ProviderKind::Ollama,
        &closed_port_url().await,
        "m",
    ));
    let mut events = Vec::new();
    service
        .chat(&ai, "req-early", user("Hi"), |e| events.push(e))
        .await;
    assert_eq!(failure(&events).unwrap().kind, AiErrorKind::Cancelled);
}

#[tokio::test]
async fn http_failures_are_named() {
    let server = serve(vec![
        (
            "/v1/chat/completions",
            json(401, json!({"error": {"message": "Invalid token"}})),
        ),
        (
            "/v1/chat/completions",
            Reply::Json {
                status: 429,
                body: r#"{"error":{"message":"Slow down"}}"#.into(),
                headers: vec![("Retry-After", "7".into())],
            },
        ),
        (
            "/v1/chat/completions",
            json(404, json!({"error": {"message": "The model `nope` does not exist"}})),
        ),
        (
            "/v1/chat/completions",
            json(400, json!({"error": {"message": "This model's maximum context length is 4096 tokens."}})),
        ),
    ])
    .await;
    let service = service();
    let ai = enabled(provider(
        ProviderKind::OpenAiCompatible,
        &format!("{}/v1", server.url),
        "nope",
    ));
    let auth = chat(&service, &ai, "a").await;
    assert_eq!(failure(&auth).unwrap().kind, AiErrorKind::Auth);
    assert!(failure(&auth).unwrap().message.contains("Invalid token"));
    let limited = chat(&service, &ai, "b").await;
    assert_eq!(failure(&limited).unwrap().kind, AiErrorKind::RateLimited);
    assert_eq!(failure(&limited).unwrap().retry_after_secs, Some(7));
    let missing = chat(&service, &ai, "c").await;
    assert_eq!(failure(&missing).unwrap().kind, AiErrorKind::ModelNotFound);
    let long = chat(&service, &ai, "d").await;
    assert_eq!(failure(&long).unwrap().kind, AiErrorKind::ContextOverflow);
}

#[tokio::test]
async fn a_stream_cut_short_is_interrupted_and_keeps_the_partial_text() {
    let full = include_bytes!("../fixtures/openai-responses-text.sse");
    let text = std::str::from_utf8(full).unwrap();
    let cut = text.find("event: response.output_text.done").unwrap();
    let server = serve(vec![("/v1/responses", sse(&full[..cut]))]).await;
    let service = service();
    service.set_key("provider-1", "k-test").await.unwrap();
    let mut config = provider(ProviderKind::OpenAi, &format!("{}/v1", server.url), "gpt");
    config.has_credential = true;
    let events = chat(&service, &enabled(config), "Hi").await;
    assert_eq!(text_of(&events), "Hi there! 你好");
    assert_eq!(failure(&events).unwrap().kind, AiErrorKind::Interrupted);
}

#[tokio::test]
async fn nothing_is_sent_until_ai_is_set_up_and_agreed_to() {
    let service = service();
    let config = provider(
        ProviderKind::Anthropic,
        "https://api.anthropic.com",
        "claude",
    );

    let mut off = enabled(config.clone());
    off.enabled = false;
    assert_eq!(
        failure(&chat(&service, &off, "a").await).unwrap().kind,
        AiErrorKind::Disabled
    );

    let mut no_provider = enabled(config.clone());
    no_provider.active_provider = None;
    let err = chat(&service, &no_provider, "a").await;
    assert_eq!(failure(&err).unwrap().kind, AiErrorKind::NotConfigured);

    let mut no_model = enabled(config.clone());
    no_model.providers[0].model.clear();
    let err = chat(&service, &no_model, "a").await;
    assert!(failure(&err).unwrap().message.contains("Choose a model"));

    // A cloud provider the user hasn't agreed to send messages to.
    let err = chat(&service, &enabled(config.clone()), "a").await;
    assert!(
        failure(&err).unwrap().message.contains("api.anthropic.com"),
        "{err:?}"
    );

    // Agreed, but no key: refused before any request.
    let mut agreed = enabled(config);
    agreed
        .cloud_acknowledged
        .insert("provider-1".into(), "2026-11-03T10:00:00Z".into());
    let err = chat(&service, &agreed, "a").await;
    assert!(failure(&err).unwrap().message.contains("Add an API key"));
}

#[tokio::test]
async fn a_conversation_too_long_for_the_window_is_refused_before_sending() {
    let server = serve(vec![("/api/show", json(200, json!({})))]).await;
    let mut config = provider(ProviderKind::Ollama, &server.url, "m");
    config.options.context_window_override = Some(512);
    let events = chat(&service(), &enabled(config), &"word ".repeat(1000)).await;
    assert_eq!(failure(&events).unwrap().kind, AiErrorKind::ContextOverflow);
    assert!(server.requests().iter().all(|r| r.path != "/api/chat"));
}

#[tokio::test]
async fn the_connection_test_lists_models_then_asks_for_one_word() {
    let server = serve(vec![
        (
            "/api/tags",
            json(200, json!({"models": [{"name": "llama3.2:latest"}]})),
        ),
        ("/api/show", json(200, json!({}))),
        (
            "/api/chat",
            ndjson(include_bytes!("../fixtures/ollama-text.ndjson"), false),
        ),
    ])
    .await;
    let service = service();
    let config = provider(ProviderKind::Ollama, &server.url, "llama3.2:latest");
    let report = service.test_connection(&config).await;
    assert!(report.ok, "{report:?}");
    assert!(report.message.contains("answered"));
    let sent = server.request_to("/api/chat").json();
    assert_eq!(
        sent["messages"],
        json!([{"role": "user", "content": "Reply with the word OK."}])
    );
    assert_eq!(sent["options"]["num_predict"], 16);

    let models = service.list_models(&config).await;
    // The route was used up, so the server now says 404.
    assert!(models.is_err());
}

#[tokio::test]
async fn an_unreachable_server_is_reported_as_such() {
    let config = provider(ProviderKind::Ollama, &closed_port_url().await, "m");
    let report = service().test_connection(&config).await;
    assert!(!report.ok);
    assert_eq!(report.error.unwrap().kind, AiErrorKind::Unreachable);
    assert!(
        report.message.contains("Couldn't connect"),
        "{}",
        report.message
    );
}

#[tokio::test]
async fn anthropic_model_lists_are_paged() {
    let server = serve(vec![
        (
            "/v1/models",
            json(200, json!({"data": [{"id": "a", "display_name": "A"}], "has_more": true, "last_id": "a"})),
        ),
        (
            "/v1/models",
            json(200, json!({"data": [{"id": "b", "display_name": "B"}], "has_more": false, "last_id": "b"})),
        ),
    ])
    .await;
    let service = service();
    service.set_key("provider-1", "k-test").await.unwrap();
    let mut config = provider(ProviderKind::Anthropic, &server.url, "");
    config.has_credential = true;
    let models = service.list_models(&config).await.unwrap();
    let ids: Vec<_> = models.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(ids, ["a", "b"]);
    assert_eq!(server.requests().len(), 2);
}
