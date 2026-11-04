//! One configured provider: its address, its protocol and its key. Sends
//! requests, maps failures to [`AiError`]s and streams answers.

use std::time::Duration;

use diaspar_core::settings::{ProviderConfig, ProviderKind};
use reqwest::header::RETRY_AFTER;
use reqwest::{Method, RequestBuilder, Response};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use crate::error::{AiError, from_status, from_transport};
use crate::framing::{LineSplitter, SseParser};
use crate::presets::host_of;
use crate::protocol::{Decoder, Frame, Framing, anthropic, ollama, openai_chat, openai_responses};
use crate::types::{ChatEvent, ChatRequest, ModelCapabilities, ModelInfo};

/// Model lists and model details: short requests.
const SHORT_TIMEOUT: Duration = Duration::from_secs(30);
/// Pages of Anthropic's model list read at most.
const MAX_MODEL_PAGES: usize = 10;

/// The client every provider shares.
pub fn http_client() -> Result<reqwest::Client, AiError> {
    reqwest::Client::builder()
        .user_agent(concat!("Diaspar/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(10))
        // Between chunks. A local model can take minutes to load before it
        // sends the first token.
        .read_timeout(Duration::from_secs(300))
        .build()
        .map_err(|e| AiError::invalid_response(format!("couldn't start the HTTP client: {e}")))
}

pub struct Provider<'a> {
    config: &'a ProviderConfig,
    key: Option<&'a str>,
    client: &'a reqwest::Client,
}

impl<'a> Provider<'a> {
    pub fn new(
        config: &'a ProviderConfig,
        key: Option<&'a str>,
        client: &'a reqwest::Client,
    ) -> Self {
        Self {
            config,
            key,
            client,
        }
    }

    fn name(&self) -> &str {
        &self.config.name
    }

    fn base(&self) -> &str {
        &self.config.base_url
    }

    fn request(&self, method: Method, url: String) -> RequestBuilder {
        let mut request = self.client.request(method, url);
        if self.config.kind == ProviderKind::Anthropic {
            request = request.header("anthropic-version", anthropic::API_VERSION);
        }
        // `bearer_auth` marks the header sensitive, so it's never printed.
        if let Some(key) = self.key {
            request = request.bearer_auth(key);
        }
        request
    }

    async fn send(
        &self,
        request: RequestBuilder,
        cancel: Option<&CancellationToken>,
    ) -> Result<Response, AiError> {
        let sending = request.send();
        let result = match cancel {
            Some(token) => tokio::select! {
                biased;
                () = token.cancelled() => return Err(AiError::cancelled()),
                result = sending => result,
            },
            None => sending.await,
        };
        let response =
            result.map_err(|e| from_transport(self.name(), &host_of(self.base()), &e))?;
        let status = response.status();
        if status.is_success() {
            return Ok(response);
        }
        let retry_after = response
            .headers()
            .get(RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.trim().parse::<u32>().ok());
        let body = response.text().await.unwrap_or_default();
        Err(from_status(
            self.name(),
            status.as_u16(),
            retry_after,
            &body,
        ))
    }

    async fn get_json(&self, url: String) -> Result<Value, AiError> {
        let request = self.request(Method::GET, url).timeout(SHORT_TIMEOUT);
        let response = self.send(request, None).await?;
        response.json().await.map_err(AiError::invalid_response)
    }

    async fn post_json(&self, url: String, body: &Value) -> Result<Value, AiError> {
        let request = self
            .request(Method::POST, url)
            .timeout(SHORT_TIMEOUT)
            .json(body);
        let response = self.send(request, None).await?;
        response.json().await.map_err(AiError::invalid_response)
    }

    /// The models the provider offers, from its own API.
    pub async fn list_models(&self) -> Result<Vec<ModelInfo>, AiError> {
        let base = self.base();
        match self.config.kind {
            ProviderKind::Anthropic => {
                let mut models = Vec::new();
                let mut after: Option<String> = None;
                for _ in 0..MAX_MODEL_PAGES {
                    let body = self
                        .get_json(anthropic::models_url(base, after.as_deref()))
                        .await?;
                    let (page, next) = anthropic::parse_models(&body);
                    models.extend(page);
                    match next {
                        Some(id) => after = Some(id),
                        None => break,
                    }
                }
                Ok(models)
            }
            ProviderKind::OpenAi => Ok(openai_responses::parse_models(
                &self.get_json(openai_responses::models_url(base)).await?,
            )),
            ProviderKind::OpenAiCompatible => Ok(openai_chat::parse_models(
                &self.get_json(openai_chat::models_url(base)).await?,
            )),
            // LM Studio's own list has names and sizes; older versions only
            // have the OpenAI-style one.
            ProviderKind::LmStudio => {
                match self.get_json(openai_chat::lmstudio_models_url(base)).await {
                    Ok(body) => Ok(openai_chat::parse_lmstudio_models(&body)),
                    Err(_) => Ok(openai_chat::parse_models(
                        &self.get_json(openai_chat::models_url(base)).await?,
                    )),
                }
            }
            ProviderKind::Ollama => Ok(ollama::parse_tags(
                &self.get_json(ollama::tags_url(base)).await?,
            )),
        }
    }

    /// What `model` can do, as far as the provider says. OpenAI's model list
    /// has no details, so its answers are all Unknown.
    pub async fn capabilities(&self, model: &str) -> Result<ModelCapabilities, AiError> {
        let base = self.base();
        match self.config.kind {
            ProviderKind::Anthropic => Ok(anthropic::parse_capabilities(
                &self.get_json(anthropic::model_url(base, model)).await?,
            )),
            ProviderKind::Ollama => Ok(ollama::parse_show(
                &self
                    .post_json(ollama::show_url(base), &json!({ "model": model }))
                    .await?,
            )),
            ProviderKind::LmStudio => Ok(self
                .get_json(openai_chat::lmstudio_models_url(base))
                .await
                .map(|body| openai_chat::parse_lmstudio_capabilities(&body, model))
                .unwrap_or(ModelCapabilities::UNKNOWN)),
            ProviderKind::OpenAi | ProviderKind::OpenAiCompatible => Ok(ModelCapabilities::UNKNOWN),
        }
    }

    /// Streams the answer to `request` into `on_event`, ending with
    /// `ChatEvent::Done`. Cancelling `cancel` drops the connection.
    pub async fn stream_chat(
        &self,
        request: &ChatRequest,
        cancel: &CancellationToken,
        on_event: &mut (dyn FnMut(ChatEvent) + Send),
    ) -> Result<(), AiError> {
        let base = self.base();
        let (url, body, framing, mut decoder): (String, Value, Framing, Box<dyn Decoder>) =
            match self.config.kind {
                ProviderKind::Anthropic => (
                    anthropic::chat_url(base),
                    anthropic::chat_body(request),
                    Framing::Sse,
                    Box::<anthropic::AnthropicDecoder>::default(),
                ),
                ProviderKind::OpenAi => (
                    openai_responses::chat_url(base),
                    openai_responses::chat_body(request),
                    Framing::Sse,
                    Box::<openai_responses::ResponsesDecoder>::default(),
                ),
                ProviderKind::OpenAiCompatible | ProviderKind::LmStudio => (
                    openai_chat::chat_url(base),
                    openai_chat::chat_body(request),
                    Framing::Sse,
                    Box::<openai_chat::ChatDecoder>::default(),
                ),
                ProviderKind::Ollama => (
                    ollama::chat_url(base),
                    ollama::chat_body(request),
                    Framing::Lines,
                    Box::<ollama::OllamaDecoder>::default(),
                ),
            };
        let mut response = self
            .send(self.request(Method::POST, url).json(&body), Some(cancel))
            .await?;

        let mut sse = SseParser::default();
        let mut lines = LineSplitter::default();
        loop {
            let chunk = tokio::select! {
                biased;
                () = cancel.cancelled() => return Err(AiError::cancelled()),
                chunk = response.chunk() => chunk,
            };
            let chunk = chunk.map_err(|e| from_transport(self.name(), &host_of(base), &e))?;
            let Some(bytes) = chunk else { break };
            let frames: Vec<Frame> = match framing {
                Framing::Sse => sse.feed(&bytes).into_iter().map(Frame::Sse).collect(),
                Framing::Lines => lines.feed(&bytes).into_iter().map(Frame::Line).collect(),
            };
            for frame in frames {
                for event in decoder.decode(frame)? {
                    on_event(event);
                }
                if decoder.is_done() {
                    return Ok(());
                }
            }
        }
        if !decoder.is_done() {
            for event in decoder.finish()? {
                on_event(event);
            }
        }
        Ok(())
    }
}
