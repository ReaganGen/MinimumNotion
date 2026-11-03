//! AI commands (ARCHITECTURE §9). Settings changes go through the settings
//! store; API keys go into the Keychain and no command ever returns one.

use forma_ai::knowledge::{ChatInput, Library, MAX_SELECTION_CHARS};
use forma_ai::presets::{ProviderUpdate, apply_update, new_provider};
use forma_ai::types::{ModelCapabilities, ModelInfo};
use forma_ai::{AiError, AiErrorKind, AiService, ChatStreamEvent, ConnectionReport};
use forma_core::error::{ErrorCode, ErrorPayload};
use forma_core::settings::{AiSettings, AppSettings, ProviderConfig, ProviderKind};
use forma_core::workspace::changeset::{ChangeOutcome, OperationInfo};
use forma_core::workspace::now_rfc3339;
use tauri::ipc::Channel;
use tauri::{AppHandle, State};

use crate::AppState;
use crate::session::Session;

type Result<T> = std::result::Result<T, ErrorPayload>;

fn poisoned() -> ErrorPayload {
    ErrorPayload::new(ErrorCode::Internal, "settings state is unavailable")
}

fn invalid(message: impl Into<String>) -> ErrorPayload {
    ErrorPayload::new(ErrorCode::Invalid, message)
}

fn payload(err: AiError) -> ErrorPayload {
    let code = match err.kind {
        AiErrorKind::Disabled | AiErrorKind::NotConfigured => ErrorCode::Invalid,
        AiErrorKind::ModelNotFound => ErrorCode::NotFound,
        _ => ErrorCode::Io,
    };
    ErrorPayload::new(code, err.message)
}

fn ai_settings(state: &AppState) -> Result<AiSettings> {
    Ok(state
        .settings
        .lock()
        .map_err(|_| poisoned())?
        .settings()
        .ai
        .clone())
}

fn provider(state: &AppState, id: &str) -> Result<ProviderConfig> {
    ai_settings(state)?
        .provider(id)
        .cloned()
        .ok_or_else(|| invalid("That provider was removed."))
}

/// Applies `change` to the AI settings and saves them.
fn change(
    state: &AppState,
    change: impl FnOnce(&mut AiSettings) -> Result<()>,
) -> Result<AppSettings> {
    let mut store = state.settings.lock().map_err(|_| poisoned())?;
    store.update_ai(change).map(AppSettings::for_ui)
}

fn find<'a>(ai: &'a mut AiSettings, id: &str) -> Result<&'a mut ProviderConfig> {
    ai.providers
        .iter_mut()
        .find(|p| p.id == id)
        .ok_or_else(|| invalid("That provider was removed."))
}

/// Fails early when settings can't be saved, before the Keychain is touched.
fn writable(state: &AppState) -> Result<()> {
    let store = state.settings.lock().map_err(|_| poisoned())?;
    match store.read_only_reason() {
        Some(reason) => Err(ErrorPayload::new(ErrorCode::ReadOnly, reason)),
        None => Ok(()),
    }
}

#[tauri::command]
pub async fn ai_set_enabled(state: State<'_, AppState>, enabled: bool) -> Result<AppSettings> {
    change(&state, |ai| {
        ai.enabled = enabled;
        Ok(())
    })
}

#[tauri::command]
pub async fn ai_add_provider(
    state: State<'_, AppState>,
    kind: ProviderKind,
) -> Result<AppSettings> {
    change(&state, |ai| {
        let config = new_provider(kind, ai);
        if ai.active().is_none() {
            ai.active_provider = Some(config.id.clone());
        }
        ai.providers.push(config);
        Ok(())
    })
}

#[tauri::command]
pub async fn ai_update_provider(
    state: State<'_, AppState>,
    service: State<'_, AiService>,
    id: String,
    update: ProviderUpdate,
) -> Result<AppSettings> {
    let saved = change(&state, |ai| {
        let moved = apply_update(find(ai, &id)?, update).map_err(invalid)?;
        // A new destination needs a new agreement before messages go there.
        if moved {
            ai.cloud_consent.remove(&id);
        }
        Ok(())
    })?;
    service.forget(&id);
    Ok(saved)
}

#[tauri::command]
pub async fn ai_remove_provider(
    state: State<'_, AppState>,
    service: State<'_, AiService>,
    id: String,
) -> Result<AppSettings> {
    writable(&state)?;
    service.remove_key(&id).await.map_err(payload)?;
    service.forget(&id);
    change(&state, |ai| {
        ai.providers.retain(|p| p.id != id);
        ai.cloud_consent.remove(&id);
        if ai.active_provider.as_deref() == Some(id.as_str()) {
            ai.active_provider = ai.providers.first().map(|p| p.id.clone());
        }
        Ok(())
    })
}

#[tauri::command]
pub async fn ai_set_active_provider(state: State<'_, AppState>, id: String) -> Result<AppSettings> {
    change(&state, |ai| {
        find(ai, &id)?;
        ai.active_provider = Some(id);
        Ok(())
    })
}

