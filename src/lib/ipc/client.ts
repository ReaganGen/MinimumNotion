// Typed wrappers around Tauri IPC. Components never call `invoke` directly (ARCHITECTURE §8).
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { AppSettings } from "./bindings/AppSettings";
import type { ErrorPayload } from "./bindings/ErrorPayload";
import type { PageDocument } from "./bindings/PageDocument";
import type { PageSummary } from "./bindings/PageSummary";
import type { RecoveryBuffer } from "./bindings/RecoveryBuffer";
import type { SaveOutcome } from "./bindings/SaveOutcome";
import type { SettingsSnapshot } from "./bindings/SettingsSnapshot";
import type { SettingsUpdate } from "./bindings/SettingsUpdate";
import type { ThemePreference } from "./bindings/ThemePreference";
import type { TrashEntryInfo } from "./bindings/TrashEntryInfo";
import type { WorkspaceChanges } from "./bindings/WorkspaceChanges";
import type { WorkspaceInfo } from "./bindings/WorkspaceInfo";

/** Emitted by the native menu with a command ID payload (src-tauri/src/menu.rs). */
export const MENU_EVENT = "forma://menu";
/** Emitted with a `WorkspaceChanges` payload when pages change (src-tauri/src/session.rs). */
export const WORKSPACE_CHANGED_EVENT = "workspace://changed";
/** Emitted when the app is about to quit; answer with `ipc.confirmQuit()` (src-tauri/src/main.rs). */
export const FLUSH_BEFORE_QUIT_EVENT = "forma://flush-before-quit";

export class IpcError extends Error {
  readonly code: ErrorPayload["code"];

  constructor(payload: ErrorPayload) {
    super(payload.message);
    this.name = "IpcError";
    this.code = payload.code;
  }
}

function isErrorPayload(value: unknown): value is ErrorPayload {
  return (
    typeof value === "object" &&
    value !== null &&
    "code" in value &&
    "message" in value &&
    typeof value.message === "string"
  );
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    if (isErrorPayload(error)) throw new IpcError(error);
    throw new IpcError({ code: "internal", message: String(error) });
  }
}

export const ipc = {
  getSettings: () => call<SettingsSnapshot>("get_settings"),
  saveSettings: (update: SettingsUpdate) => call<AppSettings>("save_settings", { update }),
  /** Sets the native window appearance without saving (used while settings are read-only). */
  applyWindowTheme: (theme: ThemePreference) => call<void>("apply_window_theme", { theme }),
  /** Tells the shell the first frame is rendered so it can show the window. */
  appReady: () => call<void>("app_ready"),
  /** Lets the app quit once unsaved edits are saved. */
  confirmQuit: () => call<void>("confirm_quit"),

  openLastWorkspace: () => call<WorkspaceInfo | null>("workspace_open_last"),
  chooseWorkspace: () => call<WorkspaceInfo | null>("workspace_choose"),
  createDefaultWorkspace: () => call<WorkspaceInfo>("workspace_create_default"),
  workspaceInfo: () => call<WorkspaceInfo | null>("workspace_info"),
  revealWorkspace: () => call<void>("workspace_reveal"),
  takeNotices: () => call<string[]>("take_notices"),

  listChildren: (parentId: string | null) => call<PageSummary[]>("list_children", { parentId }),
  listFavorites: () => call<PageSummary[]>("list_favorites"),
  getPage: (id: string) => call<PageDocument>("get_page", { id }),
  createPage: (parentId: string | null, title: string) =>
    call<PageSummary>("create_page", { parentId, title }),
  /** `baseBodyRev` is the body revision the editor started from (ADR-033). */
  savePage: (id: string, baseBodyRev: string | null, body: string) =>
    call<SaveOutcome>("save_page", { id, baseBodyRev, body }),
  savePageOverConflict: (id: string, body: string) =>
    call<SaveOutcome>("save_page_over_conflict", { id, body }),
  setTitle: (id: string, title: string) => call<void>("set_title", { id, title }),
  setIcon: (id: string, icon: string | null) => call<void>("set_icon", { id, icon }),
  movePage: (id: string, newParentId: string | null, position: number | null) =>
    call<void>("move_page", { id, newParentId, position }),
  duplicatePage: (id: string) => call<PageSummary>("duplicate_page", { id }),
  trashPage: (id: string) => call<TrashEntryInfo>("trash_page", { id }),
  listTrash: () => call<TrashEntryInfo[]>("list_trash"),
  restoreTrash: (entryId: string) => call<string | null>("restore_trash", { entryId }),
  deleteTrashEntry: (entryId: string) => call<void>("delete_trash_entry", { entryId }),
  setFavorite: (id: string, favorite: boolean) => call<void>("set_favorite", { id, favorite }),
  writeRecovery: (id: string, body: string, reason: string) =>
    call<void>("write_recovery", { id, body, reason }),
  listRecovery: () => call<RecoveryBuffer[]>("list_recovery"),
  discardRecovery: (id: string) => call<void>("discard_recovery", { id }),
  snapshotText: (id: string, text: string, reason: string) =>
    call<void>("snapshot_text", { id, text, reason }),
  openExternal: (url: string) => call<void>("open_external", { url }),

  /** Sends the file's bytes raw (no JSON encoding) and returns the Markdown reference. */
  importAttachment: async (pageId: string, file: File): Promise<string> => {
    const bytes = new Uint8Array(await file.arrayBuffer());
    try {
      return await invoke<string>("import_attachment", bytes, {
        headers: { "x-page-id": pageId, "x-file-name": encodeURIComponent(file.name) },
      });
    } catch (error) {
      if (isErrorPayload(error)) throw new IpcError(error);
      throw new IpcError({ code: "internal", message: String(error) });
    }
  },
};

/** URL the webview loads an attachment from (served by src-tauri/src/assets.rs). */
export function assetUrl(pageId: string, reference: string): string {
  return `forma-asset://localhost/${pageId}/${reference}`;
}

export function onMenuCommand(handler: (id: string) => void): Promise<UnlistenFn> {
  return listen<string>(MENU_EVENT, (event) => handler(event.payload));
}

export function onWorkspaceChanged(
  handler: (changes: WorkspaceChanges) => void,
): Promise<UnlistenFn> {
  return listen<WorkspaceChanges>(WORKSPACE_CHANGED_EVENT, (event) => handler(event.payload));
}

export function onFlushBeforeQuit(handler: () => void): Promise<UnlistenFn> {
  return listen(FLUSH_BEFORE_QUIT_EVENT, () => handler());
}
