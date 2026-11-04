//! Workspace IPC commands. Arguments are page IDs, never paths
//! (ARCHITECTURE §2.1). Commands are `async` so file I/O runs off the main
//! thread.

use std::path::{Path, PathBuf};
use std::process::Command;

use diaspar_core::collection::{CellInput, Schema};
use diaspar_core::error::{ErrorCode, ErrorPayload};
use diaspar_core::workspace::collections::{CollectionData, CollectionViews, EntryData, SchemaOp};
use diaspar_core::workspace::types::{
    AttachmentInfo, PageDocument, PageHit, PageLinkInfo, PageSummary, RecoveryBuffer, SaveOutcome,
    TrashEntryInfo, WikiResolution, WorkspaceInfo,
};
use diaspar_core::workspace::{count_entries, is_workspace, percent_decode};
use tauri::ipc::{InvokeBody, Request};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

use crate::AppState;
use crate::assets;
use crate::session::Session;

type Result<T> = std::result::Result<T, ErrorPayload>;

fn invalid(message: impl Into<String>) -> ErrorPayload {
    ErrorPayload::new(ErrorCode::Invalid, message)
}

/// Opens `path` and remembers it for the next launch.
pub(crate) fn open_and_remember(
    app: &AppHandle,
    session: &Session,
    state: &AppState,
    path: &Path,
) -> Result<WorkspaceInfo> {
    let info = session.open(app, path)?;
    if let Ok(mut settings) = state.settings.lock()
        && let Err(err) = settings.set_last_workspace(Some(info.root.clone()))
    {
        log::warn!("couldn't remember the workspace location: {err}");
    }
    Ok(info)
}

/// Reopens the workspace from the last session, if there was one.
#[tauri::command]
pub async fn workspace_open_last(
    app: AppHandle,
    session: State<'_, Session>,
    state: State<'_, AppState>,
) -> Result<Option<WorkspaceInfo>> {
    if let Some(info) = session.info()? {
        return Ok(Some(info));
    }
    let last = state
        .settings
        .lock()
        .ok()
        .and_then(|s| s.settings().workspace.last_path.clone());
    match last {
        Some(path) => session.open(&app, &PathBuf::from(path)).map(Some),
        None => Ok(None),
    }
}

/// Shows the native folder picker (opened by Rust, so the webview never
/// supplies a path) and opens the chosen folder.
#[tauri::command]
pub async fn workspace_choose(
    app: AppHandle,
    session: State<'_, Session>,
    state: State<'_, AppState>,
) -> Result<Option<WorkspaceInfo>> {
    let picked = app
        .dialog()
        .file()
        .set_title("Choose a folder for your Diaspar workspace")
        .blocking_pick_folder();
    let Some(picked) = picked else {
        return Ok(None);
    };
    let path = picked
        .into_path()
        .map_err(|e| invalid(format!("That folder can't be used: {e}")))?;
    if !confirm_if_large(&app, &path) {
        return Ok(None);
    }
    open_and_remember(&app, &session, &state, &path).map(Some)
}

/// A new workspace with more files and folders than this gets a warning first.
const LARGE_FOLDER: usize = 10_000;

/// Asks before making a very large folder (say, all of Documents) a workspace.
/// Folders that already are workspaces open without asking.
fn confirm_if_large(app: &AppHandle, path: &Path) -> bool {
    if is_workspace(path) || count_entries(path, LARGE_FOLDER) <= LARGE_FOLDER {
        return true;
    }
    let name = path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    );
    app.dialog()
        .message(format!(
            "\"{name}\" holds more than 10,000 files and folders. Diaspar reads every Markdown \
             file in it, which can take a while, and keeps its own data in a hidden .diaspar \
             folder there. A folder just for your notes works best.",
        ))
        .title("Open this large folder?")
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCancelCustom(
            "Open Anyway".into(),
            "Cancel".into(),
        ))
        .blocking_show()
}

/// Creates (or opens) `~/Documents/Diaspar`.
#[tauri::command]
pub async fn workspace_create_default(
    app: AppHandle,
    session: State<'_, Session>,
    state: State<'_, AppState>,
) -> Result<WorkspaceInfo> {
    let documents = app
        .path()
        .document_dir()
        .map_err(|e| invalid(format!("Your Documents folder can't be found: {e}")))?;
    let path = documents.join("Diaspar");
    std::fs::create_dir_all(&path).map_err(|e| {
        ErrorPayload::new(
            ErrorCode::Io,
            format!("Couldn't create {}: {e}", path.display()),
        )
    })?;
    open_and_remember(&app, &session, &state, &path)
}

#[tauri::command]
pub async fn workspace_info(session: State<'_, Session>) -> Result<Option<WorkspaceInfo>> {
    session.info()
}

