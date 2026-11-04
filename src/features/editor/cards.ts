// File cards and bookmarks (ADR-049, DATA_MODEL §5.2). Both are ordinary
// links alone in a paragraph, so other apps show a link:
//
//   [Report.pdf](_assets/Report.pdf)                      a file card
//   [Diaspar](https://example.com "A short description")  a bookmark
//
// Only a link into `_assets/`, or an http(s) link with a title, becomes a
// card; any other lone link stays a link. The label, destination and title
// are kept exactly as written. They are inline nodes because a paragraph is
// what holds them, in lists too; the live views are cardViews.tsx.
import { type JSONContent, Node } from "@tiptap/core";

const LABEL = String.raw`((?:\\.|[^\\[\]\n])*)`;
const FILE_LINK = new RegExp(String.raw`^\[${LABEL}\]\(((?:\./)?_assets/[^\s()<>]+)\)$`);
const BOOKMARK_LINK = new RegExp(
  String.raw`^\[${LABEL}\]\((https?://[^\s()<>]+) "((?:\\.|[^"\\\n])*)"\)$`,
  "i",
);

interface ParagraphToken {
  raw?: string;
  tokens?: { type?: string }[];
}

/** The card a paragraph holding only `token`'s link becomes, if any. */
export function loneLinkCard(token: ParagraphToken): JSONContent | null {
  const inline = token.tokens ?? [];
  if (inline.length !== 1 || inline[0]?.type !== "link") return null;
  const text = (token.raw ?? "").trim();
  const file = FILE_LINK.exec(text);
  if (file) return { type: "attachment", attrs: { label: file[1], src: file[2] } };
  const bookmark = BOOKMARK_LINK.exec(text);
  if (bookmark) {
    return {
      type: "bookmark",
      attrs: { label: bookmark[1], href: bookmark[2], description: bookmark[3] },
    };
  }
  return null;
}

/**
 * Attributes read from pasted HTML, accepted only if they would be read back
 * from Markdown as the same kind of card; anything else stays plain text.
 */
function pastedCard(type: "attachment" | "bookmark", markdown: string) {
  const card = loneLinkCard({ raw: markdown, tokens: [{ type: "link" }] });
  return card?.type === type ? (card.attrs ?? false) : false;
}

/** Text as shown: Markdown backslash escapes removed. */
export function unescapeMarkdown(text: string): string {
  return text.replace(/\\([!-/:-@[-`{-~])/g, "$1");
}

/** A bookmark description as a link title: `\` and `"` escaped, one line. */
export function escapeLinkTitle(text: string): string {
  return text.replace(/\s+/g, " ").trim().replace(/[\\"]/g, "\\$&");
}

/**
 * The URL a bookmark stores: http or https only (a bare host gets https), in
 * a form that needs no escaping in a link destination. `null` if it isn't one.
 */
export function bookmarkUrl(input: string): string | null {
  const trimmed = input.trim();
  if (!trimmed) return null;
  const withScheme = /^[a-z][a-z0-9+.-]*:/i.test(trimmed) ? trimmed : `https://${trimmed}`;
  let url: URL;
  try {
    url = new URL(withScheme);
  } catch {
    return null;
  }
  if (url.protocol !== "https:" && url.protocol !== "http:") return null;
  if (!url.hostname) return null;
  return url.href.replace(/\(/g, "%28").replace(/\)/g, "%29");
}

export const Attachment = Node.create({
  name: "attachment",
  group: "inline",
  inline: true,
  atom: true,
  addAttributes() {
    return { label: { default: "" }, src: { default: "" } };
  },
  parseHTML() {
    return [
      {
        tag: "span[data-attachment]",
        getAttrs: (el) => {
          const { attachment = "", label = "" } = (el as HTMLElement).dataset;
          return pastedCard("attachment", `[${label}](${attachment})`);
        },
      },
    ];
  },
  renderHTML({ node }) {
    return [
      "span",
      { "data-attachment": node.attrs.src, "data-label": node.attrs.label },
      unescapeMarkdown(node.attrs.label),
    ];
  },
  renderText: ({ node }) => unescapeMarkdown(node.attrs.label),
  renderMarkdown: (node) => `[${node.attrs?.label ?? ""}](${node.attrs?.src ?? ""})`,
});

export const Bookmark = Node.create({
  name: "bookmark",
  group: "inline",
  inline: true,
  atom: true,
  addAttributes() {
    return { label: { default: "" }, href: { default: "" }, description: { default: "" } };
  },
  parseHTML() {
    return [
      {
        tag: "span[data-bookmark]",
        getAttrs: (el) => {
          const { bookmark = "", label = "", description = "" } = (el as HTMLElement).dataset;
          return pastedCard("bookmark", `[${label}](${bookmark} "${description}")`);
        },
      },
    ];
  },
  renderHTML({ node }) {
    return [
      "span",
      {
        "data-bookmark": node.attrs.href,
        "data-label": node.attrs.label,
        "data-description": node.attrs.description,
      },
      unescapeMarkdown(node.attrs.label) || node.attrs.href,
    ];
  },
  renderText: ({ node }) => unescapeMarkdown(node.attrs.label) || node.attrs.href,
  renderMarkdown: (node) =>
    `[${node.attrs?.label ?? ""}](${node.attrs?.href ?? ""} "${node.attrs?.description ?? ""}")`,
});
