import { RotateCcw, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";
import { useWorkspaceStore } from "@/features/workspace/workspaceStore";
import type { TrashEntryInfo } from "@/lib/ipc/bindings/TrashEntryInfo";
import { ipc } from "@/lib/ipc/client";

interface TrashDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

/** Trashed pages, with their sub-pages, can be restored or deleted for good. */
export function TrashDialog({ open, onOpenChange }: TrashDialogProps) {
  const [entries, setEntries] = useState<TrashEntryInfo[] | null>(null);
  const [confirming, setConfirming] = useState<string | null>(null);
  const { reportError, openPage } = useWorkspaceStore.getState();

  useEffect(() => {
    if (!open) return;
    setConfirming(null);
    ipc.listTrash().then(setEntries).catch(reportError);
  }, [open, reportError]);

  async function restore(entry: TrashEntryInfo) {
    try {
      const id = await ipc.restoreTrash(entry.entry_id);
      setEntries((list) => list?.filter((e) => e.entry_id !== entry.entry_id) ?? null);
      if (id) openPage(id);
    } catch (error) {
      reportError(error);
    }
  }

  async function deleteForever(entry: TrashEntryInfo) {
    try {
      await ipc.deleteTrashEntry(entry.entry_id);
      setEntries((list) => list?.filter((e) => e.entry_id !== entry.entry_id) ?? null);
      setConfirming(null);
    } catch (error) {
      reportError(error);
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-h-[70vh] max-w-[520px] gap-0 overflow-hidden p-0 sm:max-w-[520px]">
        <div className="border-b px-5 py-4">
          <DialogTitle className="text-[15px] font-semibold">Trash</DialogTitle>
          <DialogDescription className="mt-1 text-[13px] text-muted-foreground">
            Pages stay here, with their sub-pages, until you delete them.
          </DialogDescription>
        </div>
        <ul className="max-h-[50vh] overflow-y-auto p-2">
          {entries?.length === 0 && (
            <li className="px-3 py-8 text-center text-[13px] text-muted-foreground">
              The trash is empty.
            </li>
          )}
          {entries?.map((entry) => (
            <li
              key={entry.entry_id}
              className="group flex items-center gap-3 rounded-md px-3 py-2 hover:bg-accent"
            >
              <div className="min-w-0 flex-1">
                <p className="truncate text-[13.5px]">{entry.title}</p>
                <p className="text-[12px] text-faint-foreground">
                  {new Date(entry.trashed_at).toLocaleString()}
                  {entry.page_count > 1 && ` · ${entry.page_count} pages`}
                </p>
              </div>
              {confirming === entry.entry_id ? (
                <>
                  <span className="text-[12.5px] text-muted-foreground">Delete for good?</span>
                  <button
                    type="button"
                    onClick={() => deleteForever(entry)}
                    className="rounded-md bg-destructive-fill px-2 py-1 text-[12.5px] text-primary-foreground"
                  >
                    Delete
                  </button>
                  <button
                    type="button"
                    onClick={() => setConfirming(null)}
                    className="rounded-md px-2 py-1 text-[12.5px] hover:bg-accent"
                  >
                    Cancel
                  </button>
                </>
              ) : (
                <>
                  <button
                    type="button"
                    onClick={() => restore(entry)}
                    className="inline-flex items-center gap-1 rounded-md px-2 py-1 text-[12.5px] text-muted-foreground hover:bg-selected hover:text-foreground"
                  >
                    <RotateCcw className="size-3.5" aria-hidden /> Restore
                  </button>
                  <button
                    type="button"
                    aria-label={`Delete ${entry.title} for good`}
                    onClick={() => setConfirming(entry.entry_id)}
                    className="inline-flex size-7 items-center justify-center rounded-md text-muted-foreground hover:bg-selected hover:text-destructive"
                  >
                    <Trash2 className="size-3.5" aria-hidden />
                  </button>
                </>
              )}
            </li>
          ))}
        </ul>
      </DialogContent>
    </Dialog>
  );
}
