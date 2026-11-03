// Values that exist on both sides of the IPC boundary (audit F10). Rust is
// authoritative; this test reads its source and fails when the copies drift.
import { describe, expect, it } from "vitest";
import { type CommandId, SHORTCUTS } from "@/features/commands/commands";
import { WIDTH_RANGES } from "@/features/settings/defaults";
import settingsRs from "../../../crates/forma-core/src/settings.rs?raw";
import menuRs from "../../../src-tauri/src/menu.rs?raw";

function rustRange(name: string) {
  const match = settingsRs.match(
    new RegExp(
      `${name}: WidthRange = WidthRange \\{\\s*min: (\\d+),\\s*max: (\\d+),\\s*default: (\\d+),`,
    ),
  );
  if (!match) throw new Error(`${name} not found in settings.rs`);
  return { min: Number(match[1]), max: Number(match[2]), default: Number(match[3]) };
}

/** "CmdOrCtrl+Shift+L" → "⌘⇧L" */
function symbols(accelerator: string): string {
  const keys: Record<string, string> = { CmdOrCtrl: "⌘", Shift: "⇧", Alt: "⌥", Ctrl: "⌃" };
  return accelerator
    .split("+")
    .map((part) => keys[part] ?? part)
    .join("");
}

describe("values mirrored from Rust", () => {
  it("panel width ranges", () => {
    expect(WIDTH_RANGES.sidebar).toEqual(rustRange("SIDEBAR_WIDTH"));
    expect(WIDTH_RANGES.ai_panel).toEqual(rustRange("AI_PANEL_WIDTH"));
  });

  it("shortcut labels match the menu accelerators", () => {
    const menu = new Map(
      [...menuRs.matchAll(/\("([a-z-]+)", "[^"]+", "([^"]+)"\)/g)].map(([, id, accel]) => [
        id as CommandId,
        symbols(accel ?? ""),
      ]),
    );
    expect(menu.size).toBe(Object.keys(SHORTCUTS).length);
    for (const [id, { keys }] of Object.entries(SHORTCUTS)) {
      expect(menu.get(id as CommandId), id).toBe(keys);
    }
  });
});
