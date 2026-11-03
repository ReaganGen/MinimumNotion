import { Fragment, useEffect, useState } from "react";
import { useWorkspaceStore } from "@/features/workspace/workspaceStore";
import type { CellInput } from "@/lib/ipc/bindings/CellInput";
import type { EntryData } from "@/lib/ipc/bindings/EntryData";
import { ipc } from "@/lib/ipc/client";
import { PropertyCell, TYPE_ICONS } from "./cells";

/** A collection entry's values, under its title (DATA_MODEL §6.3). */
export function EntryProperties({ pageId, editable }: { pageId: string; editable: boolean }) {
  const changes = useWorkspaceStore((s) => s.changeCount);
  const openPage = useWorkspaceStore((s) => s.openPage);
  const [entry, setEntry] = useState<EntryData | null>(null);
  const [reloads, setReloads] = useState(0);

  // biome-ignore lint/correctness/useExhaustiveDependencies: reloads on changes and after edits
  useEffect(() => {
    let live = true;
    ipc
      .getEntry(pageId)
      .then((found) => live && setEntry(found))
      .catch(() => live && setEntry(null));
    return () => {
      live = false;
    };
  }, [pageId, changes, reloads]);

  if (!entry) return null;
  const canEdit = editable && !entry.problem && entry.row.editable;

  async function run(edit: () => Promise<unknown>) {
    try {
      await edit();
      return true;
    } catch (error) {
      useWorkspaceStore.getState().reportError(error);
      return false;
    } finally {
      setReloads((n) => n + 1);
    }
  }

  return (
    <section aria-label="Properties" className="mb-6 text-[13.5px]">
      <p className="mb-1.5 text-[12.5px] text-faint-foreground">
        In{" "}
        <button
          type="button"
          onClick={() => openPage(entry.collection_id)}
          className="text-muted-foreground underline decoration-faint-foreground underline-offset-2 hover:text-foreground"
        >
          {entry.collection_title}
        </button>
      </p>
      {entry.problem && (
        <p className="mb-1.5 text-[12.5px] text-muted-foreground">{entry.problem}</p>
      )}
      <dl className="grid grid-cols-[minmax(7rem,11rem)_1fr] gap-x-2">
        {(entry.schema?.properties ?? []).map((property) => {
          const Icon = TYPE_ICONS[property.type];
          return (
            <Fragment key={property.id}>
              <dt className="flex h-8 min-w-0 items-center gap-1.5 text-muted-foreground">
                <Icon className="size-3.5 shrink-0" aria-hidden />
                <span className="truncate">{property.name}</span>
              </dt>
              <dd className="flex h-8 min-w-0 items-center rounded-md px-1.5 hover:bg-accent">
                <PropertyCell
                  property={property}
                  cell={entry.row.cells[property.id]}
                  editable={canEdit}
                  label={entry.row.title}
                  onChange={(value: CellInput) =>
                    void run(() => ipc.setProperty(pageId, property.id, value))
                  }
                  onCreateOption={(name) =>
                    run(() =>
                      ipc.updateSchema(entry.collection_id, {
                        op: "add_option",
                        property_id: property.id,
                        name,
                      }),
                    )
                  }
                />
              </dd>
            </Fragment>
          );
        })}
      </dl>
    </section>
  );
}
