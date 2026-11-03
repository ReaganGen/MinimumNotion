import { PanelLeft, Sparkles } from "lucide-react";
import { IconButton } from "@/components/layout/IconButton";
import { runCommand, SHORTCUTS } from "@/features/commands/commands";
import type { ShellLayout } from "@/features/layout/layout";
import { cn } from "@/lib/utils";

/** The editor's title bar row: window drag area plus the panel toggles that aren't already visible. */
export function EditorTopBar({ layout }: { layout: ShellLayout }) {
  const sidebarDocked = layout.sidebar.visible && layout.sidebar.mode === "docked";

  return (
    <header
      data-tauri-drag-region
      className={cn(
        "flex h-(--titlebar-height) shrink-0 items-center gap-1 px-2",
        // Leave room for the traffic lights when no docked sidebar sits under them.
        !sidebarDocked && "pl-(--traffic-lights-inset)",
      )}
    >
      {!layout.sidebar.visible && (
        <IconButton
          label="Show sidebar"
          shortcut={SHORTCUTS["toggle-sidebar"].keys}
          onClick={() => runCommand("toggle-sidebar")}
        >
          <PanelLeft />
        </IconButton>
      )}
      <div data-tauri-drag-region className="h-full flex-1" />
      {!layout.aiPanel.visible && (
        <IconButton
          label="Open AI assistant"
          shortcut={SHORTCUTS["toggle-ai-panel"].keys}
          onClick={() => runCommand("toggle-ai-panel")}
        >
          <Sparkles />
        </IconButton>
      )}
    </header>
  );
}
