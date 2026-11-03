//! IPC commands. Each one is a thin wrapper around `forma-core`.

use forma_core::error::{ErrorCode, ErrorPayload};
use forma_core::settings::{AppSettings, SettingsSnapshot, SettingsUpdate};
use tauri::{Manager, State, WebviewWindow};

use crate::AppState;
use crate::window;

fn poisoned() -> ErrorPayload {
    ErrorPayload::new(ErrorCode::Internal, "settings state is unavailable")
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Result<SettingsSnapshot, ErrorPayload> {
    let store = state.settings.lock().map_err(|_| poisoned())?;
    let notice = state.settings_notice.lock().map_err(|_| poisoned())?.take();
    Ok(SettingsSnapshot {
        settings: store.settings().for_ui(),
        notice,
        read_only: store.is_read_only(),
    })
}

#[tauri::command]
pub fn save_settings(
    webview_window: WebviewWindow,
    state: State<'_, AppState>,
    update: SettingsUpdate,
) -> Result<AppSettings, ErrorPayload> {
    let mut store = state.settings.lock().map_err(|_| poisoned())?;
    let saved = store
        .update(update)
        .map_err(|e| ErrorPayload::from(&e))?
        .for_ui();
    window::apply_theme(&webview_window, saved.appearance.theme);
    Ok(saved)
}

/// Called by the frontend once its first frame is rendered with the right theme.
#[tauri::command]
pub fn app_ready(webview_window: WebviewWindow) {
    if webview_window.label() == window::MAIN {
        let _ = webview_window.show();
        let _ = webview_window.set_focus();
    } else if let Some(main) = webview_window.app_handle().get_webview_window(window::MAIN) {
        let _ = main.show();
    }
}
