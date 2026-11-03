import { useEffect, useState } from "react";
import type { LayoutSettings } from "@/lib/ipc/bindings/LayoutSettings";

/** The editor column never gets narrower than this while a panel is docked beside it. */
export const EDITOR_MIN_WIDTH = 420;

export type PanelMode = "docked" | "overlay";

export interface PanelLayout {
  visible: boolean;
  width: number;
  /** Docked panels take space beside the editor; overlays float above it when space runs out. */
  mode: PanelMode;
}

export interface ShellLayout {
  sidebar: PanelLayout;
  aiPanel: PanelLayout;
}

/**
 * Decides how each panel is shown at a given window width. Visibility always
 * follows the user's choice: a panel that doesn't fit becomes an overlay
 * instead of silently disappearing.
 */
export function computeLayout(windowWidth: number, layout: LayoutSettings): ShellLayout {
  const { sidebar, ai_panel: ai } = layout;

  const sidebarDocks = sidebar.visible && windowWidth - sidebar.width >= EDITOR_MIN_WIDTH;
  const dockedSidebarWidth = sidebarDocks ? sidebar.width : 0;
  const aiDocks = ai.visible && windowWidth - dockedSidebarWidth - ai.width >= EDITOR_MIN_WIDTH;

  return {
    sidebar: {
      visible: sidebar.visible,
      width: sidebar.width,
      mode: sidebarDocks ? "docked" : "overlay",
    },
    aiPanel: { visible: ai.visible, width: ai.width, mode: aiDocks ? "docked" : "overlay" },
  };
}

export function useWindowWidth(): number {
  const [width, setWidth] = useState(() => window.innerWidth);
  useEffect(() => {
    const onResize = () => setWidth(window.innerWidth);
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, []);
  return width;
}
