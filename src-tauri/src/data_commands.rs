//! Settings → Data: export, backup, restore and diagnostics. Folders and
//! files come from dialogs Rust opens itself, so the webview never supplies
//! a path (ARCHITECTURE §2.1).

use std::path::PathBuf;

use diaspar_core::archive::{self, ArchiveResult, today};
use diaspar_core::diagnostics::sanitized_tail;
use diaspar_core::error::{ErrorCode, ErrorPayload};
use diaspar_core::settings::ProviderKind;
use diaspar_core::workspace::types::WorkspaceInfo;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::{DialogExt, FilePath};

use crate::AppState;
use crate::session::Session;
use crate::workspace_commands::open_and_remember;

type Result<T> = std::result::Result<T, ErrorPayload>;

/// Log lines the viewer shows, and diagnostics include (sanitized).
const VIEWER_LINES: usize = 400;
const DIAGNOSTIC_LINES: usize = 80;

fn invalid(message: impl Into<String>) -> ErrorPayload {
    ErrorPayload::new(ErrorCode::Invalid, message)
}

fn path_of(picked: FilePath) -> Result<PathBuf> {
    picked
        .into_path()
        .map_err(|e| invalid(format!("That location can't be used: {e}")))
}

fn open_info(session: &Session) -> Result<WorkspaceInfo> {
    session
        .info()?
        .ok_or_else(|| ErrorPayload::new(ErrorCode::NotFound, "No workspace is open."))
}

/// Copies the pages, without `.diaspar`, into a new folder the user picks.
#[tauri::command]
pub async fn export_workspace(
    app: AppHandle,
    session: State<'_, Session>,
) -> Result<Option<ArchiveResult>> {
    let info = open_info(&session)?;
    let Some(picked) = app
        .dialog()
        .file()
        .set_title("Choose where to put the exported pages")
        .blocking_pick_folder()
    else {
        return Ok(None);
    };
    let parent = path_of(picked)?;
    let name = format!("{} export {}", info.name, today());
    let summary = session.read(|ws| archive::export_markdown(ws.root(), &parent, &name))?;
    log::info!("exported {} files", summary.files);
    Ok(Some(summary.into()))
}

/// Zips the whole workspace, `.diaspar` included, to a file the user names.
/// The workspace is held while it's read, so the backup is consistent.
#[tauri::command]
pub async fn backup_workspace(
    app: AppHandle,
    session: State<'_, Session>,
) -> Result<Option<ArchiveResult>> {
    let info = open_info(&session)?;
    let Some(picked) = app
        .dialog()
        .file()
        .set_title("Save a backup of this workspace")
        .set_file_name(format!("{} backup {}.zip", info.name, today()))
        .add_filter("Zip archive", &["zip"])
        .blocking_save_file()
    else {
        return Ok(None);
    };
    let mut zip = path_of(picked)?;
    if zip
        .extension()
        .is_none_or(|e| !e.eq_ignore_ascii_case("zip"))
    {
        zip.set_extension("zip");
    }
    let summary = session.read(|ws| archive::backup_zip(ws.root(), &zip, &info.name))?;
    log::info!("backed up {} files", summary.files);
    Ok(Some(summary.into()))
}

/// Unzips a backup into a new folder and opens it there. Nothing existing
/// is overwritten.
#[tauri::command]
pub async fn restore_backup(
    app: AppHandle,
    session: State<'_, Session>,
    state: State<'_, AppState>,
) -> Result<Option<WorkspaceInfo>> {
    let Some(zip) = app
        .dialog()
        .file()
        .set_title("Choose a Diaspar backup to restore")
        .add_filter("Zip archive", &["zip"])
        .blocking_pick_file()
    else {
        return Ok(None);
    };
    let zip = path_of(zip)?;
    let Some(parent) = app
        .dialog()
        .file()
        .set_title("Choose where to restore it (a new folder is made there)")
        .blocking_pick_folder()
    else {
        return Ok(None);
    };
    let parent = path_of(parent)?;
    // A workspace inside the open one would show up as its pages, twice.
    if let Some(info) = session.info()?
        && archive::is_inside(&parent, std::path::Path::new(&info.root))
    {
        return Err(invalid("Choose a folder outside the open workspace."));
    }
    let summary = archive::restore_zip(&zip, &parent).map_err(|e| ErrorPayload::from(&e))?;
    log::info!("restored {} files", summary.files);
    open_and_remember(&app, &session, &state, &summary.path).map(Some)
}

