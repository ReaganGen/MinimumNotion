import type { LucideIcon } from "lucide-react";
import { FileText, FolderPlus, PanelLeftClose, Plus, Search, Settings, Trash2 } from "lucide-react";
import { IconButton } from "@/components/layout/IconButton";
import { TrashDialog } from "@/components/sidebar/TrashDialog";
import { Kbd } from "@/components/ui/kbd";
import { runCommand, SHORTCUTS } from "@/features/commands/commands";
import { useUiStore } from "@/features/ui/uiStore";
import { useWorkspaceStore } from "@/features/workspace/workspaceStore";
import { cn } from "@/lib/utils";
import { PageTree } from "./PageTree";

export function Sidebar() {
  const status = useWorkspaceStore((s) => s.status);
  const info = useWorkspaceStore((s) => s.info);
  const favorites = useWorkspaceStore((s) => s.favorites);
  const activePageId = useWorkspaceStore((s) => s.activePageId);
  const { openPage, createPage, createFolder } = useWorkspaceStore.getState();
  const dialog = useUiStore((s) => s.dialog);
  const { toggleDialog, closeDialog } = useUiStore.getState();
  const isOpen = status === "open";

  return (
    <div className="flex h-full flex-col bg-sidebar text-sidebar-foreground">
      <div
        data-tauri-drag-region
        className="flex h-(--titlebar-height) shrink-0 items-center gap-1 pr-2 pl-(--traffic-lights-inset)"
      >
        <span
          data-tauri-drag-region
          className="min-w-0 flex-1 truncate text-[13px] font-medium text-foreground"
        >
          {info?.name}
        </span>
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

      <div className="min-h-0 flex-1 overflow-y-auto px-2 pt-4 pb-2">
        {isOpen && favorites.length > 0 && (
          <section aria-labelledby="sidebar-favorites" className="mb-4">
            <h2
              id="sidebar-favorites"
              className="px-2 pb-1 text-xs font-medium text-faint-foreground"
            >
              Favorites
            </h2>
            <ul>
              {favorites.map((page) => (
                <li key={page.id}>
                  <button
                    type="button"
                    onClick={() => openPage(page.id)}
                    className={cn(
                      "flex h-7 w-full items-center gap-2 rounded-md px-2 text-left text-[13.5px]",
                      page.id === activePageId ? "bg-selected text-foreground" : "hover:bg-accent",
                    )}
                  >
                    {page.icon ? (
                      <span className="w-4 text-center text-[14px] leading-none">{page.icon}</span>
                    ) : (
                      <FileText className="size-4 shrink-0 text-muted-foreground" aria-hidden />
                    )}
                    <span className="min-w-0 flex-1 truncate">{page.title}</span>
                  </button>
                </li>
              ))}
            </ul>
          </section>
        )}

        <section aria-labelledby="sidebar-pages">
          <div className="group flex items-center pr-1">
            <h2
              id="sidebar-pages"
              className="flex-1 px-2 pb-1 text-xs font-medium text-faint-foreground"
            >
              Pages
            </h2>
            {isOpen && (
              <>
                <IconButton
                  label="New folder"
                  onClick={() => void createFolder(null)}
                  className="size-5 opacity-0 group-hover:opacity-100 focus-visible:opacity-100 [&_svg]:size-3.5"
                >
                  <FolderPlus />
                </IconButton>
                <IconButton
                  label="New page"
                  shortcut={SHORTCUTS["new-page"].keys}
                  onClick={() => void createPage(null)}
                  className="size-5 opacity-0 group-hover:opacity-100 focus-visible:opacity-100 [&_svg]:size-3.5"
                >
                  <Plus />
                </IconButton>
              </>
            )}
          </div>
          {isOpen ? (
            <PageTree />
          ) : (
            <p className="mt-1.5 px-2 text-[13px] leading-5 text-muted-foreground">
              {status === "loading" ? "Opening…" : "No workspace is open."}
            </p>
          )}
        </section>
      </div>

      <div className="px-2 pb-2">
        {isOpen && (
          <>
            <SidebarRow
              icon={Plus}
              label="New page"
              shortcut={SHORTCUTS["new-page"].keys}
              onClick={() => void createPage(null)}
            />
            <SidebarRow
              icon={FolderPlus}
              label="New folder"
              onClick={() => void createFolder(null)}
            />
            <SidebarRow icon={Trash2} label="Trash" onClick={() => toggleDialog("trash")} />
          </>
        )}
        <SidebarRow
          icon={Settings}
          label="Settings"
          shortcut={SHORTCUTS["open-settings"].keys}
          onClick={() => runCommand("open-settings")}
        />
      </div>

      <TrashDialog open={dialog === "trash"} onOpenChange={(open) => !open && closeDialog()} />
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
