import { describe, expect, it } from "vitest";
import { collapseDiff, dependents } from "./diff";

describe("diff display", () => {
  it("keeps context around changes and hides long unchanged runs", () => {
    const long = "a".repeat(500);
    const parts = collapseDiff(
      [
        { kind: "equal", text: long },
        { kind: "insert", text: "new" },
        { kind: "equal", text: long },
        { kind: "delete", text: "old" },
        { kind: "equal", text: long },
      ],
      80,
    );
    expect(parts.map((p) => p.kind)).toEqual([
      "gap",
      "equal",
      "insert",
      "equal",
      "gap",
      "equal",
      "delete",
      "equal",
      "gap",
    ]);
    expect(parts[0]).toEqual({ kind: "gap", hidden: 420 });
    expect(parts[4]).toEqual({ kind: "gap", hidden: 340 });
    const short = [
      { kind: "equal" as const, text: "short text" },
      { kind: "insert" as const, text: "x" },
    ];
    expect(collapseDiff(short)).toEqual(short);
    // Characters, not UTF-16 units: CJK and emoji aren't split.
    const cjk = collapseDiff(
      [
        { kind: "insert", text: "x" },
        { kind: "equal", text: "注".repeat(200) },
      ],
      10,
    );
    expect(cjk[1]).toEqual({ kind: "equal", text: "注".repeat(10) });
  });

  it("finds the changes a change needs and the ones that need it", () => {
    const changes = [
      { change_id: "1", depends_on: [] },
      { change_id: "2", depends_on: ["1"] },
      { change_id: "3", depends_on: ["2"] },
      { change_id: "4", depends_on: [] },
    ];
    expect(dependents(changes, "3")).toEqual({ needs: ["2", "1"], neededBy: [] });
    expect(dependents(changes, "1")).toEqual({ needs: [], neededBy: ["2", "3"] });
    expect(dependents(changes, "4")).toEqual({ needs: [], neededBy: [] });
  });
});
