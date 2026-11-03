import { FileText } from "lucide-react";
import { useEffect, useState } from "react";
import { useWorkspaceStore } from "@/features/workspace/workspaceStore";
import type { PageHit } from "@/lib/ipc/bindings/PageHit";
import { ipc } from "@/lib/ipc/client";

/** Pages that link here, by page link or wikilink (ARCHITECTURE §6.5). */
export function Backlinks({ pageId }: { pageId: string }) {
  const changes = useWorkspaceStore((s) => s.changeCount);
  const openPage = useWorkspaceStore((s) => s.openPage);
  const [pages, setPages] = useState<PageHit[]>([]);

  // biome-ignore lint/correctness/useExhaustiveDependencies: refetch after every workspace change
  useEffect(() => {
    let live = true;
    ipc
      .backlinks(pageId)
      .then((found) => live && setPages(found))
      .catch(() => live && setPages([]));
    return () => {
      live = false;
    };
  }, [pageId, changes]);

  if (pages.length === 0) return null;
  return (
    <section aria-label="Backlinks" className="mt-16 border-t pt-4">
      <h2 className="mb-1.5 text-[12.5px] font-medium text-muted-foreground">
        {pages.length === 1 ? "1 page links here" : `${pages.length} pages link here`}
      </h2>
      <ul>
        {pages.map((page) => (
          <li key={page.id}>
            <button
              type="button"
              onClick={() => openPage(page.id)}
              className="flex w-full items-center gap-2 rounded-md px-1.5 py-1 text-left text-[13.5px] hover:bg-accent"
            >
              <span className="inline-flex size-5 shrink-0 items-center justify-center text-muted-foreground">
                {page.icon ?? <FileText className="size-4" aria-hidden />}
              </span>
              <span className="truncate text-foreground">{page.title}</span>
              {page.path.length > 0 && (
                <span className="truncate text-[12px] text-faint-foreground">
                  {page.path.join(" / ")}
                </span>
              )}
            </button>
          </li>
        ))}
      </ul>
    </section>
  );
}
