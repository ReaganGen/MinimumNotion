import type { AppSettings } from "@/lib/ipc/bindings/AppSettings";
import type { LayoutSettings } from "@/lib/ipc/bindings/LayoutSettings";

// Mirrors diaspar-core/src/settings.rs. Rust is authoritative and clamps again on save.
export const WIDTH_RANGES = {
  sidebar: { min: 200, max: 480, default: 260 },
  ai_panel: { min: 300, max: 640, default: 360 },
} as const satisfies Record<keyof LayoutSettings, { min: number; max: number; default: number }>;

export type PanelId = keyof LayoutSettings;

export function clampWidth(panel: PanelId, width: number): number {
  const range = WIDTH_RANGES[panel];
  return Math.round(Math.min(range.max, Math.max(range.min, width)));
}

export function defaultSettings(): AppSettings {
  return {
    format_version: 1,
    appearance: { theme: "system" },
    layout: {
      sidebar: { visible: true, width: WIDTH_RANGES.sidebar.default },
      ai_panel: { visible: false, width: WIDTH_RANGES.ai_panel.default },
    },
    workspace: { last_path: null },
    ai: { enabled: false, active_provider: null, providers: [], cloud_consent: {} },
  };
}
