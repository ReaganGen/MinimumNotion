//! The AI service the shell holds for the app's lifetime: keys read from the
//! Keychain, model details, and the chats that are running. Every rule about
//! when a request may be sent lives here, so it's tested without the app.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use forma_core::settings::{AiSettings, ProviderConfig, ProviderKind};
use forma_core::workspace::changeset::{ChangeSet, ChangeSetPreview};
use serde::Serialize;
use tokio_util::sync::CancellationToken;
use ts_rs::TS;

use crate::error::{AiError, AiErrorKind};
use crate::knowledge::{self, ChatInput, ContextReport, Library, Source, SourceBook};
use crate::presets::{host_of, is_local, preset};
use crate::protocol::ollama;
use crate::provider::Provider;
use crate::secrets::SecretStore;
use crate::tokens;
use crate::tools::{self, ToolActivity};
use crate::types::{
    ChatEvent, ChatRequest, ModelCapabilities, ModelInfo, Role, StopReason, ToolCall, Tri, Turn,
};

/// Room kept for the answer when packing a request.
const ANSWER_RESERVE: u32 = 1024;
/// The window packed for when neither the user nor the provider gives one.
pub const DEFAULT_WINDOW: u32 = 8192;
/// Rounds of tool calls in one answer (ARCHITECTURE §10.2 allows 12).
pub const MAX_TOOL_ROUNDS: usize = 6;
/// Edit mode reads, then proposes, so it gets the full 12.
pub const MAX_EDIT_ROUNDS: usize = 12;
/// Proposals kept for review; older ones are dropped.
const KEEP_PROPOSALS: usize = 20;
/// After this, the model gets no more tools and must answer.
const TOOL_DEADLINE: Duration = Duration::from_secs(10 * 60);
/// The connection test's request may take this long, including a model load.
const TEST_TIMEOUT: Duration = Duration::from_secs(120);

