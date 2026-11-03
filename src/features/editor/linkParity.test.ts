// The editor and the Rust index (crates/forma-core/src/extract.rs) must agree
// on which links a page has; both read tests/fixtures/links/.
import { Editor, type JSONContent } from "@tiptap/core";
import { describe, expect, it } from "vitest";
import { wikiTarget } from "@/features/links/linkStore";
import { createMarkdownExtensions } from "./extensions";

const files = import.meta.glob("/tests/fixtures/links/*", {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

interface LinkRef {
  kind: "page" | "wiki";
  target: string;
  label: string;
}

function linksOf(markdown: string): LinkRef[] {
  const editor = new Editor({
    extensions: createMarkdownExtensions(),
    content: markdown,
    contentType: "markdown",
  });
  const out: LinkRef[] = [];
  const walk = (node: JSONContent) => {
    if (node.type === "pageLink") {
      out.push({ kind: "page", target: node.attrs?.id, label: node.attrs?.label });
    } else if (node.type === "wikiLink") {
      const source: string = node.attrs?.source;
      const alias = source
        .replace(/^\[\[|\]\]$/g, "")
        .split("|")[1]
        ?.trim();
      const target = wikiTarget(source);
      out.push({ kind: "wiki", target, label: alias ?? target });
    }
    for (const child of node.content ?? []) walk(child);
  };
  walk(editor.getJSON() as JSONContent);
  editor.destroy();
  return out;
}

describe("link parity with the Rust index", () => {
  const inputs = Object.entries(files).filter(([path]) => path.endsWith(".md"));

  it("has fixtures", () => {
    expect(inputs.length).toBeGreaterThanOrEqual(2);
  });

  for (const [path, markdown] of inputs) {
    it(path.split("/").pop() as string, () => {
      const expected = JSON.parse(files[path.replace(/\.md$/, ".links.json")] as string);
      expect(linksOf(markdown)).toEqual(expected);
    });
  }
});