/// Stores the key in the Keychain. The settings only record that one exists.
#[tauri::command]
pub async fn ai_set_api_key(
    state: State<'_, AppState>,
    service: State<'_, AiService>,
    id: String,
    key: String,
) -> Result<AppSettings> {
    writable(&state)?;
    provider(&state, &id)?;
    service.set_key(&id, &key).await.map_err(payload)?;
    change(&state, |ai| {
        find(ai, &id)?.has_credential = true;
        Ok(())
    })
}

#[tauri::command]
pub async fn ai_remove_api_key(
    state: State<'_, AppState>,
    service: State<'_, AiService>,
    id: String,
) -> Result<AppSettings> {
    writable(&state)?;
    service.remove_key(&id).await.map_err(payload)?;
    change(&state, |ai| {
        find(ai, &id)?.has_credential = false;
        Ok(())
    })
}

/// Records that the user agreed to send messages, and the note excerpts
/// they need, to this provider's host.
#[tauri::command]
pub async fn ai_acknowledge_cloud(state: State<'_, AppState>, id: String) -> Result<AppSettings> {
    change(&state, |ai| {
        find(ai, &id)?;
        ai.cloud_consent.insert(id, now_rfc3339());
        Ok(())
    })
}

#[tauri::command]
pub async fn ai_list_models(
    state: State<'_, AppState>,
    service: State<'_, AiService>,
    id: String,
) -> Result<Vec<ModelInfo>> {
    let config = provider(&state, &id)?;
    service.list_models(&config).await.map_err(payload)
}

#[tauri::command]
pub async fn ai_model_capabilities(
    state: State<'_, AppState>,
    service: State<'_, AiService>,
    id: String,
) -> Result<ModelCapabilities> {
    let config = provider(&state, &id)?;
    service.capabilities(&config).await.map_err(payload)
}

#[tauri::command]
pub async fn ai_test_connection(
    state: State<'_, AppState>,
    service: State<'_, AiService>,
    id: String,
) -> Result<ConnectionReport> {
    let config = provider(&state, &id)?;
    Ok(service.test_connection(&config).await)
}

/// Streams one answer from the active provider into `on_event`. Resolves
/// when the answer ends; failures arrive as a `failed` event. Notes are read
/// from the open workspace through a read-only view, never written.
#[tauri::command]
pub async fn ai_chat(
    state: State<'_, AppState>,
    service: State<'_, AiService>,
    session: State<'_, Session>,
    request_id: String,
    input: ChatInput,
    on_event: Channel<ChatStreamEvent>,
) -> Result<()> {
    if request_id.is_empty() || request_id.len() > 64 {
        return Err(invalid("A chat request needs a short ID."));
    }
    if input
        .context
        .selection
        .as_ref()
        .is_some_and(|s| s.text.chars().count() > MAX_SELECTION_CHARS)
    {
        return Err(invalid(format!(
            "The selection is longer than {MAX_SELECTION_CHARS} characters. Select less text."
        )));
    }
    let ai = ai_settings(&state)?;
    let library = Library::new(session.shared());
    service
        .chat(&ai, &request_id, input, Some(library), |event| {
            // The window may have closed; there's no one left to tell.
            let _ = on_event.send(event);
        })
        .await;
    Ok(())
}

#[tauri::command]
pub async fn ai_cancel(service: State<'_, AiService>, request_id: String) -> Result<()> {
    service.cancel(&request_id);
    Ok(())
}

/// Applies the chosen changes of a proposal: all of them or, if any page
/// changed since it was proposed, none (ARCHITECTURE §10.3). Only the
/// review dialog calls this; no model can reach it.
#[tauri::command]
pub async fn apply_changeset(
    app: AppHandle,
    service: State<'_, AiService>,
    session: State<'_, Session>,
    changeset_id: String,
    change_ids: Vec<String>,
) -> Result<ChangeOutcome> {
    let set = service
        .proposal(&changeset_id)
        .ok_or_else(|| invalid("This proposal is no longer available. Ask again."))?;
    let outcome = session.mutate(&app, |ws| ws.apply_changeset(&set, &change_ids))?;
    if matches!(outcome, ChangeOutcome::Applied { .. }) {
        service.discard_proposal(&changeset_id);
    }
    Ok(outcome)
}

/// Rejects a proposal. It was never written anywhere, so this only forgets it.
#[tauri::command]
pub async fn reject_changeset(service: State<'_, AiService>, changeset_id: String) -> Result<()> {
    service.discard_proposal(&changeset_id);
    Ok(())
}

#[tauri::command]
pub async fn list_operations(session: State<'_, Session>) -> Result<Vec<OperationInfo>> {
    session.read(|ws| Ok(ws.list_operations()))
}

/// Undoes an applied change, or redoes one that was undone (by undoing its undo).
#[tauri::command]
pub async fn undo_operation(
    app: AppHandle,
    session: State<'_, Session>,
    op_id: String,
) -> Result<ChangeOutcome> {
    session.mutate(&app, |ws| ws.undo_operation(&op_id))
}