/// Shows the workspace folder in Finder.
#[tauri::command]
pub async fn workspace_reveal(session: State<'_, Session>) -> Result<()> {
    let root = session.read(|ws| Ok(ws.root().to_path_buf()))?;
    Command::new("open")
        .arg(&root)
        .spawn()
        .map_err(|e| ErrorPayload::new(ErrorCode::Io, format!("Couldn't open Finder: {e}")))?;
    Ok(())
}

#[tauri::command]
pub async fn list_children(
    session: State<'_, Session>,
    parent_id: Option<String>,
) -> Result<Vec<PageSummary>> {
    session.read(|ws| ws.list_children(parent_id.as_deref()))
}

#[tauri::command]
pub async fn list_favorites(session: State<'_, Session>) -> Result<Vec<PageSummary>> {
    session.read(|ws| Ok(ws.list_favorites()))
}

#[tauri::command]
pub async fn get_page(session: State<'_, Session>, id: String) -> Result<PageDocument> {
    session.read(|ws| ws.get_page(&id))
}

#[tauri::command]
pub async fn create_page(
    app: AppHandle,
    session: State<'_, Session>,
    parent_id: Option<String>,
    title: String,
) -> Result<PageSummary> {
    session.mutate(&app, |ws| ws.create_page(parent_id.as_deref(), &title))
}

/// A plain folder: a directory with nothing in it (ADR-047).
#[tauri::command]
pub async fn create_folder(
    app: AppHandle,
    session: State<'_, Session>,
    parent_id: Option<String>,
    title: String,
) -> Result<PageSummary> {
    session.mutate(&app, |ws| ws.create_folder(parent_id.as_deref(), &title))
}

#[tauri::command]
pub async fn turn_folder_into_page(
    app: AppHandle,
    session: State<'_, Session>,
    id: String,
) -> Result<()> {
    session.mutate(&app, |ws| ws.turn_folder_into_page(&id))
}

#[tauri::command]
pub async fn save_page(
    app: AppHandle,
    session: State<'_, Session>,
    id: String,
    base_body_rev: Option<String>,
    body: String,
) -> Result<SaveOutcome> {
    session.mutate(&app, |ws| {
        ws.save_page(&id, base_body_rev.as_deref(), &body)
    })
}

#[tauri::command]
pub async fn save_page_over_conflict(
    app: AppHandle,
    session: State<'_, Session>,
    id: String,
    body: String,
) -> Result<SaveOutcome> {
    session.mutate(&app, |ws| ws.save_page_over_conflict(&id, &body))
}

#[tauri::command]
pub async fn set_title(
    app: AppHandle,
    session: State<'_, Session>,
    id: String,
    title: String,
) -> Result<()> {
    session.mutate(&app, |ws| ws.set_title(&id, &title))
}

#[tauri::command]
pub async fn set_icon(
    app: AppHandle,
    session: State<'_, Session>,
    id: String,
    icon: Option<String>,
) -> Result<()> {
    session.mutate(&app, |ws| ws.set_icon(&id, icon.as_deref()))
}

#[tauri::command]
pub async fn move_page(
    app: AppHandle,
    session: State<'_, Session>,
    id: String,
    new_parent_id: Option<String>,
    position: Option<u32>,
) -> Result<()> {
    session.mutate(&app, |ws| {
        ws.move_page(&id, new_parent_id.as_deref(), position.map(|p| p as usize))
    })
}

#[tauri::command]
pub async fn duplicate_page(
    app: AppHandle,
    session: State<'_, Session>,
    id: String,
) -> Result<PageSummary> {
    session.mutate(&app, |ws| ws.duplicate_page(&id))
}

#[tauri::command]
pub async fn trash_page(
    app: AppHandle,
    session: State<'_, Session>,
    id: String,
) -> Result<TrashEntryInfo> {
    session.mutate(&app, |ws| ws.trash_page(&id))
}

#[tauri::command]
pub async fn list_trash(session: State<'_, Session>) -> Result<Vec<TrashEntryInfo>> {
    session.read(|ws| Ok(ws.list_trash()))
}

#[tauri::command]
pub async fn restore_trash(
    app: AppHandle,
    session: State<'_, Session>,
    entry_id: String,
) -> Result<Option<String>> {
    session.mutate(&app, |ws| ws.restore_trash(&entry_id))
}

#[tauri::command]
pub async fn delete_trash_entry(
    app: AppHandle,
    session: State<'_, Session>,
    entry_id: String,
) -> Result<()> {
    session.mutate(&app, |ws| ws.delete_trash_entry(&entry_id))
}

#[tauri::command]
pub async fn set_favorite(
    app: AppHandle,
    session: State<'_, Session>,
    id: String,
    favorite: bool,
) -> Result<()> {
    session.mutate(&app, |ws| ws.set_favorite(&id, favorite))
}

