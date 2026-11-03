// Collection cells: how each property type shows a value, and how it's
// edited in place (DATA_MODEL §6.3). Invalid values show as written, with a
// warning, and change only when the user edits them.
import {
  AlertTriangle,
  Calendar,
  CheckSquare,
  ChevronDown,
  Hash,
  List,
  type LucideIcon,
  Type,
  X,
} from "lucide-react";
import { type KeyboardEvent, useState } from "react";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import type { Cell } from "@/lib/ipc/bindings/Cell";
import type { CellInput } from "@/lib/ipc/bindings/CellInput";
import type { Property } from "@/lib/ipc/bindings/Property";
import type { PropertyType } from "@/lib/ipc/bindings/PropertyType";
import { cn } from "@/lib/utils";

export const TYPE_LABELS: Record<PropertyType, string> = {
  text: "Text",
  number: "Number",
  date: "Date",
  checkbox: "Checkbox",
  select: "Select",
  multi_select: "Multi-select",
};

export const TYPE_ICONS: Record<PropertyType, LucideIcon> = {
  text: Type,
  number: Hash,
  date: Calendar,
  checkbox: CheckSquare,
  select: ChevronDown,
  multi_select: List,
};

export function OptionChip({ name, color }: { name: string; color: string | null }) {
  return (
    <span
      className="inline-flex max-w-full items-center truncate rounded px-1.5 py-px text-[12.5px] leading-5"
      style={{
        background: `var(--tag-${color ?? "gray"}-bg)`,
        color: `var(--tag-${color ?? "gray"}-fg)`,
      }}
      title={color ? undefined : "Not one of this property's options"}
    >
      {name}
      {!color && <span className="ml-0.5 opacity-70">?</span>}
    </span>
  );
}

function colorOf(property: Property, name: string): string | null {
  return property.options.find((o) => o.name === name)?.color ?? null;
}

export function CellDisplay({ property, cell }: { property: Property; cell: Cell | undefined }) {
  switch (cell?.kind) {
    case undefined:
    case "empty":
    case "checkbox": // drawn by the cell itself, as a working checkbox
      return null;
    case "text":
      return <span className="truncate">{cell.value}</span>;
    case "number":
      return (
        <span
          className={cn("truncate tabular-nums", cell.from_text && "underline decoration-dotted")}
          title={cell.from_text ? "Written as text in the file" : undefined}
        >
          {String(cell.value)}
        </span>
      );
    case "date":
      return (
        <span
          className="truncate tabular-nums"
          title={cell.from_timestamp ? "The file has a full timestamp" : undefined}
        >
          {cell.value}
        </span>
      );
    case "select":
      return <OptionChip name={cell.value} color={colorOf(property, cell.value)} />;
    case "multi_select":
      return (
        <span className="flex min-w-0 gap-1 overflow-hidden">
          {cell.values.map((v) => (
            <OptionChip key={v} name={v} color={colorOf(property, v)} />
          ))}
        </span>
      );
    case "invalid":
      return (
        <span
          className="flex min-w-0 items-center gap-1 text-muted-foreground"
          title={`This value is ${cell.problem}. It's kept as written until you change it.`}
        >
          <AlertTriangle
            className="size-3.5 shrink-0 text-destructive"
            aria-label="Invalid value"
          />
          <span className="truncate">{cell.raw}</span>
        </span>
      );
  }
}

/** The text a cell edits as, for text, number and date properties. */
function editText(cell: Cell | undefined): string {
  switch (cell?.kind) {
    case "text":
      return cell.value;
    case "number":
      return String(cell.value);
    case "date":
      return cell.value;
    case "invalid":
      return cell.raw;
    default:
      return "";
  }
}

/** What typed text becomes, or an error message. */
export function parseInput(type: PropertyType, text: string): CellInput | string {
  const value = text.trim();
  if (value === "") return { kind: "empty" };
  switch (type) {
    case "number": {
      const n = Number(value.replace(/,/g, ""));
      return Number.isFinite(n) ? { kind: "number", value: n } : "Enter a number.";
    }
    case "date":
      return /^\d{4}-\d{2}-\d{2}$/.test(value) && !Number.isNaN(Date.parse(value))
        ? { kind: "date", value }
        : "Use YYYY-MM-DD.";
    default:
      return { kind: "text", value: text };
  }
}

interface CellProps {
  property: Property;
  cell: Cell | undefined;
  editable: boolean;
  onChange: (value: CellInput) => void;
  onCreateOption: (name: string) => Promise<boolean>;
  /** Used in the editor's accessible name, e.g. the row's title. */
  label: string;
}

/** A cell that shows its value and edits it in place. */
export function PropertyCell(props: CellProps) {
  const { property, cell, editable } = props;
  if (property.type === "checkbox") {
    if (cell?.kind === "invalid") return <CellDisplay property={property} cell={cell} />;
    return (
      <span className="flex h-full items-center">
        <input
          type="checkbox"
          aria-label={`${property.name} for ${props.label}`}
          checked={cell?.kind === "checkbox" && cell.value}
          disabled={!editable}
          onChange={(e) => props.onChange({ kind: "checkbox", value: e.target.checked })}
          className="size-3.5 accent-(--primary)"
        />
      </span>
    );
  }
  if (property.type === "select" || property.type === "multi_select") {
    return <OptionCell {...props} />;
  }
  return <TextCell {...props} />;
}

