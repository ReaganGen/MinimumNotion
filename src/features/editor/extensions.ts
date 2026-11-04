// The editor's schema: every node and mark Diaspar supports, each with a
// Markdown parse and render rule (DATA_MODEL §5). The live editor and the
// round-trip tests use this same list, so what is tested is what ships.
import type { AnyExtension } from "@tiptap/core";
import { Image } from "@tiptap/extension-image";
import { TaskItem, TaskList } from "@tiptap/extension-list";
import { TableCell, TableHeader, TableRow } from "@tiptap/extension-table";
import { Markdown } from "@tiptap/markdown";
import { StarterKit } from "@tiptap/starter-kit";
import { Callout } from "./callout";
import { CodeBlock, HardBreak, Link, Paragraph, Table } from "./markdownRules";
import { BlockMath, InlineMath } from "./math";
import { PageLink, WikiLink } from "./pageLink";
import { FootnoteDefinition, FootnoteReference, RawBlock, RawInline } from "./rawBlock";

export function createMarkdownExtensions(): AnyExtension[] {
  return [
    StarterKit.configure({
      // Markdown has no underline, so ⌘U must not create unsaveable content.
      underline: false,
      heading: { levels: [1, 2, 3, 4, 5, 6] },
      trailingNode: false,
      // Replaced by the versions with Diaspar's Markdown rules below.
      paragraph: false,
      hardBreak: false,
      codeBlock: false,
      link: false,
    }),
    Paragraph,
    HardBreak,
    CodeBlock,
    Link.configure({ openOnClick: false, autolink: true, linkOnPaste: true }),
    TaskList,
    TaskItem.configure({ nested: true }),
    Table.configure({ resizable: false }),
    TableRow,
    TableHeader,
    TableCell,
    Image.configure({ inline: true }),
    InlineMath.configure({ katexOptions: { throwOnError: false, trust: false } }),
    BlockMath.configure({ katexOptions: { throwOnError: false, trust: false } }),
    Callout,
    RawBlock,
    RawInline,
    FootnoteDefinition,
    FootnoteReference,
    PageLink,
    WikiLink,
    Markdown.configure({ indentation: { style: "space", size: 2 } }),
  ];
}
