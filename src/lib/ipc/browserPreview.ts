// Development-only stand-in for the Rust backend, used when `vite dev` is opened in a
// regular browser instead of the Forma app. It keeps settings in memory and maps the
// menu accelerators to menu events, because a browser has no native menu.
// Never included in production builds (see main.tsx).
import { emit } from "@tauri-apps/api/event";
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import type { CommandId } from "@/features/commands/commands";
import { defaultSettings } from "@/features/settings/defaults";
import type { SettingsUpdate } from "./bindings/SettingsUpdate";
import { MENU_EVENT } from "./client";

const ACCELERATORS: Record<string, CommandId> = {
  "meta+shift+l": "toggle-sidebar",
  "meta+shift+a": "toggle-ai-panel",
  "meta+,": "open-settings",
  "meta+k": "open-search",
  "meta+p": "open-quick-open",
};

export function installBrowserPreview() {
  let settings = defaultSettings();
  let notice: string | null =
    "Browser preview: running without the Forma app, so settings are kept in memory only.";

  mockWindows("main");
  mockIPC(
    (command, args) => {
      switch (command) {
        case "get_settings": {
          const snapshot = { settings, notice, read_only_reason: null };
          notice = null;
          return snapshot;
        }
        case "save_settings": {
          const { update } = args as { update: SettingsUpdate };
          settings = { ...settings, ...update };
          return settings;
        }
        case "apply_window_theme":
        case "app_ready":
          return null;
        default:
          throw { code: "internal", message: `Not available in browser preview: ${command}` };
      }
    },
    { shouldMockEvents: true },
  );

  window.addEventListener("keydown", (event) => {
    const combo = [event.metaKey && "meta", event.shiftKey && "shift", event.key.toLowerCase()]
      .filter(Boolean)
      .join("+");
    const command = ACCELERATORS[combo];
    if (command) {
      event.preventDefault();
      void emit(MENU_EVENT, command);
    }
  });
}
