import { FileText, Search } from "lucide-react";
import { type KeyboardEvent, useEffect, useRef, useState } from "react";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";
import { markTitle, type Run, splitMarks } from "@/features/search/marks";
import { useWorkspaceStore } from "@/features/workspace/workspaceStore";
import type { PageHit } from "@/lib/ipc/bindings/PageHit";
import { ipc } from "@/lib/ipc/client";
import { cn } from "@/lib/utils";

const DEBOUNCE_MS = 120;

export type PaletteMode = "search" | "quick-open";

const COPY: Record<PaletteMode, { title: string; placeholder: string }> = {
  search: { title: "Search", placeholder: "Search titles, text and properties…" },
  "quick-open": { title: "Quick open", placeholder: "Jump to a page…" },
};

interface PaletteDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  mode: PaletteMode;
}

/** ⌘K search and ⌘P quick open: type, move with ↑↓, open with Return. */
export function PaletteDialog({ open, onOpenChange, mode }: PaletteDialogProps) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        showCloseButton={false}
        className="top-[14vh] max-w-[600px] translate-y-0 gap-0 overflow-hidden p-0 sm:max-w-[600px]"
      >
        <DialogTitle className="sr-only">{COPY[mode].title}</DialogTitle>
        {/* Remounted on every open, so it starts empty. */}
        {open && <Palette mode={mode} onDone={() => onOpenChange(false)} />}
      </DialogContent>
    </Dialog>
  );
}

function Palette({ mode, onDone }: { mode: PaletteMode; onDone: () => void }) {
  const hasWorkspace = useWorkspaceStore((s) => s.status === "open");
  const recent = useWorkspaceStore((s) => s.recent);
  const [query, setQuery] = useState("");
  const [hits, setHits] = useState<PageHit[] | null>(null);
  const [selected, setSelected] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const request = useRef(0);
  const listRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const ticket = ++request.current;
    if (!query.trim() || !hasWorkspace) {
      setHits(null);
      return;
    }
    const timer = setTimeout(() => {
      const find = mode === "search" ? ipc.search : ipc.quickOpen;
      find(query)
        .then((found) => {
          // A slower, older request must not replace newer results.
          if (ticket !== request.current) return;
          setHits(found);
          setSelected(0);
          setError(null);
        })
        .catch((e: unknown) => {
          if (ticket === request.current) setError(e instanceof Error ? e.message : String(e));
        });
    }, DEBOUNCE_MS);
    return () => clearTimeout(timer);
  }, [query, mode, hasWorkspace]);

  const showingRecent = mode === "quick-open" && !query.trim();
  const items: PageHit[] = showingRecent
    ? recent.map((r) => ({
        id: r.id,
        title: r.title,
        icon: r.icon,
        path: [],
        snippet: null,
        kind: "leaf" as const,
      }))
    : (hits ?? []);

  function choose(index: number) {
    const hit = items[index];
    if (!hit) return;
    useWorkspaceStore.getState().openPage(hit.id);
    onDone();
  }

  function onKeyDown(event: KeyboardEvent<HTMLInputElement>) {
    if (items.length === 0) return;
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      const step = event.key === "ArrowDown" ? 1 : -1;
      const next = (selected + step + items.length) % items.length;
      setSelected(next);
      listRef.current
        ?.querySelector(`[data-index="${next}"]`)
        ?.scrollIntoView?.({ block: "nearest" });
    } else if (event.key === "Enter" && !event.nativeEvent.isComposing) {
      event.preventDefault();
      choose(selected);
    }
  }

  let message: string | null = null;
  if (!hasWorkspace) message = "Open a workspace to search it.";
  else if (error) message = error;
  else if (showingRecent && items.length === 0) message = "Pages you open show up here.";
  else if (!showingRecent && hits && hits.length === 0) message = "No pages match.";
  else if (mode === "search" && !query.trim())
    message = "Search page titles, text, tags and properties.";

  const listId = `palette-${mode}-results`;
  return (
    <>
      <div className="flex h-12 items-center gap-3 border-b px-4">
        <Search className="size-4 shrink-0 text-faint-foreground" aria-hidden />
        <input
          // biome-ignore lint/a11y/noAutofocus: the palette exists to be typed into
          autoFocus
          role="combobox"
          aria-expanded={items.length > 0}
          aria-controls={listId}
          aria-activedescendant={items.length > 0 ? `${listId}-${selected}` : undefined}
          aria-label={COPY[mode].title}
          placeholder={COPY[mode].placeholder}
          disabled={!hasWorkspace}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={onKeyDown}
          className="min-w-0 flex-1 bg-transparent text-[15px] outline-none placeholder:text-faint-foreground"
        />
      </div>
      {message ? (
        <DialogDescription className="px-4 py-10 text-center text-[13px] text-muted-foreground">
          {message}
        </DialogDescription>
      ) : (
        <div ref={listRef} className="max-h-[52vh] overflow-y-auto p-1.5">
          {showingRecent && (
            <p className="px-2.5 pt-1 pb-1.5 text-[12px] text-faint-foreground">Recent</p>
          )}
          <div role="listbox" id={listId} aria-label="Results">
            {items.map((hit, index) => (
              // The keyboard drives the list from the input (aria-activedescendant).
              // biome-ignore lint/a11y/useKeyWithClickEvents: arrow keys and Return in the input do this
              <div
                key={hit.id}
                id={`${listId}-${index}`}
                data-index={index}
                role="option"
                aria-selected={index === selected}
                tabIndex={-1}
                onMouseMove={() => setSelected(index)}
                onClick={() => choose(index)}
                className={cn(
                  "flex cursor-default gap-2.5 rounded-md px-2.5 py-2",
                  index === selected && "bg-accent",
                )}
              >
                <span className="mt-0.5 inline-flex size-5 shrink-0 items-center justify-center text-muted-foreground">
                  {hit.icon ? (
                    <span className="text-[15px] leading-none">{hit.icon}</span>
                  ) : (
                    <FileText className="size-4" aria-hidden />
                  )}
                </span>
                <span className="min-w-0 flex-1">
                  <span className="block truncate text-[14px] text-foreground">
                    <Runs runs={markTitle(hit.title, query)} />
                  </span>
                  {hit.path.length > 0 && (
                    <span className="block truncate text-[12px] text-faint-foreground">
                      {hit.path.join(" / ")}
                    </span>
                  )}
                  {hit.snippet && (
                    <span className="mt-0.5 line-clamp-2 block text-[12.5px] text-muted-foreground">
                      <Runs runs={splitMarks(hit.snippet)} />
                    </span>
                  )}
                </span>
              </div>
            ))}
          </div>
        </div>
      )}
    </>
  );
}

function Runs({ runs }: { runs: Run[] }) {
  return runs.map((run, i) =>
    run.match ? (
      // biome-ignore lint/suspicious/noArrayIndexKey: runs are positional and never reorder
      <mark key={i} className="rounded-sm bg-highlight px-px text-foreground">
        {run.text}
      </mark>
    ) : (
      // biome-ignore lint/suspicious/noArrayIndexKey: runs are positional and never reorder
      <span key={i}>{run.text}</span>
    ),
  );
}
