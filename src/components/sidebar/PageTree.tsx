import {
  ChevronRight,
  Copy,
  FileText,
  FolderInput,
  MoreHorizontal,
  Pencil,
  Plus,
  Star,
  Trash2,
} from "lucide-react";
import { type DragEvent, type KeyboardEvent, useState } from "react";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from "@/components/ui/context-menu";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { type DropZone, dropZone, placeDrop } from "@/features/workspace/dropPlacement";
import { flushAllEdits, ROOT, useWorkspaceStore } from "@/features/workspace/workspaceStore";
import type { PageSummary } from "@/lib/ipc/bindings/PageSummary";
import { ipc } from "@/lib/ipc/client";
import { cn } from "@/lib/utils";
import { MoveDialog } from "./MoveDialog";

const DRAG_TYPE = "application/x-forma-page";
const INDENT = 14;

export function PageTree({
  parentId = null,
  depth = 0,
}: {
  parentId?: string | null;
  depth?: number;
}) {
  const pages = useWorkspaceStore((s) => s.children[parentId ?? ROOT]);
  if (!pages) return null;
  if (pages.length === 0) {
    return depth === 0 ? (
      <p className="px-2 py-1 text-[13px] text-muted-foreground">No pages yet.</p>
    ) : (
      <p
        className="py-1 text-[12.5px] text-faint-foreground"
        style={{ paddingLeft: 30 + depth * INDENT }}
      >
        No pages inside
      </p>
    );
  }
  return (
    <ul aria-label={depth === 0 ? "Pages" : undefined}>
      {pages.map((page) => (
        <PageRow key={page.id} page={page} parentId={parentId} depth={depth} siblings={pages} />
      ))}
    </ul>
  );
}

function usePageActions(page: PageSummary) {
  const { createPage, reportError } = useWorkspaceStore.getState();
  const run = (action: () => Promise<unknown>) => () => {
    action().catch(reportError);
  };
  return {
    addChild: run(() => createPage(page.id)),
    duplicate: run(() => ipc.duplicatePage(page.id)),
    toggleFavorite: run(() => ipc.setFavorite(page.id, !page.favorite)),
    trash: run(async () => {
      await flushAllEdits();
      await ipc.trashPage(page.id);
    }),
  };
}

interface RowProps {
  page: PageSummary;
  parentId: string | null;
  depth: number;
  siblings: PageSummary[];
}

