import { Editor } from "@tiptap/core";
import { afterEach, describe, expect, it } from "vitest";
import { createMarkdownExtensions } from "./extensions";
import { Flash, findTextRange, flashKey } from "./reveal";

let editor: Editor | null = null;
afterEach(() => editor?.destroy());

function open(markdown: string): Editor {
  editor = new Editor({
    extensions: [...createMarkdownExtensions(), Flash],
    content: markdown,
    contentType: "markdown",
  });
  return editor;
}

describe("revealing a cited passage", () => {
  it("finds text across blocks, whitespace and case", () => {
    const ed = open("# Methods\n\nWe used **cells** and\nmice.\n\n- One item\n");
    const range = findTextRange(ed.state.doc, "methods  WE USED cells and mice.");
    expect(range).not.toBeNull();
    expect(
      ed.state.doc.textBetween(range?.from ?? 0, range?.to ?? 0, " ").replace(/\s+/g, " "),
    ).toBe("Methods We used cells and mice.");
    expect(findTextRange(ed.state.doc, "nowhere in this page")).toBeNull();
  });

  it("falls back to the start of a long excerpt", () => {
    const ed = open("Glaciers carve valleys over thousands of years.\n");
    const range = findTextRange(
      ed.state.doc,
      "Glaciers carve valleys over thousands of years, and the text Rust saw goes on differently here…",
    );
    expect(ed.state.doc.textBetween(range?.from ?? 0, range?.to ?? 0)).toBe(
      "Glaciers carve valleys over thousands of years.",
    );
  });

  it("highlights with a decoration and leaves the document unchanged", () => {
    const ed = open("Some text to show.\n");
    const before = ed.state.doc;
    let changed = false;
    ed.on("update", () => {
      changed = true;
    });
    ed.view.dispatch(ed.state.tr.setMeta(flashKey, { from: 1, to: 5 }));
    expect(flashKey.getState(ed.state)?.find().length).toBe(1);
    expect(ed.view.dom.querySelector(".diaspar-flash")?.textContent).toBe("Some");
    ed.view.dispatch(ed.state.tr.setMeta(flashKey, null));
    expect(flashKey.getState(ed.state)?.find().length).toBe(0);
    expect(ed.state.doc.eq(before)).toBe(true);
    expect(changed).toBe(false);
  });
});
