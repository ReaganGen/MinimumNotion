// Page links `[Title](diaspar://page/<uuid>)` and wikilinks `[[Title]]` as
// editor nodes (ARCHITECTURE §6.5). A page link's label is refreshed to the
// target's current title when the page is saved; a wikilink is written back
// exactly as it was read.
import { Node } from "@tiptap/core";
import { escapeInlineText } from "./markdownRules";

const UUID = "[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}";
const PAGE_LINK = new RegExp(
  `^\\[((?:\\\\.|[^\\]\\\\\\n])*)\\]\\(diaspar://page/(${UUID})(#[^)\\s]*)?\\)`,
);
/** Wikilink `[[Target]]` or `[[Target|alias]]`; resolved by title (DATA_MODEL §5.4). */
const WIKILINK = /^\[\[[^[\]\n]+\]\]/;

const unescapeLabel = (label: string) => label.replace(/\\([!-/:-@[-`{-~])/g, "$1");

export interface PageLinkOptions {
  /** The target's current title, when known; otherwise the saved label is kept. */
  currentTitle: (id: string) => string | undefined;
}

export const PageLink = Node.create<PageLinkOptions>({
  name: "pageLink",
  group: "inline",
  inline: true,
  atom: true,
  selectable: true,
  addOptions() {
    return { currentTitle: () => undefined };
  },
  addAttributes() {
    return { id: { default: "" }, label: { default: "" }, anchor: { default: "" } };
  },
  parseHTML() {
    return [
      {
        tag: "a[data-page-link]",
        getAttrs: (el) => ({
          id: (el as HTMLElement).dataset.pageLink,
          label: (el as HTMLElement).textContent,
          anchor: (el as HTMLElement).dataset.anchor ?? "",
        }),
      },
    ];
  },
  renderHTML({ node }) {
    return [
      "a",
      { "data-page-link": node.attrs.id, "data-anchor": node.attrs.anchor, class: "page-link" },
      node.attrs.label,
    ];
  },
  markdownTokenName: "pageLink",
  markdownTokenizer: {
    name: "pageLink",
    level: "inline",
    start: (src: string) => src.search(/\[[^\]\n]*\]\(diaspar:\/\/page\//i),
    tokenize: (src: string) => {
      const match = PAGE_LINK.exec(src);
      if (!match) return undefined;
      return {
        type: "pageLink",
        raw: match[0],
        label: match[1] ?? "",
        id: (match[2] ?? "").toLowerCase(),
        anchor: match[3] ?? "",
      };
    },
  },
  parseMarkdown: (token) => ({
    type: "pageLink",
    attrs: {
      id: token.id as string,
      label: unescapeLabel(token.label as string),
      anchor: token.anchor as string,
    },
  }),
  renderMarkdown(node) {
    const id = node.attrs?.id as string;
    const label = this.options.currentTitle(id) ?? (node.attrs?.label as string) ?? "";
    return `[${escapeInlineText(label)}](diaspar://page/${id}${node.attrs?.anchor ?? ""})`;
  },
});

export const WikiLink = Node.create({
  name: "wikiLink",
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
        tag: "span[data-wikilink]",
        getAttrs: (el) => ({ source: (el as HTMLElement).textContent }),
      },
    ];
  },
  renderHTML({ node }) {
    return ["span", { "data-wikilink": "", class: "wiki-link" }, node.attrs.source];
  },
  markdownTokenName: "wikilink",
  markdownTokenizer: {
    name: "wikilink",
    level: "inline",
    start: (src: string) => src.indexOf("[["),
    tokenize: (src: string) => {
      const match = WIKILINK.exec(src);
      if (!match) return undefined;
      return { type: "wikilink", raw: match[0], text: match[0] };
    },
  },
  parseMarkdown: (token) => ({ type: "wikiLink", attrs: { source: token.raw ?? "" } }),
  renderMarkdown: (node) => node.attrs?.source ?? "",
});
