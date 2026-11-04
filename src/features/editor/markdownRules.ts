// Diaspar's Markdown serialization rules (DATA_MODEL §5.2–5.3), layered on
// @tiptap/markdown where its defaults would change meaning or bytes:
//
// - Text is escaped minimally: a character is escaped only where leaving it
//   would change how that position parses. The stock encoder turned every
//   `&`, `<` and `>` into HTML entities and escaped all underscores.
// - Paragraph lines that would start a block (`# `, `1. `, `- `, `> `) are
//   escaped. The stock serializer didn't, so `\# text` came back as a heading.
// - Code fences grow longer than any fence inside the code. The stock
//   serializer always used ``` and corrupted code that contained a fence.
// - Hard breaks are written as `\` (trailing spaces are easy to lose).
// - Autolinks keep their original `<url>` or bare-URL form.
import type { JSONContent } from "@tiptap/core";
import { CodeBlock as BaseCodeBlock } from "@tiptap/extension-code-block";
import { HardBreak as BaseHardBreak } from "@tiptap/extension-hard-break";
import { Link as BaseLink } from "@tiptap/extension-link";
import { Paragraph as BaseParagraph } from "@tiptap/extension-paragraph";
import { Table as BaseTable } from "@tiptap/extension-table";
import { MarkdownManager } from "@tiptap/markdown";
import { INLINE_MATH } from "./math";

const PUNCTUATION = /[!-/:-@[-`{-~]/;
const WORD = /[\p{L}\p{N}]/u;

function isSpace(char: string | undefined): boolean {
  return char === undefined || /\s/.test(char);
}

/**
 * Escapes `text` (one text run, outside code) just enough that it parses back
 * as the same literal text. Emphasis characters are left alone when spaces on
 * both sides mean they can't open or close emphasis.
 */
export function escapeInlineText(text: string): string {
  let out = "";
  for (let i = 0; i < text.length; i++) {
    const char = text.charAt(i);
    const prev = i > 0 ? text.charAt(i - 1) : undefined;
    const next = i + 1 < text.length ? text.charAt(i + 1) : undefined;
    let escaped = false;
    switch (char) {
      case "\\":
        escaped = next === undefined || PUNCTUATION.test(next);
        break;
      case "`":
      case "[":
      case "]":
        escaped = true;
        break;
      case "*":
      case "~":
        escaped = !(isSpace(prev) && isSpace(next) && prev !== undefined && next !== undefined);
        break;
      case "_": {
        const intraword =
          prev !== undefined && next !== undefined && WORD.test(prev) && WORD.test(next);
        const spaced = prev !== undefined && next !== undefined && isSpace(prev) && isSpace(next);
        escaped = !intraword && !spaced;
        break;
      }
      case "<":
        escaped = next !== undefined && /[A-Za-z/!?]/.test(next);
        break;
      case "&":
        escaped = /^&(?:#\d{1,7}|#[xX][0-9a-fA-F]{1,6}|[A-Za-z][A-Za-z0-9]{1,31});/.test(
          text.slice(i),
        );
        break;
      case "$":
        escaped = INLINE_MATH.test(text.slice(i));
        break;
    }
    out += escaped ? `\\${char}` : char;
  }
  return out;
}

