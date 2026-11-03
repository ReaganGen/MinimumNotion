import { create } from "zustand";

export type DialogId = "settings" | "search" | "quick-open" | "trash";
export type SettingsSection = "appearance" | "workspace" | "ai" | "data";

interface UiState {
  /** At most one dialog is open at a time. */
  dialog: DialogId | null;
  /** The section Settings opens at. */
  settingsSection: SettingsSection;
  /** Opens `dialog`, or closes it if it is already open (pressing ⌘K twice closes search). */
  toggleDialog: (dialog: DialogId) => void;
  closeDialog: () => void;
  openSettings: (section: SettingsSection) => void;
  setSettingsSection: (section: SettingsSection) => void;
}

export const useUiStore = create<UiState>()((set, get) => ({
  dialog: null,
  settingsSection: "appearance",
  toggleDialog: (dialog) => set({ dialog: get().dialog === dialog ? null : dialog }),
  closeDialog: () => set({ dialog: null }),
  openSettings: (section) => set({ dialog: "settings", settingsSection: section }),
  setSettingsSection: (section) => set({ settingsSection: section }),
}));
