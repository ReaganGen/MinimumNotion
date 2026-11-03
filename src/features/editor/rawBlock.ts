// Markdown that the editor can't represent is kept byte for byte
// (DATA_MODEL §5.2): raw HTML and footnotes become atom nodes
// that display their source and write it back unchanged. Raw HTML is shown as
// text and never rendered (ARCHITECTURE §2.1). Wikilinks are in pageLink.ts.
import { Extension, Node } from "@tiptap/core";

/** HTML block start: a tag, closing tag, comment, declaration or processing instruction at line start. */
const HTML_BLOCK =
  /^ {0,3}(?:<!--[\s\S]*?-->|<[?!][\s\S]*?>|<\/?[A-Za-z][A-Za-z0-9-]*(?:\s[^\n]*)?>)[^\n]*(?:\n(?!\s*\n)[^\n]*)*(?:\n|$)/;
/** Footnote definition `[^label]: text`, with indented continuation lines. */
const FOOTNOTE_DEFINITION = /^\[\^[^\]\s]+\]:[^\n]*(?:\n(?: {4}|\t)[^\n]*)*(?:\n|$)/;
/** Inline HTML tag or comment. `<br>` is left to the hard-break rule. */
const INLINE_HTML = /^(?:<!--[\s\S]*?-->|<\/?(?!br\b)[A-Za-z][A-Za-z0-9-]*(?:\s+[^<>]*?)?\/?>)/i;
/** Footnote reference `[^label]`. */
const FOOTNOTE_REFERENCE = /^\[\^[^\]\s]+\](?!:)/;

export const RawBlock = Node.create({
  name: "rawBlock",
  group: "block",
  atom: true,
  selectable: true,
  addAttributes() {
    return { source: { default: "" } };
  },
  parseHTML() {
    return [
      {
        tag: "pre[data-raw-block]",
        getAttrs: (el) => ({ source: (el as HTMLElement).textContent }),
      },
    ];
  },
  renderHTML({ node }) {
    return ["pre", { "data-raw-block": "", class: "raw-block" }, node.attrs.source];
  },
  markdownTokenName: "rawHtmlBlock",
  markdownTokenizer: {
    name: "rawHtmlBlock",
    level: "block",
    start: (src: string) => src.search(/(?:^|\n) {0,3}</),
    tokenize: (src: string) => {
      const match = HTML_BLOCK.exec(src);
      if (!match) return undefined;
      return { type: "rawHtmlBlock", raw: match[0], text: match[0] };
    },
  },
  parseMarkdown: (token) => ({
    type: "rawBlock",
    attrs: { source: (token.raw ?? "").replace(/\n+$/, "") },
  }),
  renderMarkdown: (node) => node.attrs?.source ?? "",
});

export const RawInline = Node.create({
  name: "rawInline",
  group: "inline",
  inline: true,
  atom: true,
  selectable: true,
  addAttributes() {
    return { source: { default: "" } };
  },
  parseHTML() {
    return [
      {
        tag: "code[data-raw-inline]",
        getAttrs: (el) => ({ source: (el as HTMLElement).textContent }),
      },
    ];
  },
  renderHTML({ node }) {
    return ["code", { "data-raw-inline": "", class: "raw-inline" }, node.attrs.source];
  },
  markdownTokenName: "rawInlineHtml",
  markdownTokenizer: {
    name: "rawInlineHtml",
    level: "inline",
    start: (src: string) => src.indexOf("<"),
    tokenize: (src: string) => {
      const match = INLINE_HTML.exec(src);
      if (!match) return undefined;
      return { type: "rawInlineHtml", raw: match[0], text: match[0] };
    },
  },
  parseMarkdown: (token) => rawInline(token.raw),
  renderMarkdown: (node) => node.attrs?.source ?? "",
});

function rawInline(source: string | undefined) {
  return { type: "rawInline", attrs: { source: source ?? "" } };
}

/** `[^label]: …` definitions, kept as raw blocks. */
export const FootnoteDefinition = Extension.create({
  name: "footnoteDefinition",
  markdownTokenName: "footnoteDefinition",
  markdownTokenizer: {
    name: "footnoteDefinition",
    level: "block",
    start: (src: string) => src.search(/(?:^|\n)\[\^/),
    tokenize: (src: string) => {
      const match = FOOTNOTE_DEFINITION.exec(src);
      if (!match) return undefined;
      return { type: "footnoteDefinition", raw: match[0], text: match[0] };
    },
  },
  parseMarkdown: (token) => ({
    type: "rawBlock",
    attrs: { source: (token.raw ?? "").replace(/\n+$/, "") },
  }),
});

/** `[^label]` references, kept as raw inline text. */
export const FootnoteReference = Extension.create({
  name: "footnoteReference",
  markdownTokenName: "footnoteReference",
  markdownTokenizer: {
    name: "footnoteReference",
    level: "inline",
    start: (src: string) => src.indexOf("[^"),
    tokenize: (src: string) => {
      const match = FOOTNOTE_REFERENCE.exec(src);
      if (!match) return undefined;
      return { type: "footnoteReference", raw: match[0], text: match[0] };
    },
  },
  parseMarkdown: (token) => rawInline(token.raw),
});
