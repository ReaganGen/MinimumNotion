import { ChevronRight, FileText } from "lucide-react";
import { useEffect, useState } from "react";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";
import { useWorkspaceStore } from "@/features/workspace/workspaceStore";
import type { PageSummary } from "@/lib/ipc/bindings/PageSummary";
import { ipc } from "@/lib/ipc/client";
import { cn } from "@/lib/utils";

const INDENT = 14;

/**
 * "Move to…": picks a new parent for a page. The page itself isn't listed,
 * so neither are its sub-pages, which can't hold it.
 */
export function MoveDialog({
  page,
  currentParentId,
  open,
  onOpenChange,
}: {
  page: PageSummary;
  currentParentId: string | null;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  async function moveTo(parentId: string | null) {
    onOpenChange(false);
    const { expand, reportError } = useWorkspaceStore.getState();
    try {
      await ipc.movePage(page.id, parentId, null);
      if (parentId) await expand(parentId);
    } catch (error) {
      reportError(error);
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-[420px] gap-0 p-0 sm:max-w-[420px]">
        <div className="border-b px-5 py-4">
          <DialogTitle className="text-[15px] font-semibold">Move “{page.title}”</DialogTitle>
          <DialogDescription className="mt-1 text-[13px] text-muted-foreground">
            Choose where it goes. Pages inside it move along.
          </DialogDescription>
        </div>
        <div className="max-h-[50vh] overflow-y-auto p-2">
          <Destination
            label="Top level"
            depth={0}
            current={currentParentId === null}
            onChoose={() => void moveTo(null)}
          />
          <Branch
            parentId={null}
            depth={0}
            exclude={page.id}
            currentParentId={currentParentId}
            onChoose={(id) => void moveTo(id)}
          />
        </div>
      </DialogContent>
    </Dialog>
  );
}

interface BranchProps {
  parentId: string | null;
  depth: number;
  exclude: string;
  currentParentId: string | null;
  onChoose: (id: string) => void;
}

function Branch({ parentId, depth, exclude, currentParentId, onChoose }: BranchProps) {
  const [pages, setPages] = useState<PageSummary[] | null>(null);

  useEffect(() => {
    let live = true;
    ipc
      .listChildren(parentId)
      .then((children) => live && setPages(children))
      .catch(() => live && setPages([]));
    return () => {
      live = false;
    };
  }, [parentId]);

  if (!pages) return null;
  return (
    <ul>
      {pages
        .filter((p) => p.id !== exclude)
        .map((p) => (
          <BranchRow
            key={p.id}
            page={p}
            depth={depth}
            exclude={exclude}
            currentParentId={currentParentId}
            onChoose={onChoose}
          />
        ))}
    </ul>
  );
}

function BranchRow({
  page,
  depth,
  exclude,
  currentParentId,
  onChoose,
}: Omit<BranchProps, "parentId"> & { page: PageSummary }) {
  const [expanded, setExpanded] = useState(false);
  return (
    <li>
      <Destination
        label={page.title}
        icon={page.icon}
        depth={depth + 1}
        current={currentParentId === page.id}
        onChoose={() => onChoose(page.id)}
        expander={
          page.has_children ? (
            <button
              type="button"
              aria-label={expanded ? `Collapse ${page.title}` : `Expand ${page.title}`}
              aria-expanded={expanded}
              onClick={() => setExpanded((open) => !open)}
              className="inline-flex size-5 shrink-0 items-center justify-center rounded text-faint-foreground hover:bg-accent hover:text-foreground"
            >
              <ChevronRight
                className={cn(
                  "size-3.5 transition-transform duration-(--duration-fast)",
                  expanded && "rotate-90",
                )}
              />
            </button>
          ) : (
            <span className="size-5 shrink-0" />
          )
        }
      />
      {expanded && (
        <Branch
          parentId={page.id}
          depth={depth + 1}
          exclude={exclude}
          currentParentId={currentParentId}
          onChoose={onChoose}
        />
      )}
    </li>
  );
}

function Destination({
  label,
  icon,
  depth,
  current,
  onChoose,
  expander,
}: {
  label: string;
  icon?: string | null;
  depth: number;
  current: boolean;
  onChoose: () => void;
  expander?: React.ReactNode;
}) {
  return (
    <div
      className="flex h-7 items-center gap-1 rounded-md pr-2 hover:bg-accent"
      style={{ paddingLeft: 4 + Math.max(0, depth - 1) * INDENT }}
    >
      {expander}
      <button
        type="button"
        disabled={current}
        onClick={onChoose}
        className="flex min-w-0 flex-1 items-center gap-1.5 text-left text-[13.5px] disabled:text-muted-foreground"
      >
        <span className="inline-flex size-5 shrink-0 items-center justify-center text-muted-foreground">
          {icon ? (
            <span className="text-[15px] leading-none">{icon}</span>
          ) : (
            <FileText className="size-4" aria-hidden />
          )}
        </span>
        <span className="truncate">{label}</span>
        {current && <span className="ml-auto shrink-0 text-[12px]">Current location</span>}
      </button>
    </div>
  );
}
