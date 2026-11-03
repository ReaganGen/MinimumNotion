mod ai_commands;
mod assets;
mod commands;
mod menu;
mod session;
mod window;
mod workspace_commands;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use forma_ai::AiService;
use forma_ai::secrets::{KeychainStore, MemoryStore, SecretStore};
use forma_core::settings::SettingsStore;
use tauri::{AppHandle, Emitter, Manager, RunEvent, Runtime, WindowEvent};
use tauri_plugin_log::{RotationStrategy, Target, TargetKind};
use tauri_plugin_window_state::StateFlags;

use session::Session;

pub struct AppState {
    pub settings: Mutex<SettingsStore>,
    /// One-time message about the settings file (reset, read-only), shown by the UI.
    pub settings_notice: Mutex<Option<String>>,
}

/// Asks the window to save unsaved edits before quitting (ARCHITECTURE §6.4).
const FLUSH_EVENT: &str = "forma://flush-before-quit";
/// Quit anyway if the window doesn't answer in time.
const FLUSH_TIMEOUT: Duration = Duration::from_secs(5);
static QUIT_CONFIRMED: AtomicBool = AtomicBool::new(false);
static FLUSH_REQUESTED: AtomicBool = AtomicBool::new(false);

fn request_flush<R: Runtime>(app: &AppHandle<R>) {
    if FLUSH_REQUESTED.swap(true, Ordering::SeqCst) {
        return;
    }
    if let Err(err) = app.emit(FLUSH_EVENT, ()) {
        log::warn!("couldn't ask the window to save before quitting: {err}");
        QUIT_CONFIRMED.store(true, Ordering::SeqCst);
        app.exit(0);
        return;
    }
    let handle = app.clone();
    thread::spawn(move || {
        thread::sleep(FLUSH_TIMEOUT);
        if !QUIT_CONFIRMED.swap(true, Ordering::SeqCst) {
            log::warn!("the window didn't confirm saving within {FLUSH_TIMEOUT:?}; quitting");
            handle.exit(0);
        }
    });
}

/// Called by the window once unsaved edits are saved (or kept as recovery buffers).
#[tauri::command]
fn confirm_quit(app: AppHandle) {
    QUIT_CONFIRMED.store(true, Ordering::SeqCst);
    app.exit(0);
}

