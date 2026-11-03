//! The AI service the shell holds for the app's lifetime: keys read from the
//! Keychain, model details, and the chats that are running. Every rule about
//! when a request may be sent lives here, so it's tested without the app.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use forma_core::settings::{AiSettings, ProviderConfig, ProviderKind};
use serde::Serialize;
use tokio_util::sync::CancellationToken;
use ts_rs::TS;

use crate::error::{AiError, AiErrorKind};
use crate::presets::{host_of, is_local, preset};
use crate::protocol::ollama;
use crate::provider::Provider;
use crate::secrets::SecretStore;
use crate::tokens;
use crate::types::{
    ChatEvent, ChatRequest, Message, ModelCapabilities, ModelInfo, Role, StopReason,
};

/// Phase 4 sends no notes, so the model is told it can't see any.
pub const SYSTEM_PROMPT: &str = "You are the assistant in Forma, a note-taking app. \
You can't see the user's notes; answer from this conversation only, and say so if the \
user asks about notes. Reply in the language the user writes in. Use Markdown for \
formatting when it helps.";

/// Room kept for the answer when checking that a conversation fits.
const ANSWER_RESERVE: u32 = 1024;
/// The connection test's request may take this long, including a model load.
const TEST_TIMEOUT: Duration = Duration::from_secs(120);

