//! The open workspace and its file watcher. All access goes through the
//! mutex, which makes the Document Service a single writer (ARCHITECTURE §4.2).

use std::path::Path;
use std::sync::{Arc, Mutex};

use forma_core::error::{CoreResult, ErrorCode, ErrorPayload};
use forma_core::workspace::Workspace;
use forma_core::workspace::types::WorkspaceInfo;
use forma_core::workspace::watcher::{self, Watcher};
use tauri::{AppHandle, Emitter, Runtime};

/// Emitted with a `WorkspaceChanges` payload whenever pages change.
pub const CHANGED_EVENT: &str = "workspace://changed";

#[derive(Default)]
pub struct Session {
    workspace: Arc<Mutex<Option<Workspace>>>,
    watcher: Mutex<Option<Watcher>>,
}

fn poisoned() -> ErrorPayload {
    ErrorPayload::new(ErrorCode::Internal, "the workspace state is unavailable")
}

fn no_workspace() -> ErrorPayload {
    ErrorPayload::new(ErrorCode::NotFound, "No workspace is open.")
}

impl Session {
    /// Opens `path`, replacing any open workspace, and starts watching it.
    pub fn open<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        path: &Path,
    ) -> Result<WorkspaceInfo, ErrorPayload> {
        let mut workspace = Workspace::open(path).map_err(|e| ErrorPayload::from(&e))?;
        let info = workspace.info();
        // The window shows these from `info`; later ones come from `take_notices`.
        workspace.take_notices();
        let root = workspace.root().to_path_buf();
        // Stop the old watcher before the old workspace (and its lock) go away.
        *self.watcher.lock().map_err(|_| poisoned())? = None;
        *self.workspace.lock().map_err(|_| poisoned())? = Some(workspace);

        let shared = Arc::clone(&self.workspace);
        let handle = app.clone();
        match watcher::watch(&root, move || reconcile_and_emit(&shared, &handle)) {
            Ok(w) => *self.watcher.lock().map_err(|_| poisoned())? = Some(w),
            Err(err) => log::warn!("can't watch {root:?} for outside changes: {err}"),
        }
        log::info!("opened workspace {root:?}");
        Ok(info)
    }

    pub fn read<T>(&self, f: impl FnOnce(&Workspace) -> CoreResult<T>) -> Result<T, ErrorPayload> {
        let guard = self.workspace.lock().map_err(|_| poisoned())?;
        let workspace = guard.as_ref().ok_or_else(no_workspace)?;
        f(workspace).map_err(|e| ErrorPayload::from(&e))
    }

    /// Runs a mutation and tells the UI what changed.
    pub fn mutate<T, R: Runtime>(
        &self,
        app: &AppHandle<R>,
        f: impl FnOnce(&mut Workspace) -> CoreResult<T>,
    ) -> Result<T, ErrorPayload> {
        let (result, changes) = {
            let mut guard = self.workspace.lock().map_err(|_| poisoned())?;
            let workspace = guard.as_mut().ok_or_else(no_workspace)?;
            let result = f(workspace);
            (result, workspace.take_changes())
        };
        if !changes.is_empty()
            && let Err(err) = app.emit(CHANGED_EVENT, &changes)
        {
            log::warn!("couldn't send workspace changes to the window: {err}");
        }
        result.map_err(|e| {
            log::warn!("workspace operation failed: {e}");
            ErrorPayload::from(&e)
        })
    }

    /// Runs a change that the UI doesn't need to hear about (e.g. taking notices).
    pub fn mutate_quiet(&self, f: impl FnOnce(&mut Workspace)) -> Result<(), ErrorPayload> {
        let mut guard = self.workspace.lock().map_err(|_| poisoned())?;
        let workspace = guard.as_mut().ok_or_else(no_workspace)?;
        f(workspace);
        Ok(())
    }

    pub fn info(&self) -> Result<Option<WorkspaceInfo>, ErrorPayload> {
        let guard = self.workspace.lock().map_err(|_| poisoned())?;
        Ok(guard.as_ref().map(Workspace::info))
    }
}

fn reconcile_and_emit<R: Runtime>(shared: &Mutex<Option<Workspace>>, app: &AppHandle<R>) {
    let changes = {
        let Ok(mut guard) = shared.lock() else { return };
        let Some(workspace) = guard.as_mut() else {
            return;
        };
        workspace.reconcile();
        workspace.take_changes()
    };
    if !changes.is_empty() {
        log::debug!("outside changes: {changes:?}");
        if let Err(err) = app.emit(CHANGED_EVENT, &changes) {
            log::warn!("couldn't send workspace changes to the window: {err}");
        }
    }
}
