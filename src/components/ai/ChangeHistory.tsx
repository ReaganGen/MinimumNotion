// AI changes that were applied, newest first, from `.forma/operations/`.
// Each can be undone while its pages are as it left them; an undo can be
// redone the same way.
import { Redo2, Undo2 } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";
import { flushAllEdits, useWorkspaceStore } from "@/features/workspace/workspaceStore";
import type { ChangeConflict } from "@/lib/ipc/bindings/ChangeConflict";
import type { OperationInfo } from "@/lib/ipc/bindings/OperationInfo";
import { ipc } from "@/lib/ipc/client";

export function ChangeHistory({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const [operations, setOperations] = useState<OperationInfo[] | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [problems, setProblems] = useState<Record<string, ChangeConflict[] | string>>({});
  const { reportError, openPage } = useWorkspaceStore.getState();

  const load = useCallback(() => {
    ipc.listOperations().then(setOperations).catch(reportError);
  }, [reportError]);

  useEffect(() => {
    if (!open) return;
    setProblems({});
    load();
  }, [open, load]);

  async function undo(op: OperationInfo) {
    setBusy(op.op_id);
    try {
      await flushAllEdits();
      const outcome = await ipc.undoOperation(op.op_id);
      if (outcome.outcome === "conflicts") {
        setProblems((p) => ({ ...p, [op.op_id]: outcome.conflicts }));
      }
      load();
    } catch (error) {
      setProblems((p) => ({ ...p, [op.op_id]: (error as Error).message }));
    } finally {
      setBusy(null);
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-h-[75vh] max-w-[560px] gap-0 overflow-hidden p-0 sm:max-w-[560px]">
        <div className="border-b px-5 py-4">
          <DialogTitle className="text-[15px] font-semibold">AI changes</DialogTitle>
          <DialogDescription className="mt-1 text-[13px] text-muted-foreground">
            Changes you applied from the assistant. Undo one while its pages are as it left them.
          </DialogDescription>
        </div>
        <ul aria-label="Applied changes" className="max-h-[56vh] overflow-y-auto p-2">
          {operations?.length === 0 && (
            <li className="px-3 py-8 text-center text-[13px] text-muted-foreground">
              No AI changes have been applied in this workspace.
            </li>
          )}
          {operations?.map((op) => {
            const isUndo = op.source === "undo";
            const problem = problems[op.op_id];
            return (
              <li key={op.op_id} className="rounded-md px-3 py-2 hover:bg-accent">
                <div className="flex items-start gap-3">
                  <div className="min-w-0 flex-1">
                    <p className="text-[13.5px]">
                      {op.summary}
                      {op.undone && (
                        <span className="ml-1.5 text-[12px] text-faint-foreground">
                          {isUndo ? "Redone" : "Undone"}
                        </span>
                      )}
                    </p>
                    <p className="truncate text-[12px] text-faint-foreground">
                      {new Date(op.applied_at).toLocaleString()}
                      {op.request && ` · “${op.request}”`}
                    </p>
                    <p className="mt-0.5 flex flex-wrap gap-x-2 text-[12px]">
                      {op.pages
                        .filter((p) => p.title !== null)
                        .slice(0, 5)
                        .map((p) => (
                          <button
                            key={p.id}
                            type="button"
                            onClick={() => {
                              openPage(p.id);
                              onOpenChange(false);
                            }}
                            className="text-link hover:underline"
                          >
                            {p.title}
                          </button>
                        ))}
                    </p>
                  </div>
                  {!op.undone && (
                    <button
                      type="button"
                      disabled={busy !== null}
                      onClick={() => void undo(op)}
                      className="inline-flex shrink-0 items-center gap-1 rounded-md px-2 py-1 text-[12.5px] text-muted-foreground hover:bg-selected hover:text-foreground disabled:opacity-50"
                    >
                      {isUndo ? (
                        <>
                          <Redo2 className="size-3.5" aria-hidden /> Redo
                        </>
                      ) : (
                        <>
                          <Undo2 className="size-3.5" aria-hidden /> Undo
                        </>
                      )}
                    </button>
                  )}
                </div>
                {problem && (
                  <div role="alert" className="mt-1 text-[12px] text-muted-foreground">
                    {typeof problem === "string" ? (
                      <p className="text-destructive">{problem}</p>
                    ) : (
                      <>
                        <p className="text-destructive">Nothing was changed:</p>
                        <ul>
                          {problem.map((c) => (
                            <li key={`${c.change_id}-${c.message}`}>
                              “{c.title}”: {c.message}
                            </li>
                          ))}
                        </ul>
                      </>
                    )}
                  </div>
                )}
              </li>
            );
          })}
        </ul>
      </DialogContent>
    </Dialog>
  );
}
