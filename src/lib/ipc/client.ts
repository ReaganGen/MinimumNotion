// Typed wrappers around Tauri IPC. Components never call `invoke` directly (ARCHITECTURE §8).
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { AppSettings } from "./bindings/AppSettings";
import type { ErrorPayload } from "./bindings/ErrorPayload";
import type { SettingsSnapshot } from "./bindings/SettingsSnapshot";
import type { SettingsUpdate } from "./bindings/SettingsUpdate";

/** Emitted by the native menu with a command ID payload (src-tauri/src/menu.rs). */
export const MENU_EVENT = "forma://menu";

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
  /** Tells the shell the first frame is rendered so it can show the window. */
  appReady: () => call<void>("app_ready"),
};

export function onMenuCommand(handler: (id: string) => void): Promise<UnlistenFn> {
  return listen<string>(MENU_EVENT, (event) => handler(event.payload));
}
