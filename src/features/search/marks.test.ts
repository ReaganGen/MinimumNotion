import { describe, expect, it } from "vitest";
import { markTitle, splitMarks } from "./marks";

describe("splitMarks", () => {
  it("turns markers into runs", () => {
    expect(splitMarks("…the \u0001protein\u0002 design \u0001pro\u0002")).toEqual([
      { text: "…the ", match: false },
      { text: "protein", match: true },
      { text: " design ", match: false },
      { text: "pro", match: true },
    ]);
  });

  it("keeps markup-looking text as text", () => {
    expect(splitMarks("<b>\u0001x\u0002</b>")).toEqual([
      { text: "<b>", match: false },
      { text: "x", match: true },
      { text: "</b>", match: false },
    ]);
  });
});

describe("markTitle", () => {
  it("marks each word of the query, ignoring case", () => {
    expect(markTitle("Project notes", "proj NOTES")).toEqual([
      { text: "Proj", match: true },
      { text: "ect ", match: false },
      { text: "notes", match: true },
    ]);
    expect(markTitle("知识管理", "管理")).toEqual([
      { text: "知识", match: false },
      { text: "管理", match: true },
    ]);
    expect(markTitle("Plain", " ")).toEqual([{ text: "Plain", match: false }]);
  });
});