/// The end of the log file, as written, for the viewer on this Mac.
#[tauri::command]
pub async fn read_log(app: AppHandle) -> Result<String> {
    let text = log_text(&app);
    let lines: Vec<&str> = text.lines().collect();
    Ok(lines[lines.len().saturating_sub(VIEWER_LINES)..].join("\n"))
}

fn log_text(app: &AppHandle) -> String {
    app.path()
        .app_log_dir()
        .ok()
        .and_then(|dir| std::fs::read_to_string(dir.join("diaspar.log")).ok())
        .unwrap_or_default()
}

fn macos_version() -> String {
    std::process::Command::new("/usr/bin/sw_vers")
        .arg("-productVersion")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|v| v.trim().to_string())
        .unwrap_or_else(|| "unknown".into())
}

/// A report to paste into a bug report: versions, a settings summary and
/// recent log lines with every path and quoted name removed. It holds no
/// note text, page titles, addresses of custom servers, or keys.
#[tauri::command]
pub async fn diagnostics(
    app: AppHandle,
    session: State<'_, Session>,
    state: State<'_, AppState>,
) -> Result<String> {
    let mut out = vec![
        format!(
            "Diaspar {} ({}), macOS {}",
            app.package_info().version,
            std::env::consts::ARCH,
            macos_version()
        ),
        String::new(),
    ];
    match session.info()? {
        Some(info) => {
            let pages = session
                .read(|ws| Ok(ws.read_view().page_count()))
                .unwrap_or(0);
            out.push(format!(
                "Workspace: open, {pages} pages, {}",
                if info.read_only_reason.is_some() {
                    "read-only"
                } else {
                    "writable"
                }
            ));
        }
        None => out.push("Workspace: none open".into()),
    }
    if let Ok(store) = state.settings.lock() {
        let settings = store.settings();
        out.push(format!(
            "Theme: {:?}. Settings {}.",
            settings.appearance.theme,
            if store.read_only_reason().is_some() {
                "read-only"
            } else {
                "writable"
            }
        ));
        let ai = &settings.ai;
        out.push(format!("AI: {}", if ai.enabled { "on" } else { "off" }));
        for p in &ai.providers {
            let kind = match p.kind {
                ProviderKind::Anthropic => "Anthropic",
                ProviderKind::OpenAi => "OpenAI",
                ProviderKind::OpenAiCompatible => "OpenAI-compatible",
                ProviderKind::Ollama => "Ollama",
                ProviderKind::LmStudio => "LM Studio",
            };
            let local = diaspar_ai::presets::is_local(&p.base_url);
            out.push(format!(
                "  {kind}{}: {}, {}, model {}, key {}, tools {}",
                if ai.active_provider.as_deref() == Some(p.id.as_str()) {
                    " (active)"
                } else {
                    ""
                },
                if local { "on this Mac" } else { "remote" },
                if ai.cloud_consent.contains_key(&p.id) {
                    "agreed"
                } else {
                    "not agreed"
                },
                if p.model.is_empty() {
                    "none"
                } else {
                    p.model.as_str()
                },
                if p.has_credential { "stored" } else { "none" },
                match p.options.tool_calling_override {
                    None => "automatic",
                    Some(true) => "on",
                    Some(false) => "off",
                }
            ));
        }
    }
    out.push(String::new());
    out.push(format!(
        "Recent log (last {DIAGNOSTIC_LINES} lines; paths and quoted names removed):"
    ));
    out.extend(sanitized_tail(&log_text(&app), DIAGNOSTIC_LINES));
    Ok(out.join("\n"))
}
