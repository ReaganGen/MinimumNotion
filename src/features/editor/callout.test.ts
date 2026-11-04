import { Editor, type JSONContent } from "@tiptap/core";
import { afterEach, describe, expect, it } from "vitest";
import { CALLOUT_KINDS, calloutKind } from "./callout";
import { createMarkdownExtensions } from "./extensions";
import { docToMarkdown } from "./markdown";

let editor: Editor | null = null;
afterEach(() => {
  editor?.destroy();
  editor = null;
});

function open(markdown: string): Editor {
  editor = new Editor({
    extensions: createMarkdownExtensions(),
    content: markdown,
    contentType: "markdown",
  });
  return editor;
}

const blocks = (e: Editor): JSONContent[] => e.getJSON().content ?? [];

describe("callouts (ADR-048)", () => {
  it("reads a GitHub alert as a callout and keeps its first line", () => {
    const e = open("> [!WARNING]\n> Mind the gap.\n");
    const [callout] = blocks(e);
    expect(callout?.type).toBe("callout");
    expect(callout?.attrs).toEqual({ marker: "WARNING", fold: "", title: "" });
    expect(callout?.content?.[0]?.type).toBe("paragraph");
    expect(docToMarkdown(e)).toBe("> [!WARNING]\n> Mind the gap.\n");
  });

  it("keeps an Obsidian title, fold marker and lowercase type", () => {
    const e = open("> [!tip]+ Read this\n> Body\n");
    expect(blocks(e)[0]?.attrs).toEqual({ marker: "tip", fold: "+", title: " Read this" });
    expect(docToMarkdown(e)).toBe("> [!tip]+ Read this\n> Body\n");
  });

  it("writes an empty callout as its first line alone", () => {
    const e = open("> [!NOTE]\n");
    expect(blocks(e)[0]?.type).toBe("callout");
    expect(docToMarkdown(e)).toBe("> [!NOTE]\n");
  });

  it("leaves quotes without a marker, and lazy continuations, to the blockquote", () => {
    expect(blocks(open("> Just a quote\n"))[0]?.type).toBe("blockquote");
    // CommonMark continues the quote's paragraph on the unprefixed line.
    expect(blocks(open("> [!NOTE] Title\nlazy line\n"))[0]?.type).toBe("blockquote");
  });

  it("names the five types and shows any other word as neutral", () => {
    expect(CALLOUT_KINDS.map((k) => k.label)).toEqual([
      "Note",
      "Tip",
      "Important",
      "Warning",
      "Caution",
    ]);
    expect(calloutKind("caution").tone).toBe("red");
    expect(calloutKind("question")).toEqual({ id: "other", label: "question", tone: "gray" });
  });

  it("Backspace at its start turns the callout back into text", () => {
    const e = open("> [!NOTE]\n> Text\n");
    // Inside the callout (0) and its paragraph (1), before the "T".
    e.commands.setTextSelection(2);
    e.commands.keyboardShortcut("Backspace");
    expect(docToMarkdown(e)).toBe("Text\n");
  });

  it("changing the type rewrites only the marker", () => {
    const e = open("> [!NOTE] Heads up\n> Text\n");
    e.commands.command(({ tr }) => {
      tr.setNodeMarkup(0, undefined, { marker: "CAUTION", fold: "", title: " Heads up" });
      return true;
    });
    expect(docToMarkdown(e)).toBe("> [!CAUTION] Heads up\n> Text\n");
  });
});
