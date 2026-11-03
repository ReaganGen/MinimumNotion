// The toolbar popovers of a collection view: which properties show, how rows
// sort, and which rows a filter keeps (DATA_MODEL §6.5).
import { ArrowUpDown, Eye, Filter, Plus, X } from "lucide-react";
import type { ReactNode } from "react";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import {
  FILTER_OPS,
  type FilterOp,
  kindOf,
  OP_LABELS,
  opNeedsValue,
  PSEUDO,
  PSEUDO_LABELS,
  type PseudoProperty,
} from "@/features/collections/viewLogic";
import type { FilterRule } from "@/lib/ipc/bindings/FilterRule";
import type { Schema } from "@/lib/ipc/bindings/Schema";
import type { SortRule } from "@/lib/ipc/bindings/SortRule";
import type { ViewSettings } from "@/lib/ipc/bindings/ViewSettings";
import { cn } from "@/lib/utils";

interface ControlProps {
  schema: Schema | null;
  view: ViewSettings;
  onChange: (view: ViewSettings) => void;
}

function ToolbarButton({ icon, label, count }: { icon: ReactNode; label: string; count?: number }) {
  return (
    <PopoverTrigger asChild>
      <button
        type="button"
        className={cn(
          "inline-flex h-7 items-center gap-1.5 rounded-md px-2 text-[13px] text-muted-foreground hover:bg-accent hover:text-foreground [&_svg]:size-3.5",
          count ? "text-link" : undefined,
        )}
      >
        {icon}
        {label}
        {count ? <span className="tabular-nums">{count}</span> : null}
      </button>
    </PopoverTrigger>
  );
}

const selectClass = "h-7 min-w-0 rounded-sm bg-muted px-1 text-[12.5px] outline-none";

/** Every property a rule can use: the built-ins, then the schema's. */
function propertyChoices(schema: Schema | null): { id: string; name: string }[] {
  return [
    ...PSEUDO.map((id) => ({ id, name: PSEUDO_LABELS[id as PseudoProperty] })),
    ...(schema?.properties ?? []).map((p) => ({ id: p.id, name: p.name })),
  ];
}

export function PropertiesControl({ schema, view, onChange }: ControlProps) {
  const properties = schema?.properties ?? [];
  return (
    <Popover>
      <ToolbarButton icon={<Eye aria-hidden />} label="Properties" count={view.hidden.length} />
      <PopoverContent align="end" className="w-60 p-1.5">
        {properties.length === 0 && (
          <p className="px-1.5 py-1 text-[13px] text-muted-foreground">No properties yet.</p>
        )}
        {properties.map((property) => (
          <label
            key={property.id}
            className="flex items-center gap-2 rounded-md px-1.5 py-1 text-[13px] hover:bg-accent"
          >
            <input
              type="checkbox"
              checked={!view.hidden.includes(property.id)}
              onChange={(e) =>
                onChange({
                  ...view,
                  hidden: e.target.checked
                    ? view.hidden.filter((id) => id !== property.id)
                    : [...view.hidden, property.id],
                })
              }
              className="size-3.5 accent-(--primary)"
            />
            <span className="truncate">{property.name}</span>
          </label>
        ))}
      </PopoverContent>
    </Popover>
  );
}

export function SortControl({ schema, view, onChange }: ControlProps) {
  const choices = propertyChoices(schema);
  const update = (index: number, rule: SortRule | null) => {
    const sort = [...view.sort];
    if (rule) sort[index] = rule;
    else sort.splice(index, 1);
    onChange({ ...view, sort });
  };
  return (
    <Popover>
      <ToolbarButton icon={<ArrowUpDown aria-hidden />} label="Sort" count={view.sort.length} />
      <PopoverContent align="end" className="w-80">
        {view.sort.map((rule, index) => (
          // Rules are positional and have no IDs; editing one never reorders them.
          // biome-ignore lint/suspicious/noArrayIndexKey: see above
          <div key={index} className="mb-1.5 flex items-center gap-1.5">
            <select
              aria-label="Sort by"
              value={rule.property}
              onChange={(e) => update(index, { ...rule, property: e.target.value })}
              className={cn(selectClass, "flex-1")}
            >
              {choices.map((c) => (
                <option key={c.id} value={c.id}>
                  {c.name}
                </option>
              ))}
            </select>
            <select
              aria-label="Direction"
              value={rule.direction}
              onChange={(e) =>
                update(index, { ...rule, direction: e.target.value as SortRule["direction"] })
              }
              className={selectClass}
            >
              <option value="asc">Ascending</option>
              <option value="desc">Descending</option>
            </select>
            <RemoveButton label="Remove sort" onClick={() => update(index, null)} />
          </div>
        ))}
        <AddRuleButton
          label="Add a sort"
          onClick={() =>
            onChange({ ...view, sort: [...view.sort, { property: "title", direction: "asc" }] })
          }
        />
      </PopoverContent>
    </Popover>
  );
}

