import { ChevronRight, PanelLeft, Sparkles } from "lucide-react";
import { IconButton } from "@/components/layout/IconButton";
import { runCommand, SHORTCUTS } from "@/features/commands/commands";
import type { SaveStatus } from "@/features/editor/saveController";
import type { ShellLayout } from "@/features/layout/layout";
import { useWorkspaceStore } from "@/features/workspace/workspaceStore";
import type { PageRef } from "@/lib/ipc/bindings/PageRef";
import { cn } from "@/lib/utils";

const STATUS_TEXT: Record<SaveStatus, string> = {
  saved: "Saved",
  unsaved: "Edited",
  saving: "Saving…",
  error: "Couldn't save — retrying",
  conflict: "Not saved — see above",
};

// A stable empty list: a selector that returns a new array on every call
// makes the store look changed forever and re-renders without end.
const NO_CRUMBS: PageRef[] = [];

/** The editor's title bar row: panel toggles, breadcrumbs and save status. */
export function EditorTopBar({ layout }: { layout: ShellLayout }) {
  const sidebarDocked = layout.sidebar.visible && layout.sidebar.mode === "docked";
  const crumbs = useWorkspaceStore((s) => (s.activePageId ? s.breadcrumbs : NO_CRUMBS));
  const saveStatus = useWorkspaceStore((s) => (s.activePageId ? s.saveStatus : null));
  const saveError = useWorkspaceStore((s) => s.saveError);
  const openPage = useWorkspaceStore((s) => s.openPage);

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
      <nav aria-label="Breadcrumbs" className="flex min-w-0 items-center text-[13px]">
        {crumbs.map((crumb, index) => (
          <span key={crumb.id} className="flex min-w-0 items-center">
            {index > 0 && (
              <ChevronRight className="mx-0.5 size-3 shrink-0 text-faint-foreground" aria-hidden />
            )}
            <button
              type="button"
              onClick={() => openPage(crumb.id)}
              aria-current={index === crumbs.length - 1 ? "page" : undefined}
              className="max-w-48 truncate rounded px-1.5 py-0.5 text-muted-foreground hover:bg-accent hover:text-foreground aria-[current=page]:text-foreground"
            >
              {crumb.icon ? `${crumb.icon} ` : ""}
              {crumb.title}
            </button>
          </span>
        ))}
      </nav>
      <div data-tauri-drag-region className="h-full flex-1" />
      {saveStatus && (
        <span
          role="status"
          title={saveError ?? undefined}
          className={cn(
            "mr-1 text-[12px] text-faint-foreground",
            (saveStatus === "error" || saveStatus === "conflict") && "text-destructive",
          )}
        >
          {STATUS_TEXT[saveStatus]}
        </span>
      )}
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
