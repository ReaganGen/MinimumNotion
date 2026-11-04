// What opening a folder shows (ADR-047): its contents, never an editor. A
// folder has no text, and opening one writes nothing.
import { FilePlus2, FileText, Folder, FolderPlus, Plus, Table2 } from "lucide-react";
import { useEffect } from "react";
import { turnFolderIntoPage } from "@/components/sidebar/PageTree";
import { Button } from "@/components/ui/button";
import { useWorkspaceStore } from "@/features/workspace/workspaceStore";
import type { PageDocument } from "@/lib/ipc/bindings/PageDocument";

export function FolderView({ page, editable }: { page: PageDocument; editable: boolean }) {
  const children = useWorkspaceStore((s) => s.children[page.id]);
  const { loadChildren, openPage, createPage, createFolder, reportError } =
    useWorkspaceStore.getState();

  useEffect(() => {
    void loadChildren(page.id);
  }, [page.id, loadChildren]);

  return (
    <div className="min-h-0 flex-1 overflow-y-auto">
      <article className="mx-auto w-full max-w-(--reading-width) px-12 pt-14 pb-[30vh]">
        <div className="flex items-center gap-3 text-muted-foreground">
          <Folder className="size-6" aria-hidden />
          <span className="text-[13px]">Folder</span>
        </div>
        <h1 className="mt-2 text-[2rem] leading-tight font-semibold tracking-tight">
          {page.title}
        </h1>
        <p className="mt-2 text-[14px] text-muted-foreground">
          A folder groups pages. It has no text of its own.
        </p>

        {editable && (
          <div className="mt-6 flex flex-wrap gap-2">
            <Button variant="outline" size="sm" onClick={() => void createPage(page.id)}>
              <Plus aria-hidden /> New page here
            </Button>
            <Button variant="outline" size="sm" onClick={() => void createFolder(page.id)}>
              <FolderPlus aria-hidden /> New folder here
            </Button>
            <Button
              variant="ghost"
              size="sm"
              onClick={() => turnFolderIntoPage(page.id).catch(reportError)}
            >
              <FilePlus2 aria-hidden /> Turn into page
            </Button>
          </div>
        )}

        <h2 className="mt-10 mb-2 text-xs font-medium text-faint-foreground">Inside</h2>
        {children && children.length === 0 && (
          <p className="text-[14px] text-muted-foreground">Nothing here yet.</p>
        )}
        {children && children.length > 0 && (
          <ul aria-label={`Inside ${page.title}`}>
            {children.map((child) => (
              <li key={child.id}>
                <button
                  type="button"
                  onClick={() => openPage(child.id)}
                  className="flex h-8 w-full items-center gap-2 rounded-md px-2 text-left text-[14px] hover:bg-accent"
                >
                  <span className="inline-flex size-5 shrink-0 items-center justify-center text-muted-foreground">
                    {child.kind === "folder" ? (
                      <Folder className="size-4" aria-label="Folder" />
                    ) : child.icon ? (
                      <span className="text-[15px] leading-none">{child.icon}</span>
                    ) : child.is_collection ? (
                      <Table2 className="size-4" aria-label="Collection" />
                    ) : (
                      <FileText className="size-4" aria-hidden />
                    )}
                  </span>
                  <span className="min-w-0 flex-1 truncate">{child.title}</span>
                </button>
              </li>
            ))}
          </ul>
        )}
      </article>
    </div>
  );
}