fn main() {
    let app = tauri::Builder::default()
        // Must be registered first so a second launch focuses this instance (ADR-027).
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(main) = app.get_webview_window(window::MAIN) {
                window::focus(&main);
            }
        }))
        // Logs go to ~/Library/Logs/<bundle id>/forma.log and to stdout (audit F7).
        // They never contain note content or credentials.
        .plugin(
            tauri_plugin_log::Builder::new()
                .targets([
                    Target::new(TargetKind::Stdout),
                    Target::new(TargetKind::LogDir {
                        file_name: Some("forma".into()),
                    }),
                ])
                .level(if cfg!(debug_assertions) {
                    log::LevelFilter::Debug
                } else {
                    log::LevelFilter::Info
                })
                // The HTTP stack's debug output is noise, and the less it
                // prints about requests the better (keys are sensitive headers).
                .level_for("hyper", log::LevelFilter::Info)
                .level_for("hyper_util", log::LevelFilter::Info)
                .level_for("h2", log::LevelFilter::Info)
                .level_for("reqwest", log::LevelFilter::Info)
                .max_file_size(5_000_000)
                .rotation_strategy(RotationStrategy::KeepSome(3))
                .build(),
        )
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_window_state::Builder::new()
                // Visibility is controlled by `app_ready`, not restored.
                .with_state_flags(StateFlags::all() - StateFlags::VISIBLE)
                .build(),
        )
        .manage(Session::default())
        .register_uri_scheme_protocol(assets::SCHEME, assets::handle)
        .setup(|app| {
            // Settings problems never stop the app from opening: it falls back
            // to read-only defaults and the UI shows the notice.
            let (store, notice) = match app.path().app_config_dir() {
                Ok(config_dir) => SettingsStore::load_or_fallback(&config_dir),
                Err(err) => SettingsStore::unavailable(&err.to_string()),
            };
            if let Some(reason) = store.read_only_reason() {
                log::warn!("settings are read-only: {reason}");
            }
            let theme = store.settings().appearance.theme;
            app.manage(AppState {
                settings: Mutex::new(store),
                settings_notice: Mutex::new(notice),
            });

            // Keys live in the login keychain (ADR-020). If it can't be
            // opened, keys last only until Forma quits, and the log says so.
            let secrets: Arc<dyn SecretStore> = match KeychainStore::new() {
                Ok(store) => Arc::new(store),
                Err(err) => {
                    log::warn!("the Keychain is unavailable, so API keys won't be saved: {err}");
                    Arc::new(MemoryStore::default())
                }
            };
            app.manage(AiService::new(forma_ai::provider::http_client()?, secrets));

            menu::install(app.handle())?;

            if let Some(main) = app.get_webview_window(window::MAIN) {
                window::apply_theme(&main, theme);
                window::show_if_frontend_stalls(main);
            }
            Ok(())
        })
        .on_window_event(|window, event| match event {
            WindowEvent::ThemeChanged(theme) => window::sync_background(window, *theme),
            WindowEvent::CloseRequested { api, .. } if !QUIT_CONFIRMED.load(Ordering::SeqCst) => {
                api.prevent_close();
                request_flush(window.app_handle());
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::save_settings,
            commands::apply_window_theme,
            commands::app_ready,
            confirm_quit,
            workspace_commands::workspace_open_last,
            workspace_commands::workspace_choose,
            workspace_commands::workspace_create_default,
            workspace_commands::workspace_info,
            workspace_commands::workspace_reveal,
            workspace_commands::list_children,
            workspace_commands::list_favorites,
            workspace_commands::get_page,
            workspace_commands::create_page,
            workspace_commands::save_page,
            workspace_commands::save_page_over_conflict,
            workspace_commands::set_title,
            workspace_commands::set_icon,
            workspace_commands::move_page,
            workspace_commands::duplicate_page,
            workspace_commands::trash_page,
            workspace_commands::list_trash,
            workspace_commands::restore_trash,
            workspace_commands::delete_trash_entry,
            workspace_commands::set_favorite,
            workspace_commands::import_attachment,
            workspace_commands::write_recovery,
            workspace_commands::list_recovery,
            workspace_commands::discard_recovery,
            workspace_commands::snapshot_text,
            workspace_commands::take_notices,
            workspace_commands::open_external,
            workspace_commands::search,
            workspace_commands::quick_open,
            workspace_commands::backlinks,
            workspace_commands::resolve_pages,
            workspace_commands::resolve_wikilinks,
            workspace_commands::ensure_persisted_id,
            workspace_commands::rebuild_index,
            workspace_commands::get_collection,
            workspace_commands::get_entry,
            workspace_commands::create_collection,
            workspace_commands::convert_to_collection,
            workspace_commands::remove_collection,
            workspace_commands::update_schema,
            workspace_commands::set_property,
            workspace_commands::get_views,
            workspace_commands::save_views,
            ai_commands::ai_set_enabled,
            ai_commands::ai_add_provider,
            ai_commands::ai_update_provider,
            ai_commands::ai_remove_provider,
            ai_commands::ai_set_active_provider,
            ai_commands::ai_set_api_key,
            ai_commands::ai_remove_api_key,
            ai_commands::ai_acknowledge_cloud,
            ai_commands::ai_list_models,
            ai_commands::ai_model_capabilities,
            ai_commands::ai_test_connection,
            ai_commands::ai_chat,
            ai_commands::ai_cancel,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Forma");

    app.run(|app, event| {
        if let RunEvent::ExitRequested { api, .. } = event
            && !QUIT_CONFIRMED.load(Ordering::SeqCst)
        {
            api.prevent_exit();
            request_flush(app);
        }
    });
}
