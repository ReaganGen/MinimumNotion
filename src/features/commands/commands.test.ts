import { emit } from "@tauri-apps/api/event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { defaultSettings } from "@/features/settings/defaults";
import { resetSettingsStoreForTests, useSettingsStore } from "@/features/settings/settingsStore";
import { useUiStore } from "@/features/ui/uiStore";
import type { SettingsUpdate } from "@/lib/ipc/bindings/SettingsUpdate";
import { MENU_EVENT } from "@/lib/ipc/client";
import { isCommandId, MENU_COMMANDS, runCommand, SHORTCUTS, startMenuBridge } from "./commands";

beforeEach(() => {
  resetSettingsStoreForTests();
  useUiStore.setState({ dialog: null });
  mockIPC(
    (command, args) => {
      if (command === "save_settings") {
        return { ...defaultSettings(), ...(args as { update: SettingsUpdate }).update };
      }
    },
    { shouldMockEvents: true },
  );
});

afterEach(() => clearMocks());

describe("commands", () => {
  it("has a shortcut label for every menu command", () => {
    for (const id of MENU_COMMANDS) expect(SHORTCUTS[id].keys).toMatch(/^⌘/);
  });

  it("recognizes only known command IDs", () => {
    expect(isCommandId("toggle-sidebar")).toBe(true);
    expect(isCommandId("quit")).toBe(false);
  });

  it("toggles panels", () => {
    runCommand("toggle-sidebar");
    expect(useSettingsStore.getState().settings.layout.sidebar.visible).toBe(false);
    runCommand("toggle-ai-panel");
    expect(useSettingsStore.getState().settings.layout.ai_panel.visible).toBe(true);
  });

  it("opens one dialog at a time and closes it on a second press", () => {
    runCommand("open-search");
    expect(useUiStore.getState().dialog).toBe("search");
    runCommand("open-settings");
    expect(useUiStore.getState().dialog).toBe("settings");
    runCommand("open-settings");
    expect(useUiStore.getState().dialog).toBeNull();
  });
});

describe("menu bridge", () => {
  it("runs commands sent by the native menu and ignores unknown IDs", async () => {
    const stop = await startMenuBridge();

    await emit(MENU_EVENT, "open-quick-open");
    expect(useUiStore.getState().dialog).toBe("quick-open");

    await emit(MENU_EVENT, "rm -rf");
    expect(useUiStore.getState().dialog).toBe("quick-open");

    stop();
    await emit(MENU_EVENT, "open-quick-open");
    expect(useUiStore.getState().dialog).toBe("quick-open");
  });
});