/// What the AI panel receives while an answer streams.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(export)]
pub enum ChatStreamEvent {
    Text {
        text: String,
    },
    Usage {
        input_tokens: Option<u32>,
        output_tokens: Option<u32>,
    },
    Done {
        stop_reason: StopReason,
    },
    Failed {
        error: AiError,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct ConnectionReport {
    pub ok: bool,
    pub message: String,
    pub error: Option<AiError>,
}

pub struct AiService {
    client: reqwest::Client,
    secrets: Arc<dyn SecretStore>,
    /// Keys read this session, so the Keychain is asked once per provider.
    keys: Mutex<HashMap<String, String>>,
    /// (provider ID, model) → what the provider reported.
    capabilities: Mutex<HashMap<(String, String), ModelCapabilities>>,
    running: Mutex<HashMap<String, CancellationToken>>,
}

fn not_configured(message: impl Into<String>) -> AiError {
    AiError::new(AiErrorKind::NotConfigured, message)
}

fn keychain_error(err: impl std::fmt::Display) -> AiError {
    not_configured(err.to_string())
}

impl AiService {
    pub fn new(client: reqwest::Client, secrets: Arc<dyn SecretStore>) -> Self {
        Self {
            client,
            secrets,
            keys: Mutex::default(),
            capabilities: Mutex::default(),
            running: Mutex::default(),
        }
    }

    /// Stores `key` for `provider_id` in the Keychain.
    pub async fn set_key(&self, provider_id: &str, key: &str) -> Result<(), AiError> {
        let key = key.trim().to_string();
        if key.is_empty() {
            return Err(not_configured("Enter an API key."));
        }
        if key.chars().any(char::is_whitespace) {
            return Err(not_configured("An API key has no spaces or line breaks."));
        }
        let (secrets, account, value) = (
            Arc::clone(&self.secrets),
            provider_id.to_string(),
            key.clone(),
        );
        // Keychain calls can wait on a macOS prompt; keep them off the async workers.
        tokio::task::spawn_blocking(move || secrets.set(&account, &value))
            .await
            .map_err(keychain_error)?
            .map_err(keychain_error)?;
        self.keys
            .lock()
            .unwrap()
            .insert(provider_id.to_string(), key);
        Ok(())
    }

    pub async fn remove_key(&self, provider_id: &str) -> Result<(), AiError> {
        self.keys.lock().unwrap().remove(provider_id);
        let (secrets, account) = (Arc::clone(&self.secrets), provider_id.to_string());
        tokio::task::spawn_blocking(move || secrets.delete(&account))
            .await
            .map_err(keychain_error)?
            .map_err(keychain_error)
    }

    /// Forgets what's cached for a provider after its settings change.
    pub fn forget(&self, provider_id: &str) {
        self.capabilities
            .lock()
            .unwrap()
            .retain(|(id, _), _| id != provider_id);
    }

    async fn key(&self, config: &ProviderConfig) -> Result<Option<String>, AiError> {
        if !config.has_credential {
            if preset(config.kind).key_required {
                return Err(not_configured(format!(
                    "Add an API key for {} in Settings → AI.",
                    config.name
                )));
            }
            return Ok(None);
        }
        if let Some(key) = self.keys.lock().unwrap().get(&config.id) {
            return Ok(Some(key.clone()));
        }
        let (secrets, account) = (Arc::clone(&self.secrets), config.id.clone());
        let stored = tokio::task::spawn_blocking(move || secrets.get(&account))
            .await
            .map_err(keychain_error)?
            .map_err(keychain_error)?;
        let Some(key) = stored else {
            return Err(not_configured(format!(
                "The API key for {} isn't in the Keychain anymore. Add it again in Settings → AI.",
                config.name
            )));
        };
        self.keys
            .lock()
            .unwrap()
            .insert(config.id.clone(), key.clone());
        Ok(Some(key))
    }

    pub async fn list_models(&self, config: &ProviderConfig) -> Result<Vec<ModelInfo>, AiError> {
        let key = self.key(config).await?;
        Provider::new(config, key.as_deref(), &self.client)
            .list_models()
            .await
            .map_err(|e| e.redacted(key.as_deref()))
    }

    /// What the provider's chosen model can do. Cached for the session.
    pub async fn capabilities(
        &self,
        config: &ProviderConfig,
    ) -> Result<ModelCapabilities, AiError> {
        if config.model.is_empty() {
            return Ok(ModelCapabilities::UNKNOWN);
        }
        let cache_key = (config.id.clone(), config.model.clone());
        if let Some(caps) = self.capabilities.lock().unwrap().get(&cache_key) {
            return Ok(caps.clone());
        }
        let key = self.key(config).await?;
        let caps = Provider::new(config, key.as_deref(), &self.client)
            .capabilities(&config.model)
            .await
            .map_err(|e| e.redacted(key.as_deref()))?;
        self.capabilities
            .lock()
            .unwrap()
            .insert(cache_key, caps.clone());
        Ok(caps)
    }

    /// The context window requests use: the user's override, else what the
    /// model reports. Ollama always gets one, capped so memory use stays modest.
    fn context_window(config: &ProviderConfig, caps: &ModelCapabilities) -> Option<u32> {
        let chosen = config
            .options
            .context_window_override
            .or(caps.context_window);
        if config.kind == ProviderKind::Ollama {
            return Some(config.options.context_window_override.unwrap_or_else(|| {
                caps.context_window
                    .map_or(ollama::DEFAULT_CONTEXT_WINDOW, |n| {
                        n.min(ollama::DEFAULT_CONTEXT_WINDOW)
                    })
            }));
        }
        chosen
    }

    fn request(
        config: &ProviderConfig,
        messages: Vec<Message>,
        window: Option<u32>,
    ) -> ChatRequest {
        ChatRequest {
            model: config.model.clone(),
            system: Some(SYSTEM_PROMPT.to_string()),
            messages,
            tools: Vec::new(),
            max_output_tokens: config.options.max_output_tokens,
            temperature: config.options.temperature,
            context_window: window,
        }
    }

    /// Runs one chat turn with the active provider, sending everything that
    /// happens to `on_event`. Always ends with `Done` or `Failed`.
    pub async fn chat(
        &self,
        ai: &AiSettings,
        request_id: &str,
        messages: Vec<Message>,
        mut on_event: impl FnMut(ChatStreamEvent) + Send,
    ) {
        let token = {
            let mut running = self.running.lock().unwrap();
            running.entry(request_id.to_string()).or_default().clone()
        };
        let mut key = None;
        let result = self
            .run_chat(ai, &token, messages, &mut key, &mut on_event)
            .await;
        self.running.lock().unwrap().remove(request_id);
        match result {
            Ok(()) => {}
            Err(error) => {
                let error = error.redacted(key.as_deref());
                if error.kind != AiErrorKind::Cancelled {
                    // Kind and status only: a provider's message can quote what was sent.
                    log::warn!(
                        "AI request {request_id} failed: {:?} (status {:?})",
                        error.kind,
                        error.status
                    );
                }
                on_event(ChatStreamEvent::Failed { error });
            }
        }
    }

    async fn run_chat(
        &self,
        ai: &AiSettings,
        token: &CancellationToken,
        messages: Vec<Message>,
        key_out: &mut Option<String>,
        on_event: &mut (impl FnMut(ChatStreamEvent) + Send),
    ) -> Result<(), AiError> {
        if !ai.enabled {
            return Err(AiError::new(
                AiErrorKind::Disabled,
                "AI is turned off. Turn it on in Settings → AI.",
            ));
        }
        let config = ai
            .active()
            .ok_or_else(|| not_configured("Choose a provider in Settings → AI."))?;
        if config.model.is_empty() {
            return Err(not_configured(format!(
                "Choose a model for {} in Settings → AI.",
                config.name
            )));
        }
        if !is_local(&config.base_url) && !ai.cloud_acknowledged.contains_key(&config.id) {
            return Err(not_configured(format!(
                "Confirm that messages may be sent to {} first.",
                host_of(&config.base_url)
            )));
        }
        if messages.last().is_none_or(|m| m.role != Role::User) {
            return Err(not_configured("There's no message to send."));
        }
        if token.is_cancelled() {
            return Err(AiError::cancelled());
        }

        let key = self.key(config).await?;
        key_out.clone_from(&key);
        // Details help fit the window; a provider that can't give them can
        // still answer, so failing here isn't fatal.
        let caps = self
            .capabilities(config)
            .await
            .unwrap_or(ModelCapabilities::UNKNOWN);
        let window = Self::context_window(config, &caps);
        if let Some(window) = window {
            let needed = tokens::estimate_messages(Some(SYSTEM_PROMPT), &messages);
            let reserve = config
                .options
                .max_output_tokens
                .unwrap_or(ANSWER_RESERVE)
                .min(window / 2);
            if needed + reserve > window {
                return Err(AiError::new(
                    AiErrorKind::ContextOverflow,
                    format!(
                        "This conversation is about {needed} tokens, more than fits in the {window}-token context window. Clear the chat to start over, or raise the context window in Settings → AI."
                    ),
                ));
            }
        }

        let request = Self::request(config, messages, window);
        log::info!(
            "AI request to {} ({}, {} messages)",
            host_of(&config.base_url),
            config.model,
            request.messages.len()
        );
        let provider = Provider::new(config, key.as_deref(), &self.client);
        provider
            .stream_chat(&request, token, &mut |event| match event {
                ChatEvent::TextDelta(text) => on_event(ChatStreamEvent::Text { text }),
                ChatEvent::Usage { input, output } => on_event(ChatStreamEvent::Usage {
                    input_tokens: input,
                    output_tokens: output,
                }),
                ChatEvent::Done(stop_reason) => on_event(ChatStreamEvent::Done { stop_reason }),
                // No tools are offered before Phase 5.
                _ => {}
            })
            .await
    }

    /// Stops a running chat. Also works if the chat hasn't started yet.
    pub fn cancel(&self, request_id: &str) {
        self.running
            .lock()
            .unwrap()
            .entry(request_id.to_string())
            .or_default()
            .cancel();
    }

    /// Lists the models, then, if one is chosen, asks it for a one-word answer.
    pub async fn test_connection(&self, config: &ProviderConfig) -> ConnectionReport {
        let fail = |error: AiError| ConnectionReport {
            ok: false,
            message: error.message.clone(),
            error: Some(error),
        };
        let key = match self.key(config).await {
            Ok(key) => key,
            Err(error) => return fail(error),
        };
        let provider = Provider::new(config, key.as_deref(), &self.client);
        let models = match provider.list_models().await {
            Ok(models) => models,
            Err(error) => return fail(error.redacted(key.as_deref())),
        };
        if config.model.is_empty() {
            return ConnectionReport {
                ok: true,
                message: format!(
                    "Connected to {}. Found {} models; choose one to finish.",
                    config.name,
                    models.len()
                ),
                error: None,
            };
        }
        let caps = self
            .capabilities(config)
            .await
            .unwrap_or(ModelCapabilities::UNKNOWN);
        let mut request = Self::request(
            config,
            vec![Message {
                role: Role::User,
                content: "Reply with the word OK.".into(),
            }],
            Self::context_window(config, &caps),
        );
        request.system = None;
        request.max_output_tokens = Some(16);
        let started = Instant::now();
        let cancel = CancellationToken::new();
        let outcome = tokio::time::timeout(
            TEST_TIMEOUT,
            provider.stream_chat(&request, &cancel, &mut |_| {}),
        )
        .await;
        match outcome {
            Ok(Ok(())) => ConnectionReport {
                ok: true,
                message: format!(
                    "Connected. {} answered in {:.1} s.",
                    config.model,
                    started.elapsed().as_secs_f64()
                ),
                error: None,
            },
            Ok(Err(error)) => fail(error.redacted(key.as_deref())),
            Err(_) => fail(AiError::new(
                AiErrorKind::Timeout,
                format!(
                    "{} didn't answer within {} s.",
                    config.model,
                    TEST_TIMEOUT.as_secs()
                ),
            )),
        }
    }
}
