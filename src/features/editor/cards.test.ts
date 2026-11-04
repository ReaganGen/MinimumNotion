import { Editor, type JSONContent } from "@tiptap/core";
import { describe, expect, it } from "vitest";
import { bookmarkUrl, escapeLinkTitle, unescapeMarkdown } from "./cards";
import { createMarkdownExtensions } from "./extensions";
import { docToMarkdown, roundTripMarkdown } from "./markdown";
import { escapeInlineText } from "./markdownRules";

function parse(markdown: string): JSONContent[] {
  const editor = new Editor({
    extensions: createMarkdownExtensions(),
    content: markdown,
    contentType: "markdown",
  });
  try {
    return editor.getJSON().content ?? [];
  } finally {
    editor.destroy();
  }
}

/** The types of the inline nodes of each top-level block. */
function inlineTypes(markdown: string): string[][] {
  return parse(markdown).map((block) => (block.content ?? []).map((child) => child.type ?? ""));
}

describe("file cards", () => {
  it("are links into _assets/ alone in a paragraph", () => {
    const [card] = parse("[Report.pdf](_assets/Report.pdf)");
    expect(card?.content).toEqual([
      { type: "attachment", attrs: { label: "Report.pdf", src: "_assets/Report.pdf" } },
    ]);
    expect(inlineTypes("[a](./_assets/a.zip)")).toEqual([["attachment"]]);
  });

  it("leave other links alone", () => {
    expect(inlineTypes("[Report](Report.pdf)")).toEqual([["text"]]);
    expect(inlineTypes("See [a](_assets/a.pdf) here")).toEqual([["text", "text", "text"]]);
    expect(inlineTypes('[a](_assets/a.pdf "title")')).toEqual([["text"]]);
    expect(inlineTypes("![a](_assets/a.pdf)")).toEqual([["image"]]);
  });

  it("keep the label as written", () => {
    const input = "[Budget \\[draft\\]\\_v2.xlsx](_assets/Budget.xlsx)\n";
    expect(roundTripMarkdown(input)).toBe(input);
    const [card] = parse(input);
    expect(unescapeMarkdown(card?.content?.[0]?.attrs?.label)).toBe("Budget [draft]_v2.xlsx");
  });

  it("work in quotes and loose list items, and stay links in tight lists", () => {
    const quote = parse("> [a.pdf](_assets/a.pdf)")[0];
    expect(quote?.content?.[0]?.content?.[0]?.type).toBe("attachment");
    const input = "- [a.pdf](_assets/a.pdf)\n- [b.pdf](_assets/b.pdf)\n";
    expect(roundTripMarkdown(input)).toBe(input);
  });
});

describe("bookmarks", () => {
  it("are web links with a title, alone in a paragraph", () => {
    const [block] = parse('[Diaspar](https://example.com/d "Local notes")');
    expect(block?.content).toEqual([
      {
        type: "bookmark",
        attrs: { label: "Diaspar", href: "https://example.com/d", description: "Local notes" },
      },
    ]);
    expect(inlineTypes('[x](HTTP://example.com "")')).toEqual([["bookmark"]]);
  });

  it("leave links without a title, or with other schemes, alone", () => {
    expect(inlineTypes("[Diaspar](https://example.com)")).toEqual([["text"]]);
    expect(inlineTypes('[Mail](mailto:a@b.c "x")')).toEqual([["text"]]);
    expect(inlineTypes('[Page](diaspar://page/x "x")')).toEqual([["text"]]);
    expect(inlineTypes('Read [this](https://e.com "t") now')).toEqual([["text", "text", "text"]]);
  });

  it("write what the user typed so it reads back the same", () => {
    const label = escapeInlineText('A *bold* [claim] \\ "quoted"');
    const description = escapeLinkTitle('She said "hi" \\ bye\nthen left');
    const href = bookmarkUrl("example.com/a b(1)");
    expect(href).toBe("https://example.com/a%20b%281%29");
    const markdown = `[${label}](${href} "${description}")\n`;
    expect(roundTripMarkdown(markdown)).toBe(markdown);
    const attrs = parse(markdown)[0]?.content?.[0]?.attrs;
    expect(unescapeMarkdown(attrs?.label)).toBe('A *bold* [claim] \\ "quoted"');
    expect(unescapeMarkdown(attrs?.description)).toBe('She said "hi" \\ bye then left');
  });

  it("only take http and https URLs", () => {
    expect(bookmarkUrl("https://example.com")).toBe("https://example.com/");
    expect(bookmarkUrl("  http://a.org/x?y=1 ")).toBe("http://a.org/x?y=1");
    for (const bad of [
      "",
      "javascript:alert(1)",
      "file:///etc/hosts",
      "mailto:a@b.c",
      "https://",
    ]) {
      expect(bookmarkUrl(bad), bad).toBeNull();
    }
  });
});

describe("lone images", () => {
  it("stay in their paragraphs, so replacing the content keeps them apart", () => {
    const markdown = "![a](_assets/a.png)\n\n![b](_assets/b.mp4)\n";
    expect(inlineTypes(markdown)).toEqual([["image"], ["image"]]);
    const editor = new Editor({ extensions: createMarkdownExtensions() });
    try {
      // What a reload after an outside change, or after adding an attachment, does.
      editor.commands.setContent(markdown, { contentType: "markdown" });
      expect(editor.getJSON().content?.map((b) => b.type)).toEqual(["paragraph", "paragraph"]);
      expect(docToMarkdown(editor)).toBe(markdown);
    } finally {
      editor.destroy();
    }
  });
});

describe("pasted cards", () => {
  function paste(html: string): JSONContent[] {
    const editor = new Editor({ extensions: createMarkdownExtensions() });
    try {
      editor.commands.setContent(html);
      return editor.getJSON().content ?? [];
    } finally {
      editor.destroy();
    }
  }

  it("keep cards copied from Diaspar", () => {
    const [block] = paste(
      '<p><span data-bookmark="https://example.com/" data-label="Ex" data-description="d">Ex</span></p>',
    );
    expect(block?.content?.[0]).toEqual({
      type: "bookmark",
      attrs: { label: "Ex", href: "https://example.com/", description: "d" },
    });
    const [file] = paste(
      '<p><span data-attachment="_assets/a.pdf" data-label="a.pdf">a</span></p>',
    );
    expect(file?.content?.[0]?.type).toBe("attachment");
  });

  it("refuse anything Markdown wouldn't read back as a card", () => {
    const types = (html: string) =>
      paste(html).flatMap((b) => (b.content ?? []).map((c) => c.type ?? ""));
    expect(
      types('<p><span data-bookmark="javascript:alert(1)" data-label="x">x</span></p>'),
    ).toEqual(["text"]);
    expect(
      types('<p><span data-attachment="../../secret.txt" data-label="s">s</span></p>'),
    ).toEqual(["text"]);
    expect(
      types('<p><span data-attachment="_assets/a.pdf" data-label="a](evil) [b">x</span></p>'),
    ).toEqual(["text"]);
    expect(paste('<pre data-html-embed="python"><code>x</code></pre>')[0]?.type).not.toBe(
      "htmlEmbed",
    );
    expect(paste('<pre data-html-embed="html embed"><code>&lt;b&gt;</code></pre>')[0]).toEqual({
      type: "htmlEmbed",
      attrs: { info: "html embed", code: "<b>" },
    });
  });
});
