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
  /** Why changes can't be saved, or null when they can. */
  readOnlyReason: string | null;
  /** One-time message from the backend, e.g. "settings were reset". */
  notice: string | null;
  saveError: string | null;

  hydrate: (snapshot: SettingsSnapshot) => void;
  setTheme: (theme: ThemePreference) => void;
  togglePanel: (panel: PanelId) => void;
  setPanelVisible: (panel: PanelId, visible: boolean) => void;
  setPanelWidth: (panel: PanelId, width: number) => void;
  dismissNotice: () => void;
  /** Takes the AI settings from what an AI command saved, keeping local layout edits. */
  applyAi: (saved: AppSettings) => void;
}

let saveTimer: ReturnType<typeof setTimeout> | undefined;
// Incremented on every local change, so a slow save can't overwrite newer state.
let revision = 0;

export const useSettingsStore = create<SettingsState>()((set, get) => {
  async function persist() {
    saveTimer = undefined;
    const { settings, readOnlyReason } = get();
    if (readOnlyReason !== null) return;
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
    readOnlyReason: null,
    notice: null,
    saveError: null,

    hydrate: (snapshot) =>
      set({
        settings: snapshot.settings,
        readOnlyReason: snapshot.read_only_reason,
        notice: snapshot.notice,
      }),

    setTheme: (theme) => {
      const { settings, readOnlyReason } = get();
      set({ settings: { ...settings, appearance: { ...settings.appearance, theme } } });
      if (readOnlyReason !== null) {
        // Nothing can be saved, but the title bar should still match the content.
        // A failure here is cosmetic: the page itself has already switched theme.
        ipc.applyWindowTheme(theme).catch(() => {});
        return;
      }
      // Saved right away: saving also updates the native window appearance.
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

    applyAi: (saved) => set({ settings: { ...get().settings, ai: saved.ai } }),
  };
});

/** Test helper: cancels pending saves and resets module state. */
export function resetSettingsStoreForTests() {
  if (saveTimer !== undefined) clearTimeout(saveTimer);
  saveTimer = undefined;
  revision = 0;
  useSettingsStore.setState({
    settings: defaultSettings(),
    readOnlyReason: null,
    notice: null,
    saveError: null,
  });
}
