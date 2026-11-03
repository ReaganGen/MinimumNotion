import { useEffect, useState } from "react";
import { AiPanel } from "@/components/ai/AiPanel";
import { EditorArea } from "@/components/editor/EditorArea";
import { EditorTopBar } from "@/components/layout/EditorTopBar";
import { NoticeBar } from "@/components/layout/NoticeBar";
import { ResizeHandle } from "@/components/layout/ResizeHandle";
import { SidePanel } from "@/components/layout/SidePanel";
import { Sidebar } from "@/components/sidebar/Sidebar";
import { computeLayout, useWindowWidth } from "@/features/layout/layout";
import { type PanelId, WIDTH_RANGES } from "@/features/settings/defaults";
import { useSettingsStore } from "@/features/settings/settingsStore";

/** Three-panel layout: sidebar, editor, AI panel (ARCHITECTURE §7). */
export function AppShell() {
  const layoutSettings = useSettingsStore((s) => s.settings.layout);
  const setPanelWidth = useSettingsStore((s) => s.setPanelWidth);
  const setPanelVisible = useSettingsStore((s) => s.setPanelVisible);
  const windowWidth = useWindowWidth();
  const layout = computeLayout(windowWidth, layoutSettings);
  const [resizing, setResizing] = useState<PanelId | null>(null);

  const handleProps = (panel: PanelId) => ({
    width: layoutSettings[panel].width,
    min: WIDTH_RANGES[panel].min,
    max: WIDTH_RANGES[panel].max,
    defaultWidth: WIDTH_RANGES[panel].default,
    onResize: (width: number) => setPanelWidth(panel, width),
    onDraggingChange: (dragging: boolean) => setResizing(dragging ? panel : null),
  });

  // A panel floating over the editor (a narrow window) closes with Esc or a
  // click beside it, like a popover.
  const overlays: PanelId[] = [];
  if (layout.sidebar.visible && layout.sidebar.mode === "overlay") overlays.push("sidebar");
  if (layout.aiPanel.visible && layout.aiPanel.mode === "overlay") overlays.push("ai_panel");
  const closeOverlays = () => {
    for (const panel of overlays) setPanelVisible(panel, false);
  };
  const hasOverlay = overlays.length > 0;
  // biome-ignore lint/correctness/useExhaustiveDependencies: re-subscribes when overlays change
  useEffect(() => {
    if (!hasOverlay) return;
    const onKey = (event: KeyboardEvent) => {
      // Menus and dialogs handle their own Esc first.
      if (event.key !== "Escape" || event.defaultPrevented) return;
      if (document.querySelector('[role="dialog"], [role="menu"], [role="listbox"]')) return;
      closeOverlays();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [hasOverlay, overlays.join()]);

  // `clip`, not `hidden`: a hidden panel sits outside the window, and focus
  // must never scroll the shell sideways to reveal it.
  return (
    <div className="relative flex h-full overflow-clip bg-background">
      <SidePanel
        side="left"
        label="Sidebar"
        layout={layout.sidebar}
        resizing={resizing === "sidebar"}
      >
        <Sidebar />
        <ResizeHandle edge="right" label="Resize sidebar" {...handleProps("sidebar")} />
      </SidePanel>

      {hasOverlay && (
        <button
          type="button"
          tabIndex={-1}
          aria-label="Close the floating panel"
          onClick={closeOverlays}
          className="absolute inset-0 z-20 cursor-default bg-transparent"
        />
      )}

      <main className="flex min-w-0 flex-1 flex-col">
        <EditorTopBar layout={layout} />
        <NoticeBar />
        <EditorArea />
      </main>

      <SidePanel
        side="right"
        label="AI assistant"
        layout={layout.aiPanel}
        resizing={resizing === "ai_panel"}
      >
        <AiPanel />
        <ResizeHandle edge="left" label="Resize AI panel" {...handleProps("ai_panel")} />
      </SidePanel>
    </div>
  );
}
