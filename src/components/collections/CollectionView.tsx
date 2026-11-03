// A collection page's entries as a table or a list (DATA_MODEL §6). Rows are
// virtualized so a large collection stays fast; values are edited in place.
import { useVirtualizer } from "@tanstack/react-virtual";
import { FileText, MoreHorizontal, Plus } from "lucide-react";
import { type PointerEvent, useRef, useState } from "react";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { useCollection } from "@/features/collections/useCollection";
import { applyView, visibleProperties } from "@/features/collections/viewLogic";
import { useWorkspaceStore } from "@/features/workspace/workspaceStore";
import type { CellInput } from "@/lib/ipc/bindings/CellInput";
import type { CollectionData } from "@/lib/ipc/bindings/CollectionData";
import type { CollectionRow } from "@/lib/ipc/bindings/CollectionRow";
import type { SchemaOp } from "@/lib/ipc/bindings/SchemaOp";
import type { ViewKind } from "@/lib/ipc/bindings/ViewKind";
import type { ViewSettings } from "@/lib/ipc/bindings/ViewSettings";
import { ipc } from "@/lib/ipc/client";
import { CellDisplay, PropertyCell } from "./cells";
import { AddPropertyButton, PropertyHeader } from "./PropertyMenus";
import { FilterControl, PropertiesControl, SortControl } from "./ViewControls";

const ROW_HEIGHT = 34;
const TITLE_WIDTH = 280;
const COLUMN_WIDTH = 168;
const MIN_WIDTH = 80;
const MAX_WIDTH = 640;

/** The viewport assumed until the scroll area is measured. */
const INITIAL_RECT = { width: 1000, height: 600 };
/** Rows drawn when there's no layout to measure (a hidden window, tests). */
const UNMEASURED_ROWS = 50;

interface Slot {
  index: number;
  start: number;
  end: number;
}

/** The virtualizer's rows, or the first few when nothing could be measured. */
function slots(items: Slot[], count: number, size: number): Slot[] {
  if (items.length > 0 || count === 0) return items;
  return Array.from({ length: Math.min(count, UNMEASURED_ROWS) }, (_, index) => ({
    index,
    start: index * size,
    end: (index + 1) * size,
  }));
}

export function CollectionView({ id, editable }: { id: string; editable: boolean }) {
  const { data, error, views, setViews, run } = useCollection(id);

  if (error) {
    return <p className="mt-8 text-[13px] text-muted-foreground">{error}</p>;
  }
  if (!data || !views) return <div className="mt-8 h-40" />;

  const active = views.active_view;
  const view = views[active];
  const setView = (next: ViewSettings) => setViews({ ...views, [active]: next });
  const canEdit = editable && !data.problem && data.schema !== null;
  const rows = applyView(data.rows, data.schema, view);
  const apply = (op: SchemaOp) => run(() => ipc.updateSchema(id, op));

  return (
    <section aria-label="Collection" className="mt-10">
      {data.problem && (
        <p className="mb-3 rounded-md bg-muted px-3 py-2 text-[13px] text-muted-foreground">
          {data.problem}
        </p>
      )}
      <div className="mb-1 flex items-center gap-1 border-b pb-1.5">
        <ToggleGroup
          type="single"
          value={active}
          onValueChange={(value) => value && setViews({ ...views, active_view: value as ViewKind })}
          aria-label="View"
          className="mr-auto"
        >
          <ToggleGroupItem value="table" className="h-7 px-2.5 text-[13px]">
            Table
          </ToggleGroupItem>
          <ToggleGroupItem value="list" className="h-7 px-2.5 text-[13px]">
            List
          </ToggleGroupItem>
        </ToggleGroup>
        <PropertiesControl schema={data.schema} view={view} onChange={setView} />
        <SortControl schema={data.schema} view={view} onChange={setView} />
        <FilterControl schema={data.schema} view={view} onChange={setView} />
        {canEdit && (
          <button
            type="button"
            onClick={() => void run(() => ipc.createPage(id, "Untitled"))}
            className="ml-1 inline-flex h-7 items-center gap-1 rounded-md bg-primary px-2.5 text-[13px] text-primary-foreground hover:bg-primary/90"
          >
            <Plus className="size-3.5" aria-hidden /> New
          </button>
        )}
        {editable && <CollectionMenu id={id} />}
      </div>

      {data.rows.length === 0 ? (
        <p className="py-8 text-center text-[13px] text-muted-foreground">
          No entries yet. Each entry is a page inside this one.
        </p>
      ) : rows.length === 0 ? (
        <p className="py-8 text-center text-[13px] text-muted-foreground">
          No entries match the filters.
        </p>
      ) : active === "table" ? (
        <TableView
          data={data}
          rows={rows}
          view={view}
          onViewChange={setView}
          canEdit={canEdit}
          apply={apply}
          setValue={(row, propertyId, value) =>
            void run(() => ipc.setProperty(row.id, propertyId, value))
          }
        />
      ) : (
        <ListView data={data} rows={rows} view={view} />
      )}
      {data.rows.length > 0 && (
        <p className="mt-1.5 text-[12px] text-faint-foreground">
          {rows.length === data.rows.length
            ? `${data.rows.length} ${data.rows.length === 1 ? "entry" : "entries"}`
            : `${rows.length} of ${data.rows.length} entries`}
        </p>
      )}
    </section>
  );
}