/// Stores a pasted or dropped file. The bytes arrive as the raw request body;
/// `x-page-id` and the URI-encoded `x-file-name` arrive as headers.
#[tauri::command]
pub async fn import_attachment(
    app: AppHandle,
    session: State<'_, Session>,
    request: Request<'_>,
) -> Result<String> {
    let InvokeBody::Raw(bytes) = request.body() else {
        return Err(invalid("The attachment must be sent as raw bytes."));
    };
    let header = |name: &str| {
        request
            .headers()
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string)
            .ok_or_else(|| invalid(format!("Missing {name}.")))
    };
    let page_id = header("x-page-id")?;
    let file_name = percent_decode(&header("x-file-name")?);

    session.mutate(&app, |ws| ws.import_attachment(&page_id, &file_name, bytes))
}

/// Shows a file dialog and copies the chosen file into the page's
/// `_assets/`. `kind` is what the "/" menu's Media item asks for: `image`,
/// `audio`, `video` or `file` (ADR-049). Returns the reference for the
/// Markdown, or nothing if the dialog was cancelled. The path never crosses IPC.
#[tauri::command]
pub async fn pick_attachment(
    app: AppHandle,
    session: State<'_, Session>,
    page_id: String,
    kind: String,
) -> Result<Option<String>> {
    let dialog = app.dialog().file();
    let dialog = match kind.as_str() {
        "image" => dialog
            .set_title("Choose an image")
            .add_filter("Images", assets::IMAGE_EXTENSIONS),
        "audio" => dialog
            .set_title("Choose an audio file")
            .add_filter("Audio", assets::AUDIO_EXTENSIONS),
        "video" => dialog
            .set_title("Choose a video")
            .add_filter("Video", assets::VIDEO_EXTENSIONS),
        "file" => dialog.set_title("Choose a file to attach"),
        _ => return Err(invalid("Unknown kind of attachment.")),
    };
    let Some(picked) = dialog.blocking_pick_file() else {
        return Ok(None);
    };
    let source = picked
        .into_path()
        .map_err(|e| invalid(format!("That file can't be used: {e}")))?;
    session
        .mutate(&app, |ws| ws.import_attachment_file(&page_id, &source))
        .map(Some)
}

/// The name and size of an attachment, for its file card.
#[tauri::command]
pub async fn attachment_info(
    session: State<'_, Session>,
    page_id: String,
    reference: String,
) -> Result<AttachmentInfo> {
    session.read(|ws| ws.attachment_info(&page_id, &reference))
}

/// Opens an attachment in its default app. Programs, scripts and executable
/// files are refused; Show in Finder still works for them.
#[tauri::command]
pub async fn open_attachment(
    session: State<'_, Session>,
    page_id: String,
    reference: String,
) -> Result<()> {
    let path = session.read(|ws| ws.resolve_asset(&page_id, &reference))?;
    if !assets::may_open(&path) {
        return Err(invalid(
            "Diaspar doesn't open programs or scripts. Use Show in Finder instead.",
        ));
    }
    run_open(Command::new("open").arg(&path))
}

/// Shows an attachment in Finder.
#[tauri::command]
pub async fn reveal_attachment(
    session: State<'_, Session>,
    page_id: String,
    reference: String,
) -> Result<()> {
    let path = session.read(|ws| ws.resolve_asset(&page_id, &reference))?;
    run_open(Command::new("open").arg("-R").arg(&path))
}

fn run_open(command: &mut Command) -> Result<()> {
    command
        .spawn()
        .map_err(|e| ErrorPayload::new(ErrorCode::Io, format!("Couldn't open it: {e}")))?;
    Ok(())
}

#[tauri::command]
pub async fn write_recovery(
    session: State<'_, Session>,
    id: String,
    body: String,
    reason: String,
) -> Result<()> {
    session.read(|ws| ws.write_recovery(&id, &body, &reason))
}

#[tauri::command]
pub async fn list_recovery(session: State<'_, Session>) -> Result<Vec<RecoveryBuffer>> {
    session.read(|ws| Ok(ws.list_recovery()))
}

#[tauri::command]
pub async fn discard_recovery(session: State<'_, Session>, id: String) -> Result<()> {
    session.read(|ws| ws.discard_recovery(&id))
}

#[tauri::command]
pub async fn snapshot_text(
    session: State<'_, Session>,
    id: String,
    text: String,
    reason: String,
) -> Result<()> {
    session.read(|ws| ws.snapshot_text(&id, &text, &reason))
}

#[tauri::command]
pub async fn take_notices(session: State<'_, Session>) -> Result<Vec<String>> {
    let mut notices = Vec::new();
    session.mutate_quiet(|ws| notices = ws.take_notices())?;
    Ok(notices)
}

