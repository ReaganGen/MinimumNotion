import { create } from "zustand";
import type { AppSettings } from "@/lib/ipc/bindings/AppSettings";
import type { SettingsSnapshot } from "@/lib/ipc/bindings/SettingsSnapshot";
import type { ThemePreference } from "@/lib/ipc/bindings/ThemePreference";
import { ipc } from "@/lib/ipc/client";
import { clampWidth, defaultSettings, type PanelId } from "./defaults";

/** Width changes arrive continuously while dragging, so they are saved after a pause. */
export const SAVE_DEBOUNCE_MS = 300;

interface SettingsState {
  settings: AppSettings;
  readOnly: boolean;
  /** One-time message from the backend, e.g. "settings were reset". */
  notice: string | null;
  saveError: string | null;

  hydrate: (snapshot: SettingsSnapshot) => void;
  setTheme: (theme: ThemePreference) => void;
  togglePanel: (panel: PanelId) => void;
  setPanelVisible: (panel: PanelId, visible: boolean) => void;
  setPanelWidth: (panel: PanelId, width: number) => void;
  dismissNotice: () => void;
}

let saveTimer: ReturnType<typeof setTimeout> | undefined;
// Incremented on every local change, so a slow save can't overwrite newer state.
let revision = 0;

export const useSettingsStore = create<SettingsState>()((set, get) => {
  async function persist() {
    saveTimer = undefined;
    const { settings, readOnly } = get();
    if (readOnly) return;
    const sentRevision = revision;
    try {
      const saved = await ipc.saveSettings({
        appearance: settings.appearance,
        layout: settings.layout,
      });
      if (sentRevision === revision) set({ settings: saved, saveError: null });
    } catch (error) {
      set({ saveError: `Settings couldn't be saved: ${(error as Error).message}` });
    }
  }

  function schedule(immediate: boolean) {
    revision += 1;
    if (saveTimer !== undefined) clearTimeout(saveTimer);
    if (immediate) {
      void persist();
    } else {
      saveTimer = setTimeout(() => void persist(), SAVE_DEBOUNCE_MS);
    }
  }

  function updateLayout(panel: PanelId, patch: Partial<AppSettings["layout"][PanelId]>) {
    const { settings } = get();
    set({
      settings: {
        ...settings,
        layout: { ...settings.layout, [panel]: { ...settings.layout[panel], ...patch } },
      },
    });
  }

  return {
    settings: defaultSettings(),
    readOnly: false,
    notice: null,
    saveError: null,

    hydrate: (snapshot) =>
      set({ settings: snapshot.settings, readOnly: snapshot.read_only, notice: snapshot.notice }),

    setTheme: (theme) => {
      const { settings } = get();
      set({ settings: { ...settings, appearance: { ...settings.appearance, theme } } });
      // Saved right away: the native window appearance follows this setting.
      schedule(true);
    },

    togglePanel: (panel) => get().setPanelVisible(panel, !get().settings.layout[panel].visible),

    setPanelVisible: (panel, visible) => {
      updateLayout(panel, { visible });
      schedule(true);
    },

    setPanelWidth: (panel, width) => {
      updateLayout(panel, { width: clampWidth(panel, width) });
      schedule(false);
    },

    dismissNotice: () => set({ notice: null, saveError: null }),
  };
});

/** Test helper: cancels pending saves and resets module state. */
export function resetSettingsStoreForTests() {
  if (saveTimer !== undefined) clearTimeout(saveTimer);
  saveTimer = undefined;
  revision = 0;
  useSettingsStore.setState({
    settings: defaultSettings(),
    readOnly: false,
    notice: null,
    saveError: null,
  });
}
