mod commands;
mod menu;
mod window;

use std::sync::Mutex;

use forma_core::settings::SettingsStore;
use tauri::{Manager, WindowEvent};
use tauri_plugin_window_state::StateFlags;

pub struct AppState {
    pub settings: Mutex<SettingsStore>,
    /// One-time message about the settings file (reset, read-only), shown by the UI.
    pub settings_notice: Mutex<Option<String>>,
}

fn main() {
    tauri::Builder::default()
        // Must be registered first so a second launch focuses this instance (ADR-027).
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(main) = app.get_webview_window(window::MAIN) {
                window::focus(&main);
            }
        }))
        .plugin(
            tauri_plugin_window_state::Builder::new()
                // Visibility is controlled by `app_ready`, not restored.
                .with_state_flags(StateFlags::all() - StateFlags::VISIBLE)
                .build(),
        )
        .setup(|app| {
            // Settings problems never stop the app from opening: it falls back
            // to read-only defaults and the UI shows the notice.
            let (store, notice) = match app.path().app_config_dir() {
                Ok(config_dir) => SettingsStore::load_or_fallback(&config_dir),
                Err(err) => SettingsStore::unavailable(&err.to_string()),
            };
            let theme = store.settings().appearance.theme;
            app.manage(AppState {
                settings: Mutex::new(store),
                settings_notice: Mutex::new(notice),
            });

            menu::install(app.handle())?;

            if let Some(main) = app.get_webview_window(window::MAIN) {
                window::apply_theme(&main, theme);
                window::show_if_frontend_stalls(main);
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::ThemeChanged(theme) = event {
                window::sync_background(window, *theme);
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::save_settings,
            commands::apply_window_theme,
            commands::app_ready,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Forma");
}
