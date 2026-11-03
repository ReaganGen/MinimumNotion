import { describe, expect, it } from "vitest";
import { resolveImageSource } from "./liveExtensions";
import { filterSlashItems, SLASH_ITEMS } from "./slashCommands";

const PAGE = "11111111-1111-4111-8111-111111111111";

describe("resolveImageSource", () => {
  it("serves relative references through the asset protocol", () => {
    expect(resolveImageSource(PAGE, "_assets/photo%201.png")).toBe(
      `forma-asset://localhost/${PAGE}/_assets/photo%201.png`,
    );
  });

  it("loads https images directly (ADR-031)", () => {
    expect(resolveImageSource(PAGE, " https://example.com/a.png ")).toBe(
      "https://example.com/a.png",
    );
  });

  it("refuses every other scheme and protocol-relative URLs", () => {
    for (const src of [
      "http://example.com/a.png",
      "file:///etc/passwd",
      "javascript:alert(1)",
      "data:image/png;base64,AAAA",
      "//example.com/a.png",
      "",
    ]) {
      expect(resolveImageSource(PAGE, src), src).toBeNull();
    }
  });
});

describe("filterSlashItems", () => {
  it("shows everything for an empty query", () => {
    expect(filterSlashItems("")).toBe(SLASH_ITEMS);
  });

  it("matches titles and keywords, ignoring case", () => {
    expect(filterSlashItems("Head").map((i) => i.title)).toEqual([
      "Heading 1",
      "Heading 2",
      "Heading 3",
    ]);
    expect(filterSlashItems("todo").map((i) => i.title)).toEqual(["To-do list"]);
    expect(filterSlashItems("task").map((i) => i.title)).toEqual(["To-do list"]);
    expect(filterSlashItems("latex").map((i) => i.title)).toEqual(["Math block"]);
  });
});
