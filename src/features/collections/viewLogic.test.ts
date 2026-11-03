import { describe, expect, it } from "vitest";
import type { Cell } from "@/lib/ipc/bindings/Cell";
import type { CollectionRow } from "@/lib/ipc/bindings/CollectionRow";
import type { Schema } from "@/lib/ipc/bindings/Schema";
import { applyView, EMPTY_VIEW, matches, sortRows, visibleProperties } from "./viewLogic";

const schema: Schema = {
  version: 1,
  properties: [
    { id: "status00", name: "Status", type: "select", options: [] },
    { id: "year0000", name: "Year", type: "number", options: [] },
    { id: "tags0000", name: "Authors", type: "multi_select", options: [] },
    { id: "pub00000", name: "Published", type: "date", options: [] },
    { id: "read0000", name: "Read", type: "checkbox", options: [] },
    { id: "note0000", name: "Notes", type: "text", options: [] },
  ],
};

function row(
  id: string,
  cells: Record<string, Cell>,
  extra: Partial<CollectionRow> = {},
): CollectionRow {
  return {
    id,
    title: id,
    icon: null,
    created_at: null,
    updated_at: null,
    cells,
    editable: true,
    ...extra,
  };
}

const rows = [
  row("b", {
    status00: { kind: "select", value: "Reading", known: true },
    year0000: { kind: "number", value: 2017, from_text: false },
    tags0000: { kind: "multi_select", values: ["Vaswani", "Shazeer"], unknown: [] },
    pub00000: { kind: "date", value: "2017-06-12", from_timestamp: false },
    read0000: { kind: "checkbox", value: true },
    note0000: { kind: "text", value: "Attention" },
  }),
  row("a", {
    status00: { kind: "select", value: "To read", known: true },
    year0000: { kind: "number", value: 2020, from_text: true },
    read0000: { kind: "checkbox", value: false },
  }),
  row("c", { year0000: { kind: "invalid", raw: "twelve", problem: "not a number" } }),
];
const ids = (list: CollectionRow[]) => list.map((r) => r.id);

describe("sortRows", () => {
  it("sorts by any property, with empty values last either way", () => {
    expect(ids(sortRows(rows, [{ property: "year0000", direction: "asc" }]))).toEqual([
      "b",
      "a",
      "c",
    ]);
    expect(ids(sortRows(rows, [{ property: "status00", direction: "desc" }]))).toEqual([
      "a",
      "b",
      "c",
    ]);
    expect(ids(sortRows(rows, [{ property: "status00", direction: "asc" }]))).toEqual([
      "b",
      "a",
      "c",
    ]);
    expect(ids(sortRows(rows, [{ property: "title", direction: "asc" }]))).toEqual(["a", "b", "c"]);
  });

  it("uses the next rule to break ties and keeps order without rules", () => {
    const tie = [row("x", { read0000: { kind: "checkbox", value: true } }), ...rows];
    expect(
      ids(
        sortRows(tie, [
          { property: "read0000", direction: "desc" },
          { property: "title", direction: "desc" },
        ]),
      ),
    ).toEqual(["x", "b", "a", "c"]);
    expect(sortRows(rows, [])).toBe(rows);
  });
});

describe("filters", () => {
  const f = (property: string, op: string, value: string | null = null) =>
    ids(rows.filter((r) => matches(r, { property, op, value }, schema)));

  it("covers every operator family", () => {
    expect(f("note0000", "contains", "atten")).toEqual(["b"]);
    expect(f("note0000", "not_contains", "atten")).toEqual(["a", "c"]);
    expect(f("note0000", "empty")).toEqual(["a", "c"]);
    expect(f("year0000", "gt", "2018")).toEqual(["a"]);
    expect(f("year0000", "lte", "2017")).toEqual(["b"]);
    expect(f("year0000", "neq", "2017")).toEqual(["a", "c"]);
    expect(f("pub00000", "before", "2018-01-01")).toEqual(["b"]);
    expect(f("read0000", "is", "true")).toEqual(["b"]);
    expect(f("read0000", "is", "false")).toEqual(["a", "c"]);
    expect(f("status00", "is", "Reading")).toEqual(["b"]);
    expect(f("status00", "is_not", "Reading")).toEqual(["a", "c"]);
    expect(f("tags0000", "contains", "Shazeer")).toEqual(["b"]);
    expect(f("tags0000", "not_empty")).toEqual(["b"]);
    expect(f("title", "contains", "A")).toEqual(["a"]);
  });

  it("ignores filters without a value or on removed properties", () => {
    expect(f("note0000", "contains", "")).toEqual(["b", "a", "c"]);
    expect(f("gone0000", "is", "x")).toEqual(["b", "a", "c"]);
  });
});

describe("applyView", () => {
  it("filters with AND, then sorts", () => {
    const view = {
      ...EMPTY_VIEW,
      filter: [
        { property: "year0000", op: "gte", value: "2017" },
        { property: "read0000", op: "is", value: "false" },
      ],
      sort: [{ property: "year0000", direction: "desc" as const }],
    };
    expect(ids(applyView(rows, schema, view))).toEqual(["a"]);
  });

  it("hides columns the view hides", () => {
    const view = { ...EMPTY_VIEW, hidden: ["year0000", "read0000"] };
    expect(visibleProperties(schema, view).map((p) => p.name)).toEqual([
      "Status",
      "Authors",
      "Published",
      "Notes",
    ]);
  });
});
