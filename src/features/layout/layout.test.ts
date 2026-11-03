import { describe, expect, it } from "vitest";
import type { LayoutSettings } from "@/lib/ipc/bindings/LayoutSettings";
import { computeLayout, EDITOR_MIN_WIDTH } from "./layout";

function settings(
  sidebar: boolean,
  ai: boolean,
  sidebarWidth = 260,
  aiWidth = 360,
): LayoutSettings {
  return {
    sidebar: { visible: sidebar, width: sidebarWidth },
    ai_panel: { visible: ai, width: aiWidth },
  };
}

describe("computeLayout", () => {
  it("docks both panels when the editor keeps its minimum width", () => {
    const layout = computeLayout(260 + 360 + EDITOR_MIN_WIDTH, settings(true, true));
    expect(layout.sidebar.mode).toBe("docked");
    expect(layout.aiPanel.mode).toBe("docked");
  });

  it("turns the AI panel into an overlay first when space runs out", () => {
    const layout = computeLayout(260 + 360 + EDITOR_MIN_WIDTH - 1, settings(true, true));
    expect(layout.sidebar.mode).toBe("docked");
    expect(layout.aiPanel.mode).toBe("overlay");
    expect(layout.aiPanel.visible).toBe(true);
  });

  it("overlays the sidebar in a very narrow window instead of hiding it", () => {
    const layout = computeLayout(640, settings(true, false));
    expect(layout.sidebar).toEqual({ visible: true, width: 260, mode: "overlay" });
  });

  it("lets the AI panel dock when the sidebar is an overlay", () => {
    // The overlaid sidebar doesn't take space, so only the AI panel competes with the editor.
    const layout = computeLayout(800, settings(true, true, 400, 300));
    expect(layout.sidebar.mode).toBe("overlay");
    expect(layout.aiPanel.mode).toBe("docked");
  });

  it("never changes the user's visibility choice", () => {
    for (const width of [600, 900, 1600]) {
      const layout = computeLayout(width, settings(false, true));
      expect(layout.sidebar.visible).toBe(false);
      expect(layout.aiPanel.visible).toBe(true);
    }
  });
});
