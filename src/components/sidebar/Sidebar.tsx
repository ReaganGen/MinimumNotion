import type { LucideIcon } from "lucide-react";
import { PanelLeftClose, Search, Settings } from "lucide-react";
import { IconButton } from "@/components/layout/IconButton";
import { Kbd } from "@/components/ui/kbd";
import { runCommand, SHORTCUTS } from "@/features/commands/commands";

export function Sidebar() {
  return (
    <div className="flex h-full flex-col bg-sidebar text-sidebar-foreground">
      <div
        data-tauri-drag-region
        className="flex h-(--titlebar-height) shrink-0 items-center justify-end pr-2 pl-(--traffic-lights-inset)"
      >
        <IconButton
          label="Hide sidebar"
          shortcut={SHORTCUTS["toggle-sidebar"].keys}
          onClick={() => runCommand("toggle-sidebar")}
        >
          <PanelLeftClose />
        </IconButton>
      </div>

      <nav aria-label="Primary" className="px-2">
        <SidebarRow
          icon={Search}
          label="Search"
          shortcut={SHORTCUTS["open-search"].keys}
          onClick={() => runCommand("open-search")}
        />
      </nav>

      <section aria-labelledby="sidebar-pages" className="min-h-0 flex-1 overflow-y-auto px-2 pt-5">
        <h2 id="sidebar-pages" className="px-2 text-xs font-medium text-faint-foreground">
          Pages
        </h2>
        <p className="mt-1.5 px-2 text-[13px] leading-5 text-muted-foreground">
          No workspace is open. Creating and opening workspaces comes in the next build.
        </p>
      </section>

      <div className="px-2 pb-2">
        <SidebarRow
          icon={Settings}
          label="Settings"
          shortcut={SHORTCUTS["open-settings"].keys}
          onClick={() => runCommand("open-settings")}
        />
      </div>
    </div>
  );
}

interface SidebarRowProps {
  icon: LucideIcon;
  label: string;
  shortcut?: string;
  onClick: () => void;
}

function SidebarRow({ icon: Icon, label, shortcut, onClick }: SidebarRowProps) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="group flex h-7 w-full items-center gap-2 rounded-md px-2 text-left text-[13.5px] transition-colors duration-(--duration-fast) hover:bg-accent hover:text-foreground"
    >
      <Icon className="size-4 shrink-0 text-muted-foreground" aria-hidden />
      <span className="min-w-0 flex-1 truncate">{label}</span>
      {shortcut && (
        <Kbd className="opacity-0 transition-opacity duration-(--duration-fast) group-hover:opacity-100 group-focus-visible:opacity-100">
          {shortcut}
        </Kbd>
      )}
    </button>
  );
}
