// The "/" menu: a short list of blocks, filtered as you type (spec §7).
import { type Editor, Extension, type Range } from "@tiptap/core";
import Suggestion from "@tiptap/suggestion";
import { suggestionPopup } from "./suggestionMenu";

interface SlashItem {
  title: string;
  hint: string;
  keywords: string;
  run: (editor: Editor, range: Range) => void;
}

const chain = (editor: Editor, range: Range) => editor.chain().focus().deleteRange(range);

export const SLASH_ITEMS: SlashItem[] = [
  {
    title: "Text",
    hint: "Plain paragraph",
    keywords: "paragraph p",
    run: (e, r) => chain(e, r).setParagraph().run(),
  },
  {
    title: "Heading 1",
    hint: "#",
    keywords: "h1 title",
    run: (e, r) => chain(e, r).setHeading({ level: 1 }).run(),
  },
  {
    title: "Heading 2",
    hint: "##",
    keywords: "h2 subtitle",
    run: (e, r) => chain(e, r).setHeading({ level: 2 }).run(),
  },
  {
    title: "Heading 3",
    hint: "###",
    keywords: "h3",
    run: (e, r) => chain(e, r).setHeading({ level: 3 }).run(),
  },
  {
    title: "Bulleted list",
    hint: "-",
    keywords: "ul unordered",
    run: (e, r) => chain(e, r).toggleBulletList().run(),
  },
  {
    title: "Numbered list",
    hint: "1.",
    keywords: "ol ordered",
    run: (e, r) => chain(e, r).toggleOrderedList().run(),
  },
  {
    title: "To-do list",
    hint: "[ ]",
    keywords: "todo task checkbox check",
    run: (e, r) => chain(e, r).toggleTaskList().run(),
  },
  {
    title: "Quote",
    hint: ">",
    keywords: "blockquote",
    run: (e, r) => chain(e, r).toggleBlockquote().run(),
  },
  {
    title: "Code block",
    hint: "```",
    keywords: "code pre",
    run: (e, r) => chain(e, r).toggleCodeBlock().run(),
  },
  {
    title: "Divider",
    hint: "---",
    keywords: "hr rule line",
    run: (e, r) => chain(e, r).setHorizontalRule().run(),
  },
  {
    title: "Table",
    hint: "3 × 3",
    keywords: "grid",
    run: (e, r) => chain(e, r).insertTable({ rows: 3, cols: 3, withHeaderRow: true }).run(),
  },
  {
    title: "Math block",
    hint: "$$",
    keywords: "equation latex katex formula",
    run: (e, r) => chain(e, r).insertBlockMath({ latex: "x^2" }).run(),
  },
];

export function filterSlashItems(query: string): SlashItem[] {
  const q = query.trim().toLowerCase();
  if (!q) return SLASH_ITEMS;
  return SLASH_ITEMS.filter((item) => `${item.title} ${item.keywords}`.toLowerCase().includes(q));
}

export const SlashCommands = Extension.create({
  name: "slashCommands",
  addProseMirrorPlugins() {
    return [
      Suggestion<SlashItem>({
        editor: this.editor,
        char: "/",
        // Only at the start of a line or after a space, and never inside code.
        allowedPrefixes: [" "],
        allow: ({ state, range }) => {
          const $from = state.doc.resolve(range.from);
          return !$from.parent.type.spec.code;
        },
        items: ({ query }) => filterSlashItems(query),
        command: ({ editor, range, props }) => props.run(editor, range),
        render: suggestionPopup<SlashItem>({
          label: "Insert a block",
          empty: "No matching blocks",
          itemKey: (item) => item.title,
          renderItem: (item) => (
            <>
              <span className="flex-1">{item.title}</span>
              <span className="font-mono text-[11.5px] text-faint-foreground">{item.hint}</span>
            </>
          ),
        }),
      }),
    ];
  },
});