function CollectionMenu({ id }: { id: string }) {
  const [confirming, setConfirming] = useState(false);
  const reportError = useWorkspaceStore((s) => s.reportError);
  return (
    <>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <button
            type="button"
            aria-label="Collection actions"
            className="inline-flex size-7 items-center justify-center rounded-md text-muted-foreground hover:bg-accent"
          >
            <MoreHorizontal className="size-4" aria-hidden />
          </button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" className="w-56">
          <DropdownMenuItem onSelect={() => setConfirming(true)}>
            Turn into a regular page
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
      <Dialog open={confirming} onOpenChange={setConfirming}>
        <DialogContent className="max-w-[420px] sm:max-w-[420px]">
          <DialogTitle className="text-[15px] font-semibold">Turn into a regular page?</DialogTitle>
          <DialogDescription className="text-[13px] leading-5 text-muted-foreground">
            The table and its property settings go away. The pages inside stay, and each keeps its
            values in its file. The previous version is kept in the page's history.
          </DialogDescription>
          <div className="mt-2 flex justify-end gap-2">
            <button
              type="button"
              onClick={() => setConfirming(false)}
              className="rounded-md px-3 py-1.5 text-[13px] hover:bg-accent"
            >
              Cancel
            </button>
            <button
              type="button"
              onClick={() => {
                setConfirming(false);
                ipc.removeCollection(id).catch(reportError);
              }}
              className="rounded-md bg-primary px-3 py-1.5 text-[13px] text-primary-foreground hover:bg-primary/90"
            >
              Turn into a page
            </button>
          </div>
        </DialogContent>
      </Dialog>
    </>
  );
}

function RowTitle({ row }: { row: CollectionRow }) {
  const openPage = useWorkspaceStore((s) => s.openPage);
  return (
    <button
      type="button"
      onClick={() => openPage(row.id)}
      className="flex h-full min-w-0 flex-1 items-center gap-1.5 text-left font-medium text-foreground hover:underline"
    >
      <span className="inline-flex size-4 shrink-0 items-center justify-center text-muted-foreground">
        {row.icon ?? <FileText className="size-3.5" aria-hidden />}
      </span>
      <span className="truncate">{row.title}</span>
    </button>
  );
}

function ColumnResizer({
  label,
  width,
  onResize,
  onDone,
}: {
  label: string;
  width: number;
  onResize: (width: number) => void;
  onDone: (width: number) => void;
}) {
  const start = useRef<{ x: number; width: number } | null>(null);
  const clamp = (w: number) => Math.min(MAX_WIDTH, Math.max(MIN_WIDTH, Math.round(w)));
  return (
    <button
      type="button"
      aria-label={`Resize the ${label} column`}
      title="Drag, or use ← → keys. Double-click to reset."
      onPointerDown={(e: PointerEvent<HTMLButtonElement>) => {
        e.currentTarget.setPointerCapture(e.pointerId);
        start.current = { x: e.clientX, width };
      }}
      onPointerMove={(e) => {
        if (start.current) onResize(clamp(start.current.width + e.clientX - start.current.x));
      }}
      onPointerUp={(e) => {
        if (!start.current) return;
        onDone(clamp(start.current.width + e.clientX - start.current.x));
        start.current = null;
      }}
      onKeyDown={(e) => {
        if (e.key === "ArrowLeft" || e.key === "ArrowRight") {
          e.preventDefault();
          onDone(clamp(width + (e.key === "ArrowLeft" ? -16 : 16)));
        }
      }}
      onDoubleClick={() => onDone(label === "Title" ? TITLE_WIDTH : COLUMN_WIDTH)}
      className="absolute top-0 right-0 h-full w-1.5 cursor-col-resize outline-none hover:bg-ring/40 focus-visible:bg-ring/60"
    />
  );
}

