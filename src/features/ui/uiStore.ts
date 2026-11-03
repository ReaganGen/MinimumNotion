import { create } from "zustand";

export type DialogId = "settings" | "search" | "quick-open" | "trash";

interface UiState {
  /** At most one dialog is open at a time. */
  dialog: DialogId | null;
  /** Opens `dialog`, or closes it if it is already open (pressing ⌘K twice closes search). */
  toggleDialog: (dialog: DialogId) => void;
  closeDialog: () => void;
}

export const useUiStore = create<UiState>()((set, get) => ({
  dialog: null,
  toggleDialog: (dialog) => set({ dialog: get().dialog === dialog ? null : dialog }),
  closeDialog: () => set({ dialog: null }),
}));
