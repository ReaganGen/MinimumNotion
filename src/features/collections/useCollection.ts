// A collection's data and view settings for one view on screen. Data reloads
// after every workspace change and after every edit made here; view settings
// are saved shortly after they change (they're advisory, DATA_MODEL §6.5).
import { useCallback, useEffect, useRef, useState } from "react";
import { useWorkspaceStore } from "@/features/workspace/workspaceStore";
import type { CollectionData } from "@/lib/ipc/bindings/CollectionData";
import type { CollectionViews } from "@/lib/ipc/bindings/CollectionViews";
import { ipc } from "@/lib/ipc/client";

const SAVE_VIEWS_DELAY_MS = 400;

export function useCollection(id: string) {
  const changes = useWorkspaceStore((s) => s.changeCount);
  const [data, setData] = useState<CollectionData | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [views, setViewsState] = useState<CollectionViews | null>(null);
  const [reloads, setReloads] = useState(0);
  const saveTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const pending = useRef<CollectionViews | null>(null);

  // biome-ignore lint/correctness/useExhaustiveDependencies: reloads on changes and on request
  useEffect(() => {
    let live = true;
    ipc
      .getCollection(id)
      .then((found) => {
        if (!live) return;
        setData(found);
        setError(null);
      })
      .catch((e: unknown) => live && setError(e instanceof Error ? e.message : String(e)));
    return () => {
      live = false;
    };
  }, [id, changes, reloads]);

  useEffect(() => {
    let live = true;
    ipc
      .getViews(id)
      .then((found) => live && setViewsState(found))
      .catch(
        () =>
          live &&
          setViewsState({
            active_view: "table",
            table: { sort: [], filter: [], hidden: [], column_widths: {} },
            list: { sort: [], filter: [], hidden: [], column_widths: {} },
          }),
      );
    return () => {
      live = false;
    };
  }, [id]);

  const flushViews = useCallback(() => {
    clearTimeout(saveTimer.current);
    const next = pending.current;
    pending.current = null;
    if (next) ipc.saveViews(id, next).catch(useWorkspaceStore.getState().reportError);
  }, [id]);

  // Unsaved view changes are written when the view goes away.
  useEffect(() => flushViews, [flushViews]);

  const setViews = useCallback(
    (next: CollectionViews) => {
      setViewsState(next);
      pending.current = next;
      clearTimeout(saveTimer.current);
      saveTimer.current = setTimeout(flushViews, SAVE_VIEWS_DELAY_MS);
    },
    [flushViews],
  );

  const reload = useCallback(() => setReloads((n) => n + 1), []);

  /** Runs an edit, reports a failure, and reloads either way. */
  const run = useCallback(
    async (edit: () => Promise<unknown>) => {
      try {
        await edit();
        return true;
      } catch (e) {
        useWorkspaceStore.getState().reportError(e);
        return false;
      } finally {
        reload();
      }
    },
    [reload],
  );

  return { data, error, views, setViews, reload, run };
}