function TableView({
  data,
  rows,
  view,
  onViewChange,
  canEdit,
  apply,
  setValue,
}: {
  data: CollectionData;
  rows: CollectionRow[];
  view: ViewSettings;
  onViewChange: (view: ViewSettings) => void;
  canEdit: boolean;
  apply: (op: SchemaOp) => Promise<boolean>;
  setValue: (row: CollectionRow, propertyId: string, value: CellInput) => void;
}) {
  const scrollRef = useRef<HTMLDivElement>(null);
  // Widths while dragging; saved to the view when the drag ends.
  const [dragging, setDragging] = useState<Record<string, number>>({});
  const properties = visibleProperties(data.schema, view);
  const widthOf = (key: string, fallback: number) =>
    dragging[key] ?? view.column_widths[key] ?? fallback;
  const columns = [
    { key: "title", width: widthOf("title", TITLE_WIDTH) },
    ...properties.map((p) => ({ key: p.id, width: widthOf(p.id, COLUMN_WIDTH) })),
  ];
  const total = columns.reduce((sum, c) => sum + c.width, 0) + (canEdit ? 40 : 0);
  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 12,
    initialRect: INITIAL_RECT,
  });

  const resizer = (key: string, label: string, fallback: number) => (
    <ColumnResizer
      label={label}
      width={widthOf(key, fallback)}
      onResize={(w) => setDragging({ ...dragging, [key]: w })}
      onDone={(w) => {
        setDragging({});
        onViewChange({ ...view, column_widths: { ...view.column_widths, [key]: w } });
      }}
    />
  );

  const createOption = (propertyId: string) => (name: string) =>
    apply({ op: "add_option", property_id: propertyId, name });

  const items = slots(virtualizer.getVirtualItems(), rows.length, ROW_HEIGHT);
  const paddingTop = items[0]?.start ?? 0;
  const paddingBottom = virtualizer.getTotalSize() - (items.at(-1)?.end ?? 0);
  const headerCell = "relative border-r border-b px-2 text-left font-normal";

  return (
    <div ref={scrollRef} className="max-h-[65vh] overflow-auto">
      <table
        aria-label={data.title}
        aria-rowcount={rows.length + 1}
        className="table-fixed border-collapse text-[13.5px]"
        style={{ width: total }}
      >
        <colgroup>
          {columns.map((c) => (
            <col key={c.key} style={{ width: c.width }} />
          ))}
          {canEdit && <col style={{ width: 40 }} />}
        </colgroup>
        <thead className="sticky top-0 z-10 bg-background text-[12.5px] text-muted-foreground">
          <tr className="h-8">
            <th scope="col" className={headerCell}>
              Title
              {resizer("title", "Title", TITLE_WIDTH)}
            </th>
            {properties.map((property) => (
              <th key={property.id} scope="col" className={headerCell}>
                <PropertyHeader
                  property={property}
                  editable={canEdit}
                  apply={apply}
                  onHide={() => onViewChange({ ...view, hidden: [...view.hidden, property.id] })}
                />
                {resizer(property.id, property.name, COLUMN_WIDTH)}
              </th>
            ))}
            {canEdit && (
              <th scope="col" className="border-b">
                <AddPropertyButton apply={apply} />
              </th>
            )}
          </tr>
        </thead>
        <tbody>
          {paddingTop > 0 && <tr aria-hidden style={{ height: paddingTop }} />}
          {items.map((item) => {
            const row = rows[item.index] as CollectionRow;
            return (
              <tr
                key={row.id}
                aria-rowindex={item.index + 2}
                className="border-b hover:bg-muted"
                style={{ height: ROW_HEIGHT }}
              >
                <td className="truncate border-r px-2">
                  <RowTitle row={row} />
                </td>
                {properties.map((property) => (
                  <td key={property.id} className="min-w-0 border-r px-2">
                    <PropertyCell
                      property={property}
                      cell={row.cells[property.id]}
                      editable={canEdit && row.editable}
                      label={row.title}
                      onChange={(value) => setValue(row, property.id, value)}
                      onCreateOption={createOption(property.id)}
                    />
                  </td>
                ))}
                {canEdit && <td />}
              </tr>
            );
          })}
          {paddingBottom > 0 && <tr aria-hidden style={{ height: paddingBottom }} />}
        </tbody>
      </table>
    </div>
  );
}

function ListView({
  data,
  rows,
  view,
}: {
  data: CollectionData;
  rows: CollectionRow[];
  view: ViewSettings;
}) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const properties = visibleProperties(data.schema, view);
  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ROW_HEIGHT + 4,
    overscan: 12,
    initialRect: INITIAL_RECT,
  });
  return (
    <div ref={scrollRef} className="max-h-[65vh] overflow-y-auto">
      <ul
        aria-label={data.title}
        style={{ height: virtualizer.getTotalSize(), position: "relative" }}
      >
        {slots(virtualizer.getVirtualItems(), rows.length, ROW_HEIGHT + 4).map((item) => {
          const row = rows[item.index] as CollectionRow;
          const shown = properties
            .filter((p) => {
              const cell = row.cells[p.id];
              return cell && cell.kind !== "empty" && cell.kind !== "checkbox";
            })
            .slice(0, 3);
          return (
            <li
              key={row.id}
              className="absolute left-0 flex w-full items-center gap-3 rounded-md px-1.5 text-[13.5px] hover:bg-muted"
              style={{ height: ROW_HEIGHT + 4, transform: `translateY(${item.start}px)` }}
            >
              <RowTitle row={row} />
              <span className="flex shrink-0 items-center gap-2 text-[12.5px] text-muted-foreground">
                {shown.map((p) => (
                  <span key={p.id} className="flex max-w-40 min-w-0 items-center">
                    <CellDisplay property={p} cell={row.cells[p.id]} />
                  </span>
                ))}
              </span>
            </li>
          );
        })}
      </ul>
    </div>
  );
}
