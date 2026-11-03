//! A tiny HTTP server on 127.0.0.1 that answers with canned replies and
//! records every request, so tests exercise the real HTTP client.
#![allow(dead_code)]

use std::sync::{Arc, Mutex};
use std::time::Duration;

use forma_ai::ChatStreamEvent;
use forma_core::settings::{AiSettings, GenerationOptions, ProviderConfig, ProviderKind};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

pub enum Reply {
    Json {
        status: u16,
        body: String,
        headers: Vec<(&'static str, String)>,
    },
    /// A streamed body, sent in small pieces. With `hang`, the connection
    /// then stays open without sending anything more.
    Stream {
        content_type: &'static str,
        body: Vec<u8>,
        hang: bool,
    },
}

pub fn json(status: u16, body: serde_json::Value) -> Reply {
    Reply::Json {
        status,
        body: body.to_string(),
        headers: Vec::new(),
    }
}

pub fn sse(body: &[u8]) -> Reply {
    Reply::Stream {
        content_type: "text/event-stream",
        body: body.to_vec(),
        hang: false,
    }
}

pub fn ndjson(body: &[u8], hang: bool) -> Reply {
    Reply::Stream {
        content_type: "application/x-ndjson",
        body: body.to_vec(),
        hang,
    }
}

#[derive(Debug, Clone)]
pub struct Recorded {
    pub method: String,
    pub path: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl Recorded {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    pub fn json(&self) -> serde_json::Value {
        serde_json::from_str(&self.body).unwrap()
    }
}

pub struct MockServer {
    pub url: String,
    requests: Arc<Mutex<Vec<Recorded>>>,
}

impl MockServer {
    pub fn requests(&self) -> Vec<Recorded> {
        self.requests.lock().unwrap().clone()
    }

    pub fn request_to(&self, path: &str) -> Recorded {
        self.requests()
            .into_iter()
            .find(|r| r.path == path)
            .unwrap_or_else(|| panic!("no request to {path}"))
    }
}

/// Serves `routes` in order: each request takes the first unused route for
/// its path (query ignored). Unknown paths get a 404.
pub async fn serve(routes: Vec<(&'static str, Reply)>) -> MockServer {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let routes = Arc::new(Mutex::new(routes));
    let requests = Arc::new(Mutex::new(Vec::new()));
    let recorded = Arc::clone(&requests);
    tokio::spawn(async move {
        while let Ok((socket, _)) = listener.accept().await {
            tokio::spawn(handle(socket, Arc::clone(&routes), Arc::clone(&recorded)));
        }
    });
    MockServer { url, requests }
}

/// An address where nothing listens.
pub async fn closed_port_url() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    url
}

async fn handle(
    mut socket: TcpStream,
    routes: Arc<Mutex<Vec<(&'static str, Reply)>>>,
    requests: Arc<Mutex<Vec<Recorded>>>,
) {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    let head_end = loop {
        let Ok(n) = socket.read(&mut chunk).await else {
            return;
        };
        if n == 0 {
            return;
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break pos;
        }
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).into_owned();
    let mut lines = head.split("\r\n");
    let mut first = lines.next().unwrap_or_default().split(' ');
    let method = first.next().unwrap_or_default().to_string();
    let target = first.next().unwrap_or_default().to_string();
    let headers: Vec<(String, String)> = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(n, v)| (n.trim().to_string(), v.trim().to_string()))
        .collect();
    let length = headers
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, v)| v.parse::<usize>().ok())
        .unwrap_or(0);
    let mut body = buf[head_end + 4..].to_vec();
    while body.len() < length {
        let Ok(n) = socket.read(&mut chunk).await else {
            return;
        };
        if n == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..n]);
    }
    let path = target.split('?').next().unwrap_or_default().to_string();
    requests.lock().unwrap().push(Recorded {
        method,
        path: path.clone(),
        headers,
        body: String::from_utf8_lossy(&body).into_owned(),
    });

    let reply = {
        let mut routes = routes.lock().unwrap();
        routes
            .iter()
            .position(|(p, _)| *p == path)
            .map(|i| routes.remove(i).1)
    };
    match reply {
        Some(Reply::Json {
            status,
            body,
            headers,
        }) => {
            let extra: String = headers
                .iter()
                .map(|(n, v)| format!("{n}: {v}\r\n"))
                .collect();
            let response = format!(
                "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n{extra}\r\n{body}",
                body.len()
            );
            let _ = socket.write_all(response.as_bytes()).await;
        }
        Some(Reply::Stream {
            content_type,
            body,
            hang,
        }) => {
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nConnection: close\r\n\r\n"
            );
            if socket.write_all(head.as_bytes()).await.is_err() {
                return;
            }
            for piece in body.chunks(23) {
                if socket.write_all(piece).await.is_err() {
                    return;
                }
                let _ = socket.flush().await;
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
            if hang {
                tokio::time::sleep(Duration::from_secs(600)).await;
            }
        }
        None => {
            let body = r#"{"error":{"message":"no such route"}}"#;
            let response = format!(
                "HTTP/1.1 404 Not Found\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = socket.write_all(response.as_bytes()).await;
        }
    }
    let _ = socket.shutdown().await;
}

pub fn provider(kind: ProviderKind, base_url: &str, model: &str) -> ProviderConfig {
    ProviderConfig {
        id: "provider-1".into(),
        kind,
        name: forma_ai::presets::preset(kind).name.into(),
        base_url: base_url.into(),
        model: model.into(),
        has_credential: false,
        options: GenerationOptions::default(),
    }
}

pub fn enabled(config: ProviderConfig) -> AiSettings {
    AiSettings {
        enabled: true,
        active_provider: Some(config.id.clone()),
        providers: vec![config],
        cloud_consent: Default::default(),
    }
}

pub fn text_of(events: &[ChatStreamEvent]) -> String {
    events
        .iter()
        .filter_map(|e| match e {
            ChatStreamEvent::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

pub fn failure(events: &[ChatStreamEvent]) -> Option<&forma_ai::AiError> {
    events.iter().find_map(|e| match e {
        ChatStreamEvent::Failed { error } => Some(error),
        _ => None,
    })
}
