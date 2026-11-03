//! API keys never reach logs or anything sent to the webview (Phase 4 test
//! plan), even when a provider echoes the key back in an error. Its own test
//! binary, because it installs a global logger that records every crate's
//! output, including the HTTP client's at trace level.

mod common;

use std::sync::{Arc, Mutex};

use common::*;
use forma_ai::AiService;
use forma_ai::knowledge::{AskContext, ChatInput};
use forma_ai::secrets::MemoryStore;
use forma_ai::types::{Message, Role};
use forma_core::settings::{AppSettings, ProviderKind};
use serde_json::json;

const KEY: &str = "sk-test-SECRET-4f9a2c";

struct Capture(Mutex<Vec<String>>);

impl log::Log for Capture {
    fn enabled(&self, _: &log::Metadata) -> bool {
        true
    }
    fn log(&self, record: &log::Record) {
        self.0
            .lock()
            .unwrap()
            .push(format!("{} {}", record.target(), record.args()));
    }
    fn flush(&self) {}
}

#[tokio::test]
async fn keys_never_reach_logs_or_ipc_payloads() {
    let capture: &'static Capture = Box::leak(Box::new(Capture(Mutex::new(Vec::new()))));
    log::set_logger(capture).unwrap();
    log::set_max_level(log::LevelFilter::Trace);

    let echo = json!({"error": {"message": format!("Incorrect API key provided: {KEY}")}});
    let server = serve(vec![
        ("/v1/models", json(401, echo.clone())),
        ("/v1/chat/completions", json(401, echo.clone())),
        ("/v1/models", json(401, echo)),
    ])
    .await;
    let service = AiService::new(
        forma_ai::provider::http_client().unwrap(),
        Arc::new(MemoryStore::default()),
    );
    service.set_key("provider-1", KEY).await.unwrap();
    let mut config = provider(
        ProviderKind::OpenAiCompatible,
        &format!("{}/v1", server.url),
        "m",
    );
    config.has_credential = true;
    let ai = enabled(config.clone());

    let mut payloads = Vec::new();
    let report = service.test_connection(&config).await;
    payloads.push(serde_json::to_string(&report).unwrap());
    service
        .chat(
            &ai,
            "req-1",
            ChatInput {
                messages: vec![Message {
                    role: Role::User,
                    content: "Hi".into(),
                }],
                context: AskContext::default(),
            },
            None,
            |event| payloads.push(serde_json::to_string(&event).unwrap()),
        )
        .await;
    let models = service.list_models(&config).await.unwrap_err();
    payloads.push(serde_json::to_string(&models).unwrap());
    // The settings the webview receives hold no key.
    let settings = AppSettings {
        ai,
        ..AppSettings::default()
    };
    payloads.push(serde_json::to_string(&settings.for_ui()).unwrap());

    // The key really was sent, and really was echoed back.
    assert!(
        server
            .requests()
            .iter()
            .all(|r| r.header("authorization") == Some(&format!("Bearer {KEY}")))
    );
    assert!(payloads.iter().any(|p| p.contains("[key removed]")));
    for payload in &payloads {
        assert!(
            !payload.contains("SECRET"),
            "key in an IPC payload: {payload}"
        );
    }
    let logs = capture.0.lock().unwrap();
    assert!(
        !logs.is_empty(),
        "the logger saw nothing, so the check is meaningless"
    );
    for line in logs.iter() {
        assert!(!line.contains("SECRET"), "key in the log: {line}");
    }
}