/// What the AI panel receives while an answer streams.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(export)]
pub enum ChatStreamEvent {
    /// What the request includes, sent before it goes out.
    Context {
        context: ContextReport,
    },
    /// Notes given to the model, under the keys it cites.
    Sources {
        sources: Vec<Source>,
    },
    /// The model looked something up, or proposed a change.
    Tool {
        activity: ToolActivity,
    },
    /// Edit mode: changes proposed for the user to review. Nothing has
    /// been written; `apply_changeset` is the user's to call.
    Proposal {
        proposal: ChangeSetPreview,
    },
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
    /// Proposals awaiting review, newest last. Holding one changes nothing.
    proposals: Mutex<Vec<ChangeSet>>,
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
            proposals: Mutex::default(),
        }
    }

    /// A proposal awaiting review.
    pub fn proposal(&self, id: &str) -> Option<ChangeSet> {
        self.proposals
            .lock()
            .unwrap()
            .iter()
            .find(|s| s.id == id)
            .cloned()
    }

    /// Forgets a proposal: rejected, or applied.
    pub fn discard_proposal(&self, id: &str) {
        self.proposals.lock().unwrap().retain(|s| s.id != id);
    }

    fn keep_proposal(&self, set: ChangeSet) {
        let mut kept = self.proposals.lock().unwrap();
        kept.push(set);
        let extra = kept.len().saturating_sub(KEEP_PROPOSALS);
        kept.drain(..extra);
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
        system: Option<String>,
        messages: Vec<Turn>,
        window: Option<u32>,
    ) -> ChatRequest {
        ChatRequest {
            model: config.model.clone(),
            system,
            messages,
            tools: Vec::new(),
            allow_tool_calls: true,
            max_output_tokens: config.options.max_output_tokens,
            temperature: config.options.temperature,
            context_window: window,
        }
    }

    /// Whether to offer the model tools: the user's say first, else what
    /// the provider reported. Unknown counts as no (ADR-019).
    fn tool_calling(config: &ProviderConfig, caps: &ModelCapabilities) -> bool {
        config
            .options
            .tool_calling_override
            .unwrap_or(caps.tool_calling == Tri::Yes)
    }

    /// Runs one chat turn with the active provider, sending everything that
    /// happens to `on_event`. Always ends with `Done` or `Failed`. Notes are
    /// read through `library` only; without one, none are sent.
    pub async fn chat(
        &self,
        ai: &AiSettings,
        request_id: &str,
        input: ChatInput,
        library: Option<Library>,
        mut on_event: impl FnMut(ChatStreamEvent) + Send,
    ) {
        let token = {
            let mut running = self.running.lock().unwrap();
            running.entry(request_id.to_string()).or_default().clone()
        };
        let mut key = None;
        let result = self
            .run_chat(ai, &token, input, library.as_ref(), &mut key, &mut on_event)
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
        input: ChatInput,
        library: Option<&Library>,
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
        if !is_local(&config.base_url) && !ai.cloud_consent.contains_key(&config.id) {
            return Err(not_configured(format!(
                "Confirm that messages and note excerpts may be sent to {} first.",
                host_of(&config.base_url)
            )));
        }
        if input.messages.last().is_none_or(|m| m.role != Role::User) {
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
        let total = window.unwrap_or(DEFAULT_WINDOW);
        let reserve = config
            .options
            .max_output_tokens
            .unwrap_or(ANSWER_RESERVE)
            .min(total / 2);
        let budget = total - reserve;
        let offer_tools = Self::tool_calling(config, &caps);
        let packed = knowledge::pack(
            &input.messages,
            &input.context,
            library,
            budget,
            offer_tools,
        )?;
        on_event(ChatStreamEvent::Context {
            context: packed.report.clone(),
        });
        if !packed.book.sources.is_empty() {
            on_event(ChatStreamEvent::Sources {
                sources: packed.book.sources.clone(),
            });
        }
        let tools_on = packed.report.tools;
        let edit = packed.report.edit;
        log::info!(
            "AI request to {} ({}, {} messages, {} sources, tools {}{})",
            host_of(&config.base_url),
            config.model,
            packed.turns.len(),
            packed.book.sources.len(),
            if tools_on { "on" } else { "off" },
            if edit { ", edit mode" } else { "" }
        );
        let request_text = input
            .messages
            .last()
            .map(|m| m.content.clone())
            .unwrap_or_default();
        let mut proposal = ChangeSet::new(&request_text, &config.name, &config.model);
        let max_rounds = if edit {
            MAX_EDIT_ROUNDS
        } else {
            MAX_TOOL_ROUNDS
        };
        let specs = if edit { tools::act_specs } else { tools::specs };

        let provider = Provider::new(config, key.as_deref(), &self.client);
        let mut turns = packed.turns;
        let mut book = packed.book;
        let mut used = packed.tokens;
        let started = Instant::now();
        let mut wrote_text = false;
        for round in 0..=max_rounds {
            let tools_now = tools_on
                && round < max_rounds
                && used < budget.saturating_mul(9) / 10
                && started.elapsed() < TOOL_DEADLINE;
            let mut request =
                Self::request(config, Some(packed.system.clone()), turns.clone(), window);
            if tools_now {
                request.tools = specs();
            } else if tools_on && round > 0 {
                // Earlier rounds called tools: keep them defined, but answer now.
                request.tools = specs();
                request.allow_tool_calls = false;
            }
            let mut text = String::new();
            let mut calls: Vec<ToolCall> = Vec::new();
            let mut reasoning = Vec::new();
            let mut stop = StopReason::Other;
            provider
                .stream_chat(&request, token, &mut |event| match event {
                    ChatEvent::TextDelta(delta) => {
                        // Text from an earlier round ends with its own paragraph.
                        if text.is_empty() && wrote_text {
                            on_event(ChatStreamEvent::Text {
                                text: "\n\n".into(),
                            });
                        }
                        text.push_str(&delta);
                        wrote_text = true;
                        on_event(ChatStreamEvent::Text { text: delta });
                    }
                    ChatEvent::ToolCallStart { id, name } => calls.push(ToolCall {
                        id,
                        name,
                        arguments: String::new(),
                    }),
                    ChatEvent::ToolCallArgsDelta { id, delta } => {
                        if let Some(call) = calls.iter_mut().rev().find(|c| c.id == id) {
                            call.arguments.push_str(&delta);
                        }
                    }
                    ChatEvent::ToolCallEnd { .. } => {}
                    ChatEvent::Reasoning(block) => reasoning.push(block),
                    ChatEvent::Usage { input, output } => on_event(ChatStreamEvent::Usage {
                        input_tokens: input,
                        output_tokens: output,
                    }),
                    ChatEvent::Done(reason) => stop = reason,
                })
                .await?;
            if !tools_now || calls.is_empty() {
                let stop_reason = if stop == StopReason::ToolUse {
                    StopReason::EndTurn
                } else {
                    stop
                };
                if !proposal.is_empty()
                    && let Some(preview) =
                        library.and_then(|l| l.read(|view| proposal.preview(view)))
                {
                    log::info!("AI proposed {} change(s)", proposal.changes.len());
                    self.keep_proposal(proposal);
                    on_event(ChatStreamEvent::Proposal { proposal: preview });
                }
                on_event(ChatStreamEvent::Done { stop_reason });
                return Ok(());
            }
            if token.is_cancelled() {
                return Err(AiError::cancelled());
            }
            used += tokens::estimate(&text)
                + calls
                    .iter()
                    .map(|c| tokens::estimate(&c.arguments) + tokens::PER_MESSAGE)
                    .sum::<u32>();
            turns.push(Turn::Assistant {
                text,
                tool_calls: calls.clone(),
                reasoning,
            });
            for call in calls {
                let room = budget
                    .saturating_sub(used)
                    .saturating_sub(tokens::PER_MESSAGE);
                let first_new = book.sources.len();
                let output = run_tool(
                    library,
                    &call,
                    &mut book,
                    edit.then_some(&mut proposal),
                    room,
                );
                used += tokens::estimate(&output.content) + tokens::PER_MESSAGE;
                on_event(ChatStreamEvent::Tool {
                    activity: output.activity,
                });
                if book.sources.len() > first_new {
                    on_event(ChatStreamEvent::Sources {
                        sources: book.sources[first_new..].to_vec(),
                    });
                }
                turns.push(Turn::ToolResult {
                    call_id: call.id,
                    name: call.name,
                    content: output.content,
                    is_error: output.is_error,
                });
            }
        }
        unreachable!("the last round offers no tools and returns")
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
            None,
            vec![Turn::User("Reply with the word OK.".into())],
            Self::context_window(config, &caps),
        );
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

/// Runs one tool call against the notes. Without a workspace, the call is
/// answered with an error the model can read.
fn run_tool(
    library: Option<&Library>,
    call: &ToolCall,
    book: &mut SourceBook,
    proposal: Option<&mut ChangeSet>,
    room: u32,
) -> tools::ToolOutput {
    library
        .and_then(|l| {
            l.read(|view| match proposal {
                // Proposal tools exist only in Edit mode; elsewhere their
                // names are unknown tools.
                Some(set) => tools::run_proposal(view, &call.name, &call.arguments, set)
                    .unwrap_or_else(|| tools::run(view, &call.name, &call.arguments, book, room)),
                None => tools::run(view, &call.name, &call.arguments, book, room),
            })
        })
        .unwrap_or_else(|| tools::ToolOutput {
            content: serde_json::json!({ "error": "No workspace is open." }).to_string(),
            is_error: true,
            activity: ToolActivity {
                tool: call.name.clone(),
                label: "No workspace is open".into(),
                failed: true,
            },
        })
}