/** Escapes the start of a paragraph line that would otherwise begin a block. */
export function escapeLineStart(line: string): string {
  return line
    .replace(/^(#{1,6})(?=\s|$)/, "\\$1")
    .replace(/^(\d{1,9})([.)])(?=\s|$)/, "$1\\$2")
    .replace(/^([-+])(?=\s|$)/, "\\$1")
    .replace(/^>/, "\\>")
    .replace(/^(=+|-+)(\s*)$/, "\\$1$2");
}

// @tiptap/markdown has no option for text escaping, so its private encoder
// is replaced here. The round-trip fixtures and `textEncoderIsPatched` (tested)
// fail loudly if a library update renames it.
type EncoderHost = {
  codeTypes: Set<string>;
  encodeTextForMarkdown?: (text: string, node: JSONContent, parent?: JSONContent) => string;
};
const managerPrototype = MarkdownManager.prototype as unknown as EncoderHost;
const hadEncoder = typeof managerPrototype.encodeTextForMarkdown === "function";
managerPrototype.encodeTextForMarkdown = function (this: EncoderHost, text, node, parent) {
  const inCode =
    (parent?.type != null && this.codeTypes.has(parent.type)) ||
    (node.marks ?? []).some((m) => this.codeTypes.has(typeof m === "string" ? m : m.type));
  if (inCode) return text;
  // A URL-only autolink is written as <url> or a bare URL, where backslash
  // escapes would become part of the URL. `sanitizeAutolinks` guarantees the
  // text still equals the href before this runs.
  const link = (node.marks ?? []).find((m) => typeof m !== "string" && m.type === "link");
  if (link && typeof link !== "string" && link.attrs?.autolink && text === link.attrs.href) {
    return text;
  }
  return escapeInlineText(text);
};
export const textEncoderIsPatched = hadEncoder;

export const Paragraph = BaseParagraph.extend({
  renderMarkdown(node, helpers, ctx) {
    const rendered = this.parent?.(node, helpers, ctx) ?? "";
    return rendered.split("\n").map(escapeLineStart).join("\n");
  },
});

export const HardBreak = BaseHardBreak.extend({
  renderMarkdown: () => "\\\n",
});

export const CodeBlock = BaseCodeBlock.extend({
  renderMarkdown: (node) => {
    const code = (node.content ?? []).map((child: JSONContent) => child.text ?? "").join("");
    const longestFence = Math.max(
      0,
      ...(code.match(/^`{3,}/gm) ?? []).map((fence) => fence.length),
    );
    const fence = "`".repeat(Math.max(3, longestFence + 1));
    return `${fence}${node.attrs?.language ?? ""}\n${code}\n${fence}`;
  },
});

export const Link = BaseLink.extend({
  addAttributes() {
    return {
      ...this.parent?.(),
      // How a URL-only link was written: "angle" for <url>, "bare" for a GFM bare URL.
      autolink: { default: null, rendered: false },
    };
  },
  parseMarkdown(token, helpers) {
    const raw = token.raw ?? "";
    const autolink = raw.startsWith("<") ? "angle" : raw === token.href ? "bare" : null;
    return helpers.applyMark("link", helpers.parseInline(token.tokens ?? []), {
      href: token.href,
      title: token.title ?? null,
      autolink,
    });
  },
  // Marks are rendered around a placeholder, so this can't see the link text.
  // `sanitizeAutolinks` clears `autolink` whenever the text no longer equals
  // the href, which makes the attribute alone a safe signal here.
  renderMarkdown(node, helpers, ctx) {
    if (node.attrs?.autolink === "angle") return `<${helpers.renderChildren(node)}>`;
    if (node.attrs?.autolink === "bare") return helpers.renderChildren(node);
    return this.parent?.(node, helpers, ctx) ?? "";
  },
});

type Mark = NonNullable<JSONContent["marks"]>[number];

function autolinkMark(node: JSONContent): Mark | undefined {
  return node.marks?.find((m) => m.type === "link" && m.attrs?.autolink);
}

/**
 * Returns a copy of `doc` in which a link keeps its `autolink` attribute only
 * while it is a single run of plain text equal to its href. Editing the text
 * of a pasted URL, or making part of it bold, turns it back into a normal
 * `[text](href)` link.
 */
export function sanitizeAutolinks(doc: JSONContent): JSONContent {
  const content = doc.content?.map(sanitizeAutolinks);
  if (!content) return doc;
  const cleaned = content.map((child, i) => {
    const mark = child.type === "text" ? autolinkMark(child) : undefined;
    if (!mark) return child;
    const neighbourShares = (other: JSONContent | undefined) =>
      other?.marks?.some((m) => m.type === "link" && m.attrs?.href === mark.attrs?.href) ?? false;
    const intact =
      child.text === mark.attrs?.href &&
      child.marks?.length === 1 &&
      !neighbourShares(content[i - 1]) &&
      !neighbourShares(content[i + 1]);
    if (intact) return child;
    return {
      ...child,
      marks: child.marks?.map((m) =>
        m === mark ? { ...m, attrs: { ...m.attrs, autolink: null } } : m,
      ),
    };
  });
  return { ...doc, content: cleaned };
}

export const Table = BaseTable.extend({
  renderMarkdown(node, helpers, ctx) {
    const table = (this.parent?.(node, helpers, ctx) ?? "").replace(/^\n+|\n+$/g, "");
    return alignDelimiterRow(table);
  },
});

/**
 * The stock serializer adds alignment colons on top of the column width, so a
 * delimiter row like `| :---- |` came out wider than the cells above it. This
 * rebuilds that row at the width the header cells were padded to.
 */
export function alignDelimiterRow(table: string): string {
  const lines = table.split("\n");
  const [header, delimiter] = lines;
  if (header === undefined || delimiter === undefined) return table;
  const widths = rowCells(header).map((cell) => cell.length);
  const aligned = rowCells(delimiter).map((cell, i) => {
    const marker = cell.trim();
    const left = marker.startsWith(":");
    const right = marker.endsWith(":");
    const dashes = Math.max(1, (widths[i] ?? 3) - Number(left) - Number(right));
    return `${left ? ":" : ""}${"-".repeat(dashes)}${right ? ":" : ""}`;
  });
  lines[1] = `| ${aligned.join(" | ")} |`;
  return lines.join("\n");
}

/** A row's cells with their padding but without the one space beside each pipe. */
function rowCells(row: string): string[] {
  return row
    .trim()
    .replace(/^\|/, "")
    .replace(/(?<!\\)\|$/, "")
    .split(/(?<!\\)\|/)
    .map((cell) => cell.replace(/^ /, "").replace(/ $/, ""));
}