/// Opens a web or mail link in the default app. Other schemes are refused.
#[tauri::command]
pub async fn open_external(url: String) -> Result<()> {
    let allowed = ["https://", "http://", "mailto:"];
    if !allowed
        .iter()
        .any(|p| url.to_ascii_lowercase().starts_with(p))
    {
        return Err(invalid("Only web and email links can be opened."));
    }
    Command::new("open")
        .arg(&url)
        .spawn()
        .map_err(|e| ErrorPayload::new(ErrorCode::Io, format!("Couldn't open the link: {e}")))?;
    Ok(())
}

// ------------------------------------------------------- search and links

/// Full-text search (⌘K): titles, text, tags and property values.
#[tauri::command]
pub async fn search(session: State<'_, Session>, query: String) -> Result<Vec<PageHit>> {
    session.read(|ws| Ok(ws.search(&query, 50)))
}

/// Titles only (⌘P).
#[tauri::command]
pub async fn quick_open(session: State<'_, Session>, query: String) -> Result<Vec<PageHit>> {
    session.read(|ws| Ok(ws.quick_open(&query, 30)))
}

#[tauri::command]
pub async fn backlinks(session: State<'_, Session>, id: String) -> Result<Vec<PageHit>> {
    session.read(|ws| ws.backlinks(&id))
}

/// Current titles of linked pages, for live link labels.
#[tauri::command]
pub async fn resolve_pages(
    session: State<'_, Session>,
    ids: Vec<String>,
) -> Result<Vec<PageLinkInfo>> {
    session.read(|ws| Ok(ws.resolve_pages(&ids)))
}

#[tauri::command]
pub async fn resolve_wikilinks(
    session: State<'_, Session>,
    texts: Vec<String>,
) -> Result<Vec<WikiResolution>> {
    session.read(|ws| Ok(ws.resolve_wikilinks(&texts)))
}

/// Writes a page's ID into its file before anything links to it (ADR-005).
/// Folders can't be linked to (ADR-047).
#[tauri::command]
pub async fn ensure_persisted_id(
    app: AppHandle,
    session: State<'_, Session>,
    id: String,
) -> Result<()> {
    session.mutate(&app, |ws| ws.ensure_link_target(&id))
}

#[tauri::command]
pub async fn rebuild_index(app: AppHandle, session: State<'_, Session>) -> Result<()> {
    session.mutate(&app, |ws| ws.rebuild_index())
}

// ------------------------------------------------------------ collections

#[tauri::command]
pub async fn get_collection(session: State<'_, Session>, id: String) -> Result<CollectionData> {
    session.read(|ws| ws.get_collection(&id))
}

/// An entry page's properties, or null when the page isn't in a collection.
#[tauri::command]
pub async fn get_entry(session: State<'_, Session>, id: String) -> Result<Option<EntryData>> {
    session.read(|ws| ws.get_entry(&id))
}

#[tauri::command]
pub async fn create_collection(
    app: AppHandle,
    session: State<'_, Session>,
    parent_id: Option<String>,
    title: String,
) -> Result<PageSummary> {
    session.mutate(&app, |ws| {
        ws.create_collection(parent_id.as_deref(), &title)
    })
}

#[tauri::command]
pub async fn convert_to_collection(
    app: AppHandle,
    session: State<'_, Session>,
    id: String,
) -> Result<()> {
    session.mutate(&app, |ws| ws.convert_to_collection(&id))
}

/// "Turn into a regular page".
#[tauri::command]
pub async fn remove_collection(
    app: AppHandle,
    session: State<'_, Session>,
    id: String,
) -> Result<()> {
    session.mutate(&app, |ws| ws.remove_collection(&id))
}

#[tauri::command]
pub async fn update_schema(
    app: AppHandle,
    session: State<'_, Session>,
    id: String,
    op: SchemaOp,
) -> Result<Schema> {
    session.mutate(&app, |ws| ws.update_schema(&id, &op))
}

#[tauri::command]
pub async fn set_property(
    app: AppHandle,
    session: State<'_, Session>,
    page_id: String,
    property_id: String,
    value: CellInput,
) -> Result<()> {
    session.mutate(&app, |ws| ws.set_property(&page_id, &property_id, &value))
}

#[tauri::command]
pub async fn get_views(session: State<'_, Session>, id: String) -> Result<CollectionViews> {
    session.read(|ws| Ok(ws.get_views(&id)))
}

#[tauri::command]
pub async fn save_views(
    app: AppHandle,
    session: State<'_, Session>,
    id: String,
    views: CollectionViews,
) -> Result<()> {
    session.mutate(&app, |ws| ws.save_views(&id, &views))
}
