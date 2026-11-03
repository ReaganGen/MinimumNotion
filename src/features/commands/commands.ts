// App-level commands. Their shortcuts are native menu accelerators (src-tauri/src/menu.rs),
// so macOS shows them in the menu bar and the webview never handles these keys itself.
// A Rust test checks that every menu ID appears in MENU_COMMANDS.
import type { UnlistenFn } from "@tauri-apps/api/event";
import { useSettingsStore } from "@/features/settings/settingsStore";
import { useUiStore } from "@/features/ui/uiStore";
import { onMenuCommand } from "@/lib/ipc/client";

export const MENU_COMMANDS = [
  "toggle-sidebar",
  "toggle-ai-panel",
  "open-settings",
  "open-search",
  "open-quick-open",
] as const;

export type CommandId = (typeof MENU_COMMANDS)[number];

/** Shortcut labels for display. Keep in sync with the accelerators in menu.rs. */
export const SHORTCUTS: Record<CommandId, { label: string; keys: string }> = {
  "toggle-sidebar": { label: "Toggle sidebar", keys: "⌘⇧L" },
  "toggle-ai-panel": { label: "Toggle AI panel", keys: "⌘⇧A" },
  "open-settings": { label: "Settings", keys: "⌘," },
  "open-search": { label: "Search", keys: "⌘K" },
  "open-quick-open": { label: "Quick open", keys: "⌘P" },
};

export function isCommandId(value: string): value is CommandId {
  return (MENU_COMMANDS as readonly string[]).includes(value);
}

export function runCommand(id: CommandId) {
  switch (id) {
    case "toggle-sidebar":
      return useSettingsStore.getState().togglePanel("sidebar");
    case "toggle-ai-panel":
      return useSettingsStore.getState().togglePanel("ai_panel");
    case "open-settings":
      return useUiStore.getState().toggleDialog("settings");
    case "open-search":
      return useUiStore.getState().toggleDialog("search");
    case "open-quick-open":
      return useUiStore.getState().toggleDialog("quick-open");
  }
}

/** Routes native menu clicks and accelerators to `runCommand`. Unknown IDs are ignored. */
export function startMenuBridge(): Promise<UnlistenFn> {
  return onMenuCommand((id) => {
    if (isCommandId(id)) runCommand(id);
  });
}