function TextCell({ property, cell, editable, onChange, label }: CellProps) {
  const [editing, setEditing] = useState(false);
  const [text, setText] = useState("");
  const [problem, setProblem] = useState<string | null>(null);

  function start() {
    if (!editable) return;
    setText(editText(cell));
    setProblem(null);
    setEditing(true);
  }

  function commit() {
    const parsed = parseInput(property.type, text);
    if (typeof parsed === "string") {
      setProblem(parsed);
      return;
    }
    setEditing(false);
    if (text !== editText(cell)) onChange(parsed);
  }

  function onKeyDown(event: KeyboardEvent<HTMLInputElement>) {
    if (event.key === "Enter" && !event.nativeEvent.isComposing) commit();
    if (event.key === "Escape") {
      event.stopPropagation();
      setEditing(false);
    }
  }

  if (editing) {
    return (
      <input
        // biome-ignore lint/a11y/noAutofocus: editing starts from a click on the cell
        autoFocus
        aria-label={`${property.name} for ${label}`}
        aria-invalid={problem ? true : undefined}
        title={problem ?? undefined}
        type={property.type === "date" ? "date" : "text"}
        inputMode={property.type === "number" ? "decimal" : undefined}
        value={text}
        onChange={(e) => {
          setText(e.target.value);
          setProblem(null);
        }}
        onBlur={commit}
        onKeyDown={onKeyDown}
        className={cn(
          "h-7 w-full rounded-sm bg-background px-1.5 text-[13.5px] outline-none ring-1 ring-ring",
          problem && "ring-destructive",
        )}
      />
    );
  }
  return (
    <button
      type="button"
      disabled={!editable}
      onClick={start}
      aria-label={`${property.name} for ${label}`}
      className="flex h-full w-full min-w-0 items-center text-left disabled:cursor-default"
    >
      <CellDisplay property={property} cell={cell} />
    </button>
  );
}

function OptionCell({ property, cell, editable, onChange, onCreateOption, label }: CellProps) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const multi = property.type === "multi_select";
  const chosen: string[] =
    cell?.kind === "select" ? [cell.value] : cell?.kind === "multi_select" ? cell.values : [];
  const matching = property.options.filter((o) =>
    o.name.toLowerCase().includes(query.trim().toLowerCase()),
  );
  const exact = property.options.some((o) => o.name === query.trim());

  function choose(name: string) {
    if (multi) {
      const next = chosen.includes(name) ? chosen.filter((n) => n !== name) : [...chosen, name];
      onChange(next.length ? { kind: "multi_select", values: next } : { kind: "empty" });
    } else {
      onChange(chosen[0] === name ? { kind: "empty" } : { kind: "select", value: name });
      setOpen(false);
    }
  }

  async function create() {
    const name = query.trim();
    if (!name || exact) return;
    if (await onCreateOption(name)) {
      setQuery("");
      choose(name);
    }
  }

  return (
    <Popover open={open} onOpenChange={(next) => setOpen(editable && next)}>
      <PopoverTrigger asChild>
        <button
          type="button"
          disabled={!editable}
          aria-label={`${property.name} for ${label}`}
          className="flex h-full w-full min-w-0 items-center text-left disabled:cursor-default"
        >
          <CellDisplay property={property} cell={cell} />
        </button>
      </PopoverTrigger>
      <PopoverContent className="w-60 p-1.5">
        <input
          // biome-ignore lint/a11y/noAutofocus: the popover opens to pick or type an option
          autoFocus
          aria-label="Find or create an option"
          placeholder="Find or create an option…"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => {
            if (e.key !== "Enter" || e.nativeEvent.isComposing) return;
            const first = matching[0];
            if (first && (exact || !query.trim())) choose(first.name);
            else void create();
          }}
          className="mb-1 h-7 w-full rounded-sm bg-muted px-2 text-[13px] outline-none"
        />
        <div role="listbox" aria-label={property.name} aria-multiselectable={multi}>
          {matching.map((option) => (
            <button
              key={option.id}
              type="button"
              role="option"
              aria-selected={chosen.includes(option.name)}
              onClick={() => choose(option.name)}
              className="flex w-full items-center justify-between rounded-md px-1.5 py-1 hover:bg-accent"
            >
              <OptionChip name={option.name} color={option.color} />
              {chosen.includes(option.name) && (
                <span className="text-[12px] text-muted-foreground">✓</span>
              )}
            </button>
          ))}
        </div>
        {query.trim() && !exact && (
          <button
            type="button"
            onClick={() => void create()}
            className="mt-0.5 w-full rounded-md px-1.5 py-1 text-left text-[13px] hover:bg-accent"
          >
            Create “{query.trim()}”
          </button>
        )}
        {chosen.length > 0 && (
          <button
            type="button"
            onClick={() => {
              onChange({ kind: "empty" });
              setOpen(false);
            }}
            className="mt-0.5 flex w-full items-center gap-1.5 rounded-md px-1.5 py-1 text-left text-[13px] text-muted-foreground hover:bg-accent"
          >
            <X className="size-3.5" aria-hidden /> Clear
          </button>
        )}
        {property.options.length === 0 && !query.trim() && (
          <p className="px-1.5 py-1 text-[12.5px] text-muted-foreground">
            No options yet. Type one to create it.
          </p>
        )}
      </PopoverContent>
    </Popover>
  );
}
