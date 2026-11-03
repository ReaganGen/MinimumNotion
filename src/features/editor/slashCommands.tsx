// The "/" menu: a short list of blocks, filtered as you type (spec §7).
import { type Editor, Extension, type Range } from "@tiptap/core";
import { ReactRenderer } from "@tiptap/react";
import Suggestion, { type SuggestionKeyDownProps, type SuggestionProps } from "@tiptap/suggestion";
import { forwardRef, useImperativeHandle, useState } from "react";
import { cn } from "@/lib/utils";

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

interface MenuHandle {
  onKeyDown: (props: SuggestionKeyDownProps) => boolean;
}

const SlashMenuList = forwardRef<MenuHandle, SuggestionProps<SlashItem>>(
  function SlashMenuList(props, ref) {
    const [selected, setSelected] = useState(0);
    // A new query starts the selection over at the top.
    const [shownItems, setShownItems] = useState(props.items);
    if (shownItems !== props.items) {
      setShownItems(props.items);
      setSelected(0);
    }

    const choose = (index: number) => {
      const item = props.items[index];
      if (item) props.command(item);
    };

    useImperativeHandle(ref, () => ({
      onKeyDown: ({ event }) => {
        const count = props.items.length;
        if (count === 0) return false;
        if (event.key === "ArrowDown") {
          setSelected((s) => (s + 1) % count);
          return true;
        }
        if (event.key === "ArrowUp") {
          setSelected((s) => (s + count - 1) % count);
          return true;
        }
        if (event.key === "Enter") {
          choose(selected);
          return true;
        }
        return false;
      },
    }));

    if (props.items.length === 0) {
      return (
        <div className="forma-popover px-3 py-2 text-[13px] text-muted-foreground">
          No matching blocks
        </div>
      );
    }
    return (
      <div role="listbox" aria-label="Insert a block" className="forma-popover w-60 p-1">
        {props.items.map((item, index) => (
          <button
            key={item.title}
            type="button"
            role="option"
            aria-selected={index === selected}
            onMouseEnter={() => setSelected(index)}
            onMouseDown={(e) => {
              e.preventDefault();
              choose(index);
            }}
            className={cn(
              "flex h-8 w-full items-center justify-between rounded-md px-2 text-left text-[13.5px]",
              index === selected && "bg-accent",
            )}
          >
            <span>{item.title}</span>
            <span className="font-mono text-[11.5px] text-faint-foreground">{item.hint}</span>
          </button>
        ))}
      </div>
    );
  },
);

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
        render: () => {
          let renderer: ReactRenderer<MenuHandle, SuggestionProps<SlashItem>> | null = null;
          let host: HTMLDivElement | null = null;
          const place = (props: SuggestionProps<SlashItem>) => {
            const rect = props.clientRect?.();
            if (!host || !rect) return;
            host.style.left = `${rect.left}px`;
            host.style.top = `${rect.bottom + 6}px`;
          };
          return {
            onStart: (props) => {
              renderer = new ReactRenderer(SlashMenuList, { props, editor: props.editor });
              host = document.createElement("div");
              host.style.position = "fixed";
              host.style.zIndex = "60";
              host.append(renderer.element);
              document.body.append(host);
              place(props);
            },
            onUpdate: (props) => {
              renderer?.updateProps(props);
              place(props);
            },
            // Escape is handled by the suggestion plugin, which then calls onExit.
            onKeyDown: (props) => renderer?.ref?.onKeyDown(props) ?? false,
            onExit: () => {
              host?.remove();
              renderer?.destroy();
              host = null;
              renderer = null;
            },
          };
        },
      }),
    ];
  },
});
