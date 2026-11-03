// Sorting and filtering a collection's rows (DATA_MODEL §6.5). Pure, so the
// rules are tested without the UI. Filters combine with AND.
import type { Cell } from "@/lib/ipc/bindings/Cell";
import type { CollectionRow } from "@/lib/ipc/bindings/CollectionRow";
import type { FilterRule } from "@/lib/ipc/bindings/FilterRule";
import type { Property } from "@/lib/ipc/bindings/Property";
import type { PropertyType } from "@/lib/ipc/bindings/PropertyType";
import type { Schema } from "@/lib/ipc/bindings/Schema";
import type { SortRule } from "@/lib/ipc/bindings/SortRule";
import type { ViewSettings } from "@/lib/ipc/bindings/ViewSettings";

/** Built-in columns that can be sorted and filtered but aren't in the schema. */
export const PSEUDO = ["title", "created_at", "updated_at"] as const;
export type PseudoProperty = (typeof PSEUDO)[number];

export const PSEUDO_LABELS: Record<PseudoProperty, string> = {
  title: "Title",
  created_at: "Created",
  updated_at: "Updated",
};

export type FilterOp =
  | "contains"
  | "not_contains"
  | "empty"
  | "not_empty"
  | "eq"
  | "neq"
  | "lt"
  | "lte"
  | "gt"
  | "gte"
  | "is"
  | "is_not"
  | "before"
  | "after";

type Kind = PropertyType | "title" | "timestamp";

export const FILTER_OPS: Record<Kind, FilterOp[]> = {
  text: ["contains", "not_contains", "empty", "not_empty"],
  title: ["contains", "not_contains", "empty", "not_empty"],
  number: ["eq", "neq", "lt", "lte", "gt", "gte", "empty", "not_empty"],
  date: ["is", "before", "after", "empty", "not_empty"],
  timestamp: ["is", "before", "after"],
  checkbox: ["is"],
  select: ["is", "is_not", "empty", "not_empty"],
  multi_select: ["contains", "not_contains", "empty", "not_empty"],
};

export const OP_LABELS: Record<FilterOp, string> = {
  contains: "contains",
  not_contains: "doesn't contain",
  empty: "is empty",
  not_empty: "isn't empty",
  eq: "=",
  neq: "≠",
  lt: "<",
  lte: "≤",
  gt: ">",
  gte: "≥",
  is: "is",
  is_not: "isn't",
  before: "is before",
  after: "is after",
};

/** Operators that don't take a value. */
export function opNeedsValue(op: string): boolean {
  return op !== "empty" && op !== "not_empty";
}

export function kindOf(property: string, schema: Schema | null): Kind | null {
  if (property === "title") return "title";
  if (property === "created_at" || property === "updated_at") return "timestamp";
  return schema?.properties.find((p) => p.id === property)?.type ?? null;
}

/** What sorting and filtering compare: a string, a number, a boolean, or nothing. */
type Comparable = string | number | boolean | string[] | null;

function cellValue(cell: Cell | undefined): Comparable {
  switch (cell?.kind) {
    case "text":
      return cell.value;
    case "number":
      return cell.value;
    case "date":
      return cell.value;
    case "checkbox":
      return cell.value;
    case "select":
      return cell.value;
    case "multi_select":
      return cell.values;
    case "invalid":
      return cell.raw;
    default:
      return null;
  }
}

export function rowValue(row: CollectionRow, property: string): Comparable {
  if (property === "title") return row.title;
  if (property === "created_at") return row.created_at?.slice(0, 10) ?? null;
  if (property === "updated_at") return row.updated_at?.slice(0, 10) ?? null;
  return cellValue(row.cells[property]);
}

const collator = new Intl.Collator(undefined, { numeric: true, sensitivity: "base" });

function compareValues(a: Comparable, b: Comparable): number {
  // Empty values sort last whichever the direction (handled by the caller).
  if (typeof a === "number" && typeof b === "number") return a - b;
  if (typeof a === "boolean" && typeof b === "boolean") return Number(a) - Number(b);
  const text = (v: Comparable) => (Array.isArray(v) ? v.join(", ") : String(v));
  return collator.compare(text(a), text(b));
}

const isEmpty = (v: Comparable) => v === null || v === "" || (Array.isArray(v) && v.length === 0);

export function sortRows(rows: CollectionRow[], rules: SortRule[]): CollectionRow[] {
  if (rules.length === 0) return rows;
  return [...rows].sort((a, b) => {
    for (const rule of rules) {
      const va = rowValue(a, rule.property);
      const vb = rowValue(b, rule.property);
      if (isEmpty(va) || isEmpty(vb)) {
        if (isEmpty(va) && isEmpty(vb)) continue;
        return isEmpty(va) ? 1 : -1;
      }
      const order = compareValues(va, vb);
      if (order !== 0) return rule.direction === "asc" ? order : -order;
    }
    return 0;
  });
}

export function matches(row: CollectionRow, rule: FilterRule, schema: Schema | null): boolean {
  const kind = kindOf(rule.property, schema);
  if (!kind) return true; // a filter on a removed property filters nothing
  const value = rowValue(row, rule.property);
  const wanted = rule.value ?? "";
  if (rule.op === "empty") return isEmpty(value);
  if (rule.op === "not_empty") return !isEmpty(value);
  if (opNeedsValue(rule.op) && wanted === "" && kind !== "checkbox") return true;
  const lower = (v: Comparable) => (Array.isArray(v) ? "" : String(v ?? "").toLowerCase());
  switch (kind) {
    case "text":
    case "title":
      return rule.op === "contains"
        ? lower(value).includes(wanted.toLowerCase())
        : !lower(value).includes(wanted.toLowerCase());
    case "number": {
      if (typeof value !== "number") return rule.op === "neq";
      const n = Number(wanted);
      if (Number.isNaN(n)) return true;
      return {
        eq: value === n,
        neq: value !== n,
        lt: value < n,
        lte: value <= n,
        gt: value > n,
        gte: value >= n,
      }[rule.op as "eq"];
    }
    case "date":
    case "timestamp":
      if (typeof value !== "string") return false;
      if (rule.op === "is") return value === wanted;
      if (rule.op === "before") return value < wanted;
      return value > wanted;
    case "checkbox":
      return (value === true) === (wanted === "true");
    case "select":
      return rule.op === "is" ? value === wanted : value !== wanted;
    case "multi_select": {
      const has = Array.isArray(value) && value.includes(wanted);
      return rule.op === "contains" ? has : !has;
    }
  }
  return true;
}

/** The rows a view shows, in its order. */
export function applyView(
  rows: CollectionRow[],
  schema: Schema | null,
  view: ViewSettings,
): CollectionRow[] {
  const filtered = rows.filter((row) => view.filter.every((rule) => matches(row, rule, schema)));
  return sortRows(filtered, view.sort);
}

/** Properties shown as columns, in schema order. */
export function visibleProperties(schema: Schema | null, view: ViewSettings): Property[] {
  return (schema?.properties ?? []).filter((p) => !view.hidden.includes(p.id));
}

export const EMPTY_VIEW: ViewSettings = { sort: [], filter: [], hidden: [], column_widths: {} };