function PageRow({ page, parentId, depth, siblings }: RowProps) {
  const expanded = useWorkspaceStore((s) => Boolean(s.expanded[page.id]));
  const active = useWorkspaceStore((s) => s.activePageId === page.id);
  const { openPage, toggleExpanded, expand, reportError } = useWorkspaceStore.getState();
  const actions = usePageActions(page);
  const [renaming, setRenaming] = useState(false);
  const [moving, setMoving] = useState(false);
  const [zone, setZone] = useState<DropZone | null>(null);

  function onDragStart(event: DragEvent) {
    event.dataTransfer.setData(DRAG_TYPE, page.id);
    event.dataTransfer.effectAllowed = "move";
  }

  function onDragOver(event: DragEvent<HTMLDivElement>) {
    if (!event.dataTransfer.types.includes(DRAG_TYPE)) return;
    event.preventDefault();
    const rect = event.currentTarget.getBoundingClientRect();
    setZone(dropZone(event.clientY - rect.top, rect.height));
  }

  function onDrop(event: DragEvent) {
    const draggedId = event.dataTransfer.getData(DRAG_TYPE);
    const droppedZone = zone;
    setZone(null);
    if (!draggedId || !droppedZone) return;
    event.preventDefault();
    const placement = placeDrop(
      draggedId,
      { id: page.id, parentId },
      droppedZone,
      siblings.map((s) => s.id),
    );
    if (!placement) return;
    ipc
      .movePage(draggedId, placement.parentId, placement.position)
      .then(() => (placement.parentId === page.id ? expand(page.id) : undefined))
      .catch(reportError);
  }

  const items = (
    Item: typeof DropdownMenuItem | typeof ContextMenuItem,
    Separator: typeof DropdownMenuSeparator,
  ) => (
    <>
      <Item onSelect={actions.addChild}>
        <Plus aria-hidden /> Add a page inside
      </Item>
      <Item onSelect={() => setRenaming(true)}>
        <Pencil aria-hidden /> Rename
      </Item>
      <Item onSelect={actions.duplicate}>
        <Copy aria-hidden /> Duplicate
      </Item>
      <Item onSelect={() => setMoving(true)}>
        <FolderInput aria-hidden /> Move to…
      </Item>
      <Item onSelect={actions.toggleFavorite}>
        <Star aria-hidden /> {page.favorite ? "Remove from Favorites" : "Add to Favorites"}
      </Item>
      <Separator />
      <Item onSelect={actions.trash}>
        <Trash2 aria-hidden /> Move to Trash
      </Item>
    </>
  );

  return (
    <li>
      <ContextMenu>
        <ContextMenuTrigger asChild>
          {/* biome-ignore lint/a11y/noStaticElementInteractions: the row is only a drop target; its buttons carry keyboard access */}
          <div
            draggable={!renaming}
            onDragStart={onDragStart}
            onDragOver={onDragOver}
            onDragLeave={() => setZone(null)}
            onDrop={onDrop}
            className={cn(
              "group relative flex h-7 items-center gap-1 rounded-md pr-1 text-[13.5px]",
              active ? "bg-selected text-foreground" : "hover:bg-accent",
              zone === "inside" && "bg-selected ring-2 ring-ring ring-inset",
            )}
            style={{ paddingLeft: 4 + depth * INDENT }}
          >
            {zone === "before" && (
              <span className="absolute inset-x-1 top-0 h-0.5 rounded bg-ring" />
            )}
            {zone === "after" && (
              <span className="absolute inset-x-1 bottom-0 h-0.5 rounded bg-ring" />
            )}
            <button
              type="button"
              tabIndex={-1}
              aria-label={expanded ? `Collapse ${page.title}` : `Expand ${page.title}`}
              aria-expanded={page.has_children ? expanded : undefined}
              onClick={() => toggleExpanded(page.id)}
              className={cn(
                "inline-flex size-5 shrink-0 items-center justify-center rounded text-faint-foreground hover:bg-accent hover:text-foreground",
                !page.has_children && "invisible",
              )}
            >
              <ChevronRight
                className={cn(
                  "size-3.5 transition-transform duration-(--duration-fast)",
                  expanded && "rotate-90",
                )}
              />
            </button>
            <span className="inline-flex size-5 shrink-0 items-center justify-center text-muted-foreground">
              {page.icon ? (
                <span className="text-[15px] leading-none">{page.icon}</span>
              ) : (
                <FileText className="size-4" aria-hidden />
              )}
            </span>
            {renaming ? (
              <RenameInput page={page} onDone={() => setRenaming(false)} />
            ) : (
              <button
                type="button"
                onClick={() => openPage(page.id)}
                onDoubleClick={() => setRenaming(true)}
                aria-current={active ? "page" : undefined}
                className={cn(
                  "min-w-0 flex-1 truncate text-left outline-none focus-visible:underline",
                  page.status.state === "invalid_frontmatter" && "text-destructive",
                )}
                title={
                  page.status.state === "invalid_frontmatter" ? page.status.message : undefined
                }
              >
                {page.title}
              </button>
            )}
            {!renaming && (
              <span className="flex shrink-0 opacity-0 group-focus-within:opacity-100 group-hover:opacity-100">
                <DropdownMenu>
                  <DropdownMenuTrigger asChild>
                    <button
                      type="button"
                      aria-label={`Actions for ${page.title}`}
                      className="inline-flex size-5 items-center justify-center rounded text-muted-foreground hover:bg-accent hover:text-foreground"
                    >
                      <MoreHorizontal className="size-3.5" />
                    </button>
                  </DropdownMenuTrigger>
                  <DropdownMenuContent align="start" className="w-52">
                    {items(DropdownMenuItem, DropdownMenuSeparator)}
                  </DropdownMenuContent>
                </DropdownMenu>
                <button
                  type="button"
                  aria-label={`Add a page inside ${page.title}`}
                  onClick={actions.addChild}
                  className="inline-flex size-5 items-center justify-center rounded text-muted-foreground hover:bg-accent hover:text-foreground"
                >
                  <Plus className="size-3.5" />
                </button>
              </span>
            )}
          </div>
        </ContextMenuTrigger>
        <ContextMenuContent className="w-52">
          {items(ContextMenuItem, ContextMenuSeparator)}
        </ContextMenuContent>
      </ContextMenu>
      {expanded && <PageTree parentId={page.id} depth={depth + 1} />}
      <MoveDialog page={page} currentParentId={parentId} open={moving} onOpenChange={setMoving} />
    </li>
  );
}

function RenameInput({ page, onDone }: { page: PageSummary; onDone: () => void }) {
  const [value, setValue] = useState(page.title);
  const reportError = useWorkspaceStore((s) => s.reportError);

  function commit() {
    onDone();
    const title = value.trim();
    if (title && title !== page.title) ipc.setTitle(page.id, title).catch(reportError);
  }

  function onKeyDown(event: KeyboardEvent<HTMLInputElement>) {
    if (event.key === "Enter") commit();
    if (event.key === "Escape") onDone();
  }

  return (
    <input
      // biome-ignore lint/a11y/noAutofocus: renaming starts from an explicit user action
      autoFocus
      aria-label="Page title"
      value={value}
      onFocus={(e) => e.currentTarget.select()}
      onChange={(e) => setValue(e.target.value)}
      onKeyDown={onKeyDown}
      onBlur={commit}
      className="h-6 min-w-0 flex-1 rounded-sm bg-background px-1 text-[13.5px] outline-none ring-1 ring-ring"
    />
  );
}
