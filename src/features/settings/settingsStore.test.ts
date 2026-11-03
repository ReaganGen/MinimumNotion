import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AppSettings } from "@/lib/ipc/bindings/AppSettings";
import type { SettingsUpdate } from "@/lib/ipc/bindings/SettingsUpdate";
import { ipc } from "@/lib/ipc/client";
import { defaultSettings, WIDTH_RANGES } from "./defaults";
import { resetSettingsStoreForTests, SAVE_DEBOUNCE_MS, useSettingsStore } from "./settingsStore";

vi.mock("@/lib/ipc/client", () => ({
  ipc: {
    saveSettings: vi.fn(),
    applyWindowTheme: vi.fn(),
    getSettings: vi.fn(),
    appReady: vi.fn(),
  },
}));

const saveSettings = vi.mocked(ipc.saveSettings);
const applyWindowTheme = vi.mocked(ipc.applyWindowTheme);

/** Behaves like the backend: stores the update and echoes it back. */
function echoSave(update: SettingsUpdate): Promise<AppSettings> {
  return Promise.resolve({ ...defaultSettings(), ...update });
}

const state = () => useSettingsStore.getState();

beforeEach(() => {
  resetSettingsStoreForTests();
  saveSettings.mockReset();
  saveSettings.mockImplementation(echoSave);
  applyWindowTheme.mockReset();
  applyWindowTheme.mockResolvedValue(undefined);
});

afterEach(() => {
  vi.useRealTimers();
});

describe("settings store", () => {
  it("toggles a panel and saves right away", async () => {
    state().togglePanel("ai_panel");
    expect(state().settings.layout.ai_panel.visible).toBe(true);
    expect(saveSettings).toHaveBeenCalledTimes(1);
    expect(saveSettings.mock.calls[0]?.[0].layout.ai_panel.visible).toBe(true);
  });

  it("sends only the fields the UI may change", () => {
    state().setTheme("dark");
    expect(Object.keys(saveSettings.mock.calls[0]?.[0] ?? {}).sort()).toEqual([
      "appearance",
      "layout",
    ]);
  });

  it("debounces width changes into one save with the final width", async () => {
    vi.useFakeTimers();
    state().setPanelWidth("sidebar", 270);
    state().setPanelWidth("sidebar", 280);
    state().setPanelWidth("sidebar", 300);
    expect(saveSettings).not.toHaveBeenCalled();

    await vi.advanceTimersByTimeAsync(SAVE_DEBOUNCE_MS);
    expect(saveSettings).toHaveBeenCalledTimes(1);
    expect(saveSettings.mock.calls[0]?.[0].layout.sidebar.width).toBe(300);
  });

  it("clamps widths to the allowed range", () => {
    state().setPanelWidth("sidebar", 10);
    expect(state().settings.layout.sidebar.width).toBe(WIDTH_RANGES.sidebar.min);
    state().setPanelWidth("ai_panel", 10_000);
    expect(state().settings.layout.ai_panel.width).toBe(WIDTH_RANGES.ai_panel.max);
  });

  it("doesn't let a slow save overwrite a newer change", async () => {
    let finishFirstSave: (value: AppSettings) => void = () => {};
    saveSettings.mockImplementationOnce(
      () => new Promise<AppSettings>((resolve) => (finishFirstSave = resolve)),
    );

    state().setTheme("dark");
    const staleResponse = { ...defaultSettings(), appearance: { theme: "dark" as const } };
    state().togglePanel("sidebar"); // newer local change while the first save is in flight
    finishFirstSave(staleResponse);
    await Promise.resolve();

    expect(state().settings.layout.sidebar.visible).toBe(false);
  });

  it("never saves read-only settings", () => {
    state().hydrate({ settings: defaultSettings(), notice: "newer", read_only_reason: "newer" });
    state().setTheme("light");
    state().togglePanel("sidebar");
    expect(saveSettings).not.toHaveBeenCalled();
    expect(state().settings.appearance.theme).toBe("light");
  });

  it("still updates the native window theme while read-only", () => {
    state().hydrate({ settings: defaultSettings(), notice: null, read_only_reason: "unavailable" });
    state().setTheme("dark");
    expect(applyWindowTheme).toHaveBeenCalledWith("dark");
    expect(saveSettings).not.toHaveBeenCalled();
  });

  it("ignores a failed native theme update while read-only", async () => {
    applyWindowTheme.mockRejectedValueOnce(new Error("no window"));
    state().hydrate({ settings: defaultSettings(), notice: null, read_only_reason: "unavailable" });
    state().setTheme("dark");
    await Promise.resolve();
    expect(state().settings.appearance.theme).toBe("dark");
    expect(state().saveError).toBeNull();
  });

  it("uses the save path, not the theme-only path, when writable", () => {
    state().setTheme("dark");
    expect(saveSettings).toHaveBeenCalledTimes(1);
    expect(applyWindowTheme).not.toHaveBeenCalled();
  });

  it("reports a failed save", async () => {
    saveSettings.mockRejectedValueOnce(new Error("disk full"));
    state().setTheme("dark");
    await vi.waitFor(() => expect(state().saveError).toContain("disk full"));

    state().dismissNotice();
    expect(state().saveError).toBeNull();
  });

  it("keeps the notice from hydration until dismissed", () => {
    state().hydrate({ settings: defaultSettings(), notice: "reset", read_only_reason: null });
    expect(state().notice).toBe("reset");
    state().dismissNotice();
    expect(state().notice).toBeNull();
  });
});
