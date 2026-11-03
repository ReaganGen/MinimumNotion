import { describe, expect, it } from "vitest";
import { dropZone, placeDrop } from "./dropPlacement";

describe("dropZone", () => {
  it("splits a row into before, inside and after", () => {
    expect(dropZone(2, 28)).toBe("before");
    expect(dropZone(14, 28)).toBe("inside");
    expect(dropZone(26, 28)).toBe("after");
  });
});

describe("placeDrop", () => {
  const siblings = ["a", "b", "c", "d"];
  const target = (id: string) => ({ id, parentId: "p" });

  it("nests inside the target", () => {
    expect(placeDrop("x", target("b"), "inside", siblings)).toEqual({
      parentId: "b",
      position: null,
    });
  });

  it("inserts a page from elsewhere before or after the target", () => {
    expect(placeDrop("x", target("b"), "before", siblings)).toEqual({ parentId: "p", position: 1 });
    expect(placeDrop("x", target("b"), "after", siblings)).toEqual({ parentId: "p", position: 2 });
  });

  it("accounts for the dragged page's own slot when reordering", () => {
    // a b c d: move a after c → b c a d
    expect(placeDrop("a", target("c"), "after", siblings)).toEqual({ parentId: "p", position: 2 });
    // move d before b → a d b c
    expect(placeDrop("d", target("b"), "before", siblings)).toEqual({ parentId: "p", position: 1 });
  });

  it("ignores drops that change nothing", () => {
    expect(placeDrop("b", target("b"), "inside", siblings)).toBeNull();
    expect(placeDrop("b", target("c"), "before", siblings)).toBeNull();
    expect(placeDrop("b", target("a"), "after", siblings)).toBeNull();
  });
});