export function FilterControl({ schema, view, onChange }: ControlProps) {
  const choices = propertyChoices(schema);
  const update = (index: number, rule: FilterRule | null) => {
    const filter = [...view.filter];
    if (rule) filter[index] = rule;
    else filter.splice(index, 1);
    onChange({ ...view, filter });
  };
  return (
    <Popover>
      <ToolbarButton icon={<Filter aria-hidden />} label="Filter" count={view.filter.length} />
      <PopoverContent align="end" className="w-[26rem]">
        {view.filter.length > 1 && (
          <p className="mb-1.5 text-[12px] text-muted-foreground">Rows must match every filter.</p>
        )}
        {view.filter.map((rule, index) => {
          const kind = kindOf(rule.property, schema);
          const ops = kind ? FILTER_OPS[kind] : [];
          return (
            // Rules are positional and have no IDs; editing one never reorders them.
            // biome-ignore lint/suspicious/noArrayIndexKey: see above
            <div key={index} className="mb-1.5 flex items-center gap-1.5">
              <select
                aria-label="Filter on"
                value={rule.property}
                onChange={(e) => {
                  const next = kindOf(e.target.value, schema);
                  update(index, {
                    property: e.target.value,
                    op: next ? (FILTER_OPS[next][0] as string) : "contains",
                    value: null,
                  });
                }}
                className={cn(selectClass, "w-28")}
              >
                {choices.map((c) => (
                  <option key={c.id} value={c.id}>
                    {c.name}
                  </option>
                ))}
              </select>
              <select
                aria-label="Condition"
                value={rule.op}
                onChange={(e) => update(index, { ...rule, op: e.target.value })}
                className={cn(selectClass, "w-28")}
              >
                {ops.map((op) => (
                  <option key={op} value={op}>
                    {OP_LABELS[op as FilterOp]}
                  </option>
                ))}
              </select>
              {opNeedsValue(rule.op) && (
                <FilterValue
                  rule={rule}
                  schema={schema}
                  onChange={(value) => update(index, { ...rule, value })}
                />
              )}
              <RemoveButton label="Remove filter" onClick={() => update(index, null)} />
            </div>
          );
        })}
        <AddRuleButton
          label="Add a filter"
          onClick={() =>
            onChange({
              ...view,
              filter: [...view.filter, { property: "title", op: "contains", value: null }],
            })
          }
        />
      </PopoverContent>
    </Popover>
  );
}

function FilterValue({
  rule,
  schema,
  onChange,
}: {
  rule: FilterRule;
  schema: Schema | null;
  onChange: (value: string | null) => void;
}) {
  const kind = kindOf(rule.property, schema);
  const property = schema?.properties.find((p) => p.id === rule.property);
  const value = rule.value ?? "";
  if (kind === "checkbox") {
    return (
      <select
        aria-label="Value"
        value={value || "true"}
        onChange={(e) => onChange(e.target.value)}
        className={cn(selectClass, "flex-1")}
      >
        <option value="true">Checked</option>
        <option value="false">Not checked</option>
      </select>
    );
  }
  if ((kind === "select" || kind === "multi_select") && property) {
    return (
      <select
        aria-label="Value"
        value={value}
        onChange={(e) => onChange(e.target.value || null)}
        className={cn(selectClass, "flex-1")}
      >
        <option value="">Choose…</option>
        {property.options.map((o) => (
          <option key={o.id} value={o.name}>
            {o.name}
          </option>
        ))}
      </select>
    );
  }
  return (
    <input
      aria-label="Value"
      type={kind === "date" || kind === "timestamp" ? "date" : "text"}
      inputMode={kind === "number" ? "decimal" : undefined}
      value={value}
      placeholder="Value"
      onChange={(e) => onChange(e.target.value || null)}
      className={cn(selectClass, "min-w-0 flex-1 px-1.5")}
    />
  );
}

function RemoveButton({ label, onClick }: { label: string; onClick: () => void }) {
  return (
    <button
      type="button"
      aria-label={label}
      onClick={onClick}
      className="rounded-sm p-1 text-muted-foreground hover:bg-accent"
    >
      <X className="size-3.5" aria-hidden />
    </button>
  );
}

function AddRuleButton({ label, onClick }: { label: string; onClick: () => void }) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="flex w-full items-center gap-1.5 rounded-md px-1.5 py-1 text-[13px] text-muted-foreground hover:bg-accent"
    >
      <Plus className="size-3.5" aria-hidden /> {label}
    </button>
  );
}
