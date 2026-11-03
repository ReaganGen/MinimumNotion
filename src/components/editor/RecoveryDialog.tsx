import { useEffect, useState } from "react";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";
import { useWorkspaceStore } from "@/features/workspace/workspaceStore";
import type { RecoveryBuffer } from "@/lib/ipc/bindings/RecoveryBuffer";
import { ipc } from "@/lib/ipc/client";

/**
 * Text that couldn't be saved last time (a conflict, a deleted file, a failed
 * save) is offered back after opening the workspace (ARCHITECTURE §4.10).
 */
export function RecoveryDialog() {
  const status = useWorkspaceStore((s) => s.status);
  const root = useWorkspaceStore((s) => s.info?.root);
  const [buffers, setBuffers] = useState<RecoveryBuffer[]>([]);
  const { reportError, openPage, createPage } = useWorkspaceStore.getState();

  useEffect(() => {
    if (status !== "open" || !root) return;
    ipc.listRecovery().then(setBuffers).catch(reportError);
  }, [status, root, reportError]);

  async function keep(buffer: RecoveryBuffer) {
    try {
      const id = await createPage(null);
      if (!id) return;
      await ipc.setTitle(id, `${buffer.title || "Untitled"} (recovered)`);
      const fresh = await ipc.getPage(id);
      await ipc.savePage(id, fresh.body_rev, buffer.body);
      await ipc.discardRecovery(buffer.page_id);
      setBuffers((list) => list.filter((b) => b.page_id !== buffer.page_id));
      openPage(null);
      openPage(id);
    } catch (error) {
      reportError(error);
    }
  }

  async function discard(buffer: RecoveryBuffer) {
    try {
      await ipc.discardRecovery(buffer.page_id);
      setBuffers((list) => list.filter((b) => b.page_id !== buffer.page_id));
    } catch (error) {
      reportError(error);
    }
  }

  return (
    <Dialog open={buffers.length > 0} onOpenChange={(open) => !open && setBuffers([])}>
      <DialogContent className="max-w-[520px] gap-0 p-0 sm:max-w-[520px]">
        <div className="border-b px-5 py-4">
          <DialogTitle className="text-[15px] font-semibold">
            Unsaved text from last time
          </DialogTitle>
          <DialogDescription className="mt-1 text-[13px] text-muted-foreground">
            Forma kept this text because it couldn't be saved normally. Keep it as a new page, or
            discard it. Closing this keeps it for next time.
          </DialogDescription>
        </div>
        <ul className="max-h-[50vh] overflow-y-auto p-2">
          {buffers.map((buffer) => (
            <li key={buffer.page_id} className="rounded-md px-3 py-2">
              <p className="truncate text-[13.5px] text-foreground">{buffer.title || "Untitled"}</p>
              <p className="selectable mt-0.5 line-clamp-2 text-[12.5px] text-muted-foreground">
                {buffer.body}
              </p>
              <div className="mt-2 flex gap-2">
                <button
                  type="button"
                  onClick={() => keep(buffer)}
                  className="rounded-md bg-primary px-2.5 py-1 text-[12.5px] text-primary-foreground hover:bg-primary/90"
                >
                  Keep as new page
                </button>
                <button
                  type="button"
                  onClick={() => discard(buffer)}
                  className="rounded-md px-2.5 py-1 text-[12.5px] hover:bg-accent"
                >
                  Discard
                </button>
              </div>
            </li>
          ))}
        </ul>
      </DialogContent>
    </Dialog>
  );
}
