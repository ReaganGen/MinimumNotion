import { Editor, type JSONContent } from "@tiptap/core";
import { describe, expect, it } from "vitest";
import { createMarkdownExtensions } from "./extensions";
import { docToMarkdown } from "./markdown";
import { PageLink } from "./pageLink";

const ID = "550e8400-e29b-41d4-a716-446655440000";

function save(markdown: string, titles: Record<string, string> = {}) {
  const editor = new Editor({
    extensions: createMarkdownExtensions().map((extension) =>
      extension.name === "pageLink"
        ? PageLink.configure({ currentTitle: (id) => titles[id] })
        : extension,
    ),
    content: markdown,
    contentType: "markdown",
  });
  try {
    return { markdown: docToMarkdown(editor), json: editor.getJSON() as JSONContent };
  } finally {
    editor.destroy();
  }
}

describe("page links", () => {
  it("become nodes with the target ID, label and anchor", () => {
    const { json } = save(`See [Old \\[draft\\]](diaspar://page/${ID.toUpperCase()}#intro).`);
    const link = json.content?.[0]?.content?.find((n) => n.type === "pageLink");
    expect(link?.attrs).toEqual({ id: ID, label: "Old [draft]", anchor: "#intro" });
  });

  it("refresh their label to the current title when saved", () => {
    const { markdown } = save(`See [Old](diaspar://page/${ID}#intro).`, { [ID]: "New *title*" });
    expect(markdown).toBe(`See [New \\*title\\*](diaspar://page/${ID}#intro).\n`);
    // …and that text reads back as the same title.
    expect(save(markdown).json.content?.[0]?.content?.[1]?.attrs?.label).toBe("New *title*");
  });

  it("keep their label when the title isn't known", () => {
    expect(save(`[Kept](diaspar://page/${ID})`).markdown).toBe(`[Kept](diaspar://page/${ID})\n`);
  });

  it("leave other links and wikilinks alone", () => {
    const input = `[web](https://example.com) [[Some Page|alias]] [bad](diaspar://page/nope)\n`;
    expect(save(input, { [ID]: "X" }).markdown).toBe(input);
  });
});
