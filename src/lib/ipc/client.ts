// Typed wrappers around Tauri IPC. Components never call `invoke` directly (ARCHITECTURE §8).
import { Channel, invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { AppSettings } from "./bindings/AppSettings";
import type { ArchiveResult } from "./bindings/ArchiveResult";
import type { CellInput } from "./bindings/CellInput";
import type { ChangeOutcome } from "./bindings/ChangeOutcome";
import type { ChatInput } from "./bindings/ChatInput";
import type { ChatStreamEvent } from "./bindings/ChatStreamEvent";
import type { CollectionData } from "./bindings/CollectionData";
import type { CollectionViews } from "./bindings/CollectionViews";
import type { ConnectionReport } from "./bindings/ConnectionReport";
import type { EntryData } from "./bindings/EntryData";
import type { ErrorPayload } from "./bindings/ErrorPayload";
import type { ModelCapabilities } from "./bindings/ModelCapabilities";
import type { ModelInfo } from "./bindings/ModelInfo";
import type { OperationInfo } from "./bindings/OperationInfo";
import type { PageDocument } from "./bindings/PageDocument";
import type { PageHit } from "./bindings/PageHit";
import type { PageLinkInfo } from "./bindings/PageLinkInfo";
import type { PageSummary } from "./bindings/PageSummary";
import type { ProviderKind } from "./bindings/ProviderKind";
import type { ProviderUpdate } from "./bindings/ProviderUpdate";
import type { RecoveryBuffer } from "./bindings/RecoveryBuffer";
import type { SaveOutcome } from "./bindings/SaveOutcome";
import type { Schema } from "./bindings/Schema";
import type { SchemaOp } from "./bindings/SchemaOp";
import type { SettingsSnapshot } from "./bindings/SettingsSnapshot";
import type { SettingsUpdate } from "./bindings/SettingsUpdate";
import type { ThemePreference } from "./bindings/ThemePreference";
import type { TrashEntryInfo } from "./bindings/TrashEntryInfo";
import type { WikiResolution } from "./bindings/WikiResolution";
import type { WorkspaceChanges } from "./bindings/WorkspaceChanges";
import type { WorkspaceInfo } from "./bindings/WorkspaceInfo";

/** Emitted by the native menu with a command ID payload (src-tauri/src/menu.rs). */
export const MENU_EVENT = "diaspar://menu";
/** Emitted with a `WorkspaceChanges` payload when pages change (src-tauri/src/session.rs). */
export const WORKSPACE_CHANGED_EVENT = "workspace://changed";
/** Emitted when the app is about to quit; answer with `ipc.confirmQuit()` (src-tauri/src/main.rs). */
export const FLUSH_BEFORE_QUIT_EVENT = "diaspar://flush-before-quit";

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

  search: (query: string) => call<PageHit[]>("search", { query }),
  quickOpen: (query: string) => call<PageHit[]>("quick_open", { query }),
  backlinks: (id: string) => call<PageHit[]>("backlinks", { id }),
  resolvePages: (ids: string[]) => call<PageLinkInfo[]>("resolve_pages", { ids }),
  resolveWikilinks: (texts: string[]) => call<WikiResolution[]>("resolve_wikilinks", { texts }),
  /** Writes the page's ID into its file before a link to it is created. */
  ensurePersistedId: (id: string) => call<void>("ensure_persisted_id", { id }),
  rebuildIndex: () => call<void>("rebuild_index"),

  // Settings → Data. Each opens its own native dialog in Rust; null when cancelled.
  exportWorkspace: () => call<ArchiveResult | null>("export_workspace"),
  backupWorkspace: () => call<ArchiveResult | null>("backup_workspace"),
  restoreBackup: () => call<WorkspaceInfo | null>("restore_backup"),
  readLog: () => call<string>("read_log"),
  diagnostics: () => call<string>("diagnostics"),

  getCollection: (id: string) => call<CollectionData>("get_collection", { id }),
  getEntry: (id: string) => call<EntryData | null>("get_entry", { id }),
  createCollection: (parentId: string | null, title: string) =>
    call<PageSummary>("create_collection", { parentId, title }),
  convertToCollection: (id: string) => call<void>("convert_to_collection", { id }),
  removeCollection: (id: string) => call<void>("remove_collection", { id }),
  updateSchema: (id: string, op: SchemaOp) => call<Schema>("update_schema", { id, op }),
  setProperty: (pageId: string, propertyId: string, value: CellInput) =>
    call<void>("set_property", { pageId, propertyId, value }),
  getViews: (id: string) => call<CollectionViews>("get_views", { id }),
  saveViews: (id: string, views: CollectionViews) => call<void>("save_views", { id, views }),

  aiSetEnabled: (enabled: boolean) => call<AppSettings>("ai_set_enabled", { enabled }),
  aiAddProvider: (kind: ProviderKind) => call<AppSettings>("ai_add_provider", { kind }),
  aiUpdateProvider: (id: string, update: ProviderUpdate) =>
    call<AppSettings>("ai_update_provider", { id, update }),
  /** Also deletes the provider's key from the Keychain. */
  aiRemoveProvider: (id: string) => call<AppSettings>("ai_remove_provider", { id }),
  aiSetActiveProvider: (id: string) => call<AppSettings>("ai_set_active_provider", { id }),
  /** Hands the key to the Keychain. No command ever sends it back. */
  aiSetApiKey: (id: string, key: string) => call<AppSettings>("ai_set_api_key", { id, key }),
  aiRemoveApiKey: (id: string) => call<AppSettings>("ai_remove_api_key", { id }),
  /** Records that the user agreed to send messages to the provider's host. */
  aiAcknowledgeCloud: (id: string) => call<AppSettings>("ai_acknowledge_cloud", { id }),
  aiListModels: (id: string) => call<ModelInfo[]>("ai_list_models", { id }),
  aiModelCapabilities: (id: string) => call<ModelCapabilities>("ai_model_capabilities", { id }),
  aiTestConnection: (id: string) => call<ConnectionReport>("ai_test_connection", { id }),
  /** Streams one answer into `onEvent`. Resolves when it ends; failures arrive as events. */
  aiChat: (requestId: string, input: ChatInput, onEvent: (event: ChatStreamEvent) => void) => {
    const channel = new Channel<ChatStreamEvent>(onEvent);
    return call<void>("ai_chat", { requestId, input, onEvent: channel });
  },
  aiCancel: (requestId: string) => call<void>("ai_cancel", { requestId }),

  // AI change proposals (ARCHITECTURE §10.3): applied only from the review dialog.
  applyChangeset: (changesetId: string, changeIds: string[]) =>
    call<ChangeOutcome>("apply_changeset", { changesetId, changeIds }),
  rejectChangeset: (changesetId: string) => call<void>("reject_changeset", { changesetId }),
  listOperations: () => call<OperationInfo[]>("list_operations"),
  undoOperation: (opId: string) => call<ChangeOutcome>("undo_operation", { opId }),

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
  return `diaspar-asset://localhost/${pageId}/${reference}`;
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
