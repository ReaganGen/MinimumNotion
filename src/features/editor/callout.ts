// Callouts (ADR-048, DATA_MODEL §5.2): a blockquote that starts with
// `> [!TYPE]`, as GitHub alerts and Obsidian callouts write it. The first line
// is kept exactly as read (case, a `+`/`-` fold marker, a title), so files
// from those apps come back byte for byte. The live view is calloutView.tsx.
import { Node } from "@tiptap/core";

export interface CalloutKind {
  id: string;
  label: string;
  /** The tag palette the callout's fill and icon use (tokens.css). */
  tone: "blue" | "green" | "purple" | "yellow" | "red" | "gray";
}

/** The types the "/" menu and the type menu offer, in order. */
export const CALLOUT_KINDS: CalloutKind[] = [
  { id: "note", label: "Note", tone: "blue" },
  { id: "tip", label: "Tip", tone: "green" },
  { id: "important", label: "Important", tone: "purple" },
  { id: "warning", label: "Warning", tone: "yellow" },
  { id: "caution", label: "Caution", tone: "red" },
];

/** The look for a marker; any other word is a neutral callout labelled with it. */
export function calloutKind(marker: string): CalloutKind {
  const id = marker.toLowerCase();
  return (
    CALLOUT_KINDS.find((k) => k.id === id) ?? { id: "other", label: marker || "Note", tone: "gray" }
  );
}

/** `> [!TYPE]`, then an optional fold marker and title, on the first line. */
const CALLOUT_START = /^ {0,3}> ?\[!([A-Za-z][\w-]*)\]([+-]?)([^\n]*)(?:\n|$)/;
/** A following quote line; the `>` and one space are stripped. */
const QUOTE_LINE = /^ {0,3}> ?([^\n]*)(?:\n|$)/;

export const Callout = Node.create({
  name: "callout",
  group: "block",
  content: "block+",
  defining: true,
  addAttributes() {
    return {
      marker: { default: "NOTE" },
      fold: { default: "" },
      title: { default: "" },
    };
  },
  parseHTML() {
    return [
      {
        tag: "div[data-callout]",
        getAttrs: (el) => ({
          marker: (el as HTMLElement).dataset.callout || "NOTE",
          fold: (el as HTMLElement).dataset.fold ?? "",
          title: (el as HTMLElement).dataset.title ?? "",
        }),
      },
    ];
  },
  renderHTML({ node }) {
    const kind = calloutKind(node.attrs.marker);
    return [
      "div",
      {
        class: "callout",
        "data-callout": node.attrs.marker,
        "data-fold": node.attrs.fold,
        "data-title": node.attrs.title,
        "data-tone": kind.tone,
      },
      0,
    ];
  },
  addKeyboardShortcuts() {
    return {
      // Backspace at the very start turns the first block back into plain text.
      Backspace: ({ editor }) => {
        const { $from, empty } = editor.state.selection;
        if (!empty || $from.parentOffset !== 0 || $from.depth < 2) return false;
        if ($from.node(-1).type.name !== this.name || $from.index(-1) !== 0) return false;
        return editor.commands.lift(this.name);
      },
    };
  },
  markdownTokenName: "callout",
  markdownTokenizer: {
    name: "callout",
    level: "block",
    start: (src: string) => src.search(/(?:^|\n) {0,3}> ?\[!/),
    tokenize: (src, _tokens, lexer) => {
      const first = CALLOUT_START.exec(src);
      if (!first) return undefined;
      let raw = first[0];
      let rest = src.slice(raw.length);
      const lines: string[] = [];
      for (let line = QUOTE_LINE.exec(rest); line && rest; line = QUOTE_LINE.exec(rest)) {
        lines.push(line[1] ?? "");
        raw += line[0];
        rest = rest.slice(line[0].length);
      }
      // A non-blank line right after an open paragraph would continue it
      // lazily inside the quote; leave that to the stock blockquote.
      const next = rest.split("\n", 1)[0] ?? "";
      const open = lines.length === 0 || (lines.at(-1) ?? "").trim() !== "";
      if (next.trim() !== "" && open && (lexer.isParagraphContinuation?.(next) ?? true)) {
        return undefined;
      }
      return {
        type: "callout",
        raw,
        marker: first[1],
        fold: first[2],
        title: first[3],
        tokens: lexer.blockTokens(lines.join("\n")),
      };
    },
  },
  parseMarkdown: (token, helpers) => {
    const parse = helpers.parseBlockChildren ?? helpers.parseChildren;
    const content = parse(token.tokens ?? []);
    return helpers.createNode(
      "callout",
      { marker: token.marker, fold: token.fold ?? "", title: token.title ?? "" },
      content.length > 0 ? content : [{ type: "paragraph" }],
    );
  },
  renderMarkdown: (node, h) => {
    const head = `> [!${node.attrs?.marker ?? "NOTE"}]${node.attrs?.fold ?? ""}${node.attrs?.title ?? ""}`;
    const parts = (node.content ?? []).map(
      (child, index) => h.renderChild?.(child, index) ?? h.renderChildren([child]),
    );
    if (parts.every((part) => part.trim() === "")) return head;
    const body = parts
      .map((part) =>
        part
          .split("\n")
          .map((line) => (line.trim() === "" ? ">" : `> ${line}`))
          .join("\n"),
      )
      .join("\n>\n");
    return `${head}\n${body}`;
  },
});
