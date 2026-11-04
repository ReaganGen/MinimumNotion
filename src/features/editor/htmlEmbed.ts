// HTML embeds (ADR-049, DATA_MODEL §5.2): a fenced code block whose info
// string starts with `html embed`, so other apps show HTML code. An optional
// `height=N` in the info string is the frame height the user set. The live
// view (htmlEmbedView.tsx) shows the HTML in a sandboxed frame that runs no
// scripts and makes no requests; everywhere else it is code.
import { Node } from "@tiptap/core";
import { EMBED_INFO, fencedCode } from "./markdownRules";

export const DEFAULT_EMBED_HEIGHT = 240;
export const MIN_EMBED_HEIGHT = 60;
export const MAX_EMBED_HEIGHT = 2000;

const HEIGHT = /(^|\s)height=(\d+)(?=\s|$)/;

export function clampEmbedHeight(height: number): number {
  if (!Number.isFinite(height)) return DEFAULT_EMBED_HEIGHT;
  return Math.min(MAX_EMBED_HEIGHT, Math.max(MIN_EMBED_HEIGHT, Math.round(height)));
}

/** The frame height an info string asks for. */
export function embedHeight(info: string): number {
  const match = HEIGHT.exec(info);
  return match ? clampEmbedHeight(Number(match[2])) : DEFAULT_EMBED_HEIGHT;
}

/** `info` with its height set; the other words are kept as written. */
export function withEmbedHeight(info: string, height: number): string {
  const token = `height=${clampEmbedHeight(height)}`;
  return HEIGHT.test(info) ? info.replace(HEIGHT, `$1${token}`) : `${info} ${token}`;
}

/**
 * What the frame may load: nothing from the network, no scripts, no plugins,
 * no frames, no forms. Inline styles and `data:` images and fonts work.
 */
export const EMBED_CSP =
  "default-src 'none'; style-src 'unsafe-inline'; img-src data:; font-src data:; form-action 'none'";

/**
 * The document shown in the sandboxed frame. The policy comes first, so it
 * covers everything the embed contains; a second policy in the embed could
 * only add restrictions. Links open in a new window by default, which the
 * empty sandbox forbids, so clicking one goes nowhere; a link that targets
 * the frame itself is stopped by the app's `frame-src 'none'`.
 */
export function embedDocument(html: string): string {
  return [
    "<!doctype html><html><head>",
    '<meta charset="utf-8">',
    `<meta http-equiv="Content-Security-Policy" content="${EMBED_CSP}">`,
    '<meta http-equiv="x-dns-prefetch-control" content="off">',
    '<base target="_blank">',
    '<meta name="color-scheme" content="light dark">',
    "<style>body{margin:8px;font:14px/1.5 -apple-system,BlinkMacSystemFont,sans-serif}</style>",
    `</head><body>${html}</body></html>`,
  ].join("");
}

export const HtmlEmbed = Node.create({
  name: "htmlEmbed",
  group: "block",
  // One unit: the code is an attribute, edited in the view's code area, so
  // the cursor never lands in text that the preview hides.
  atom: true,
  addAttributes() {
    return { info: { default: "html embed" }, code: { default: "" } };
  },
  parseHTML() {
    return [
      {
        tag: "pre[data-html-embed]",
        // Ahead of the code block's `pre` rule, so a copied embed pastes as one.
        priority: 60,
        // Only Diaspar's own embeds; other code pastes as a code block.
        getAttrs: (el) => {
          const info = (el as HTMLElement).dataset.htmlEmbed ?? "";
          return EMBED_INFO.test(info) ? { info, code: el.textContent ?? "" } : false;
        },
      },
    ];
  },
  renderHTML({ node }) {
    return ["pre", { "data-html-embed": node.attrs.info }, ["code", {}, node.attrs.code]];
  },
  renderText: ({ node }) => node.attrs.code,
  renderMarkdown: (node) => fencedCode(node.attrs?.code ?? "", node.attrs?.info ?? "html embed"),
});
