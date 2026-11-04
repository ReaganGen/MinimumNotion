// The "/" menu: blocks in sections, filtered as you type (spec §7, ADR-048,
// ADR-049). A section appears only once it has items that work (design
// rule 8), so the Database section comes with Phase 10.
import { type Editor, Extension, type Range } from "@tiptap/core";
import Suggestion from "@tiptap/suggestion";
import { useLinkStore } from "@/features/links/linkStore";
import { flushAllEdits, useWorkspaceStore } from "@/features/workspace/workspaceStore";
import { ipc } from "@/lib/ipc/client";
import { bookmarkAttrs, editBookmark } from "./bookmarkDialog";
import { type AttachmentKind, pickAttachment } from "./media";
import { suggestionPopup } from "./suggestionMenu";

/** The menu's sections, in order. */
export const SLASH_SECTIONS = ["Basic", "Media", "Embed", "Database"] as const;

interface SlashItem {
  title: string;
  section: (typeof SLASH_SECTIONS)[number];
  hint: string;
  keywords: string;
  run: (editor: Editor, range: Range) => void;
}

const chain = (editor: Editor, range: Range) => editor.chain().focus().deleteRange(range);

export const SLASH_ITEMS: SlashItem[] = [
  {
    section: "Basic",
    title: "Text",
    hint: "Plain paragraph",
    keywords: "paragraph p",
    run: (e, r) => chain(e, r).setParagraph().run(),
  },
  {
    section: "Basic",
    title: "Page",
    hint: "Inside this page",
    keywords: "subpage child new link",
    run: (e, r) => void insertChildPage(e, r),
  },
  {
    section: "Basic",
    title: "Heading 1",
    hint: "#",
    keywords: "h1 title",
    run: (e, r) => chain(e, r).setHeading({ level: 1 }).run(),
  },
  {
    section: "Basic",
    title: "Heading 2",
    hint: "##",
    keywords: "h2 subtitle",
    run: (e, r) => chain(e, r).setHeading({ level: 2 }).run(),
  },
  {
    section: "Basic",
    title: "Heading 3",
    hint: "###",
    keywords: "h3",
    run: (e, r) => chain(e, r).setHeading({ level: 3 }).run(),
  },
  {
    section: "Basic",
    title: "Bulleted list",
    hint: "-",
    keywords: "ul unordered",
    run: (e, r) => chain(e, r).toggleBulletList().run(),
  },
  {
    section: "Basic",
    title: "Numbered list",
    hint: "1.",
    keywords: "ol ordered",
    run: (e, r) => chain(e, r).toggleOrderedList().run(),
  },
  {
    section: "Basic",
    title: "To-do list",
    hint: "[ ]",
    keywords: "todo task checkbox check",
    run: (e, r) => chain(e, r).toggleTaskList().run(),
  },
  {
    section: "Basic",
    title: "Quote",
    hint: ">",
    keywords: "blockquote",
    run: (e, r) => chain(e, r).toggleBlockquote().run(),
  },
  {
    section: "Basic",
    title: "Callout",
    hint: "> [!NOTE]",
    keywords: "note tip warning alert admonition box",
    run: (e, r) => chain(e, r).wrapIn("callout", { marker: "NOTE", fold: "", title: "" }).run(),
  },
  {
    section: "Basic",
    title: "Code block",
    hint: "```",
    keywords: "code pre",
    run: (e, r) => chain(e, r).toggleCodeBlock().run(),
  },
  {
    section: "Basic",
    title: "Divider",
    hint: "---",
    keywords: "hr rule line",
    run: (e, r) => chain(e, r).setHorizontalRule().run(),
  },
  {
    section: "Basic",
    title: "Table",
    hint: "3 × 3",
    keywords: "grid",
    run: (e, r) => chain(e, r).insertTable({ rows: 3, cols: 3, withHeaderRow: true }).run(),
  },
  {
    section: "Basic",
    title: "Math block",
    hint: "$$",
    keywords: "equation latex katex formula",
    run: (e, r) => chain(e, r).insertBlockMath({ latex: "x^2" }).run(),
  },
  {
    section: "Basic",
    title: "Inline math",
    hint: "$…$",
    keywords: "equation latex katex formula",
    // In a chain, the command reads the selection from before the "/" was
    // deleted, so the position is passed.
    run: (e, r) => chain(e, r).insertInlineMath({ latex: "x", pos: r.from }).run(),
  },
  {
    section: "Media",
    title: "Image",
    hint: "![…](…)",
    keywords: "picture photo png jpg gif",
    run: (e, r) => void insertPicked(e, r, "image"),
  },
  {
    section: "Media",
    title: "Audio",
    hint: ".mp3 .m4a",
    keywords: "sound music recording voice",
    run: (e, r) => void insertPicked(e, r, "audio"),
  },
  {
    section: "Media",
    title: "Video",
    hint: ".mp4 .mov",
    keywords: "movie clip recording",
    run: (e, r) => void insertPicked(e, r, "video"),
  },
  {
    section: "Media",
    title: "File",
    hint: "Any file",
    keywords: "attachment pdf document upload",
    run: (e, r) => void insertPicked(e, r, "file"),
  },
  {
    section: "Embed",
    title: "Bookmark",
    hint: "Link card",
    keywords: "link url web website card",
    run: (e, r) => void insertBookmark(e, r),
  },
  {
    section: "Embed",
    title: "HTML",
    hint: "Sandboxed",
    keywords: "embed iframe static code",
    run: (e, r) =>
      chain(e, r)
        .insertContent({ type: "htmlEmbed", attrs: { info: "html embed", code: "" } })
        .run(),
  },
];

/** Removes the typed "/" text, then lets the user pick a file to insert. */
async function insertPicked(editor: Editor, range: Range, kind: AttachmentKind) {
  chain(editor, range).run();
  await pickAttachment(editor, kind);
}

/** Asks for the bookmark's address, title and description, then inserts it. */
export async function insertBookmark(editor: Editor, range: Range) {
  chain(editor, range).run();
  const fields = await editBookmark({ url: "", title: "", description: "" });
  const attrs = fields && bookmarkAttrs(fields);
  if (!attrs || editor.isDestroyed) return;
  editor
    .chain()
    .focus()
    .insertContent({ type: "paragraph", content: [{ type: "bookmark", attrs }] })
    .run();
}

/**
 * Makes an empty child page of the open page, links it where "/" was typed,
 * saves, then opens the new page with its title focused (ADR-048).
 */
export async function insertChildPage(editor: Editor, range: Range) {
  const store = useWorkspaceStore.getState();
  const parentId = store.activePageId;
  if (!parentId) return;
  let page: Awaited<ReturnType<typeof ipc.createPage>>;
  try {
    page = await ipc.createPage(parentId, "Untitled");
  } catch (error) {
    store.reportError(error);
    return;
  }
  useLinkStore.setState({
    pages: {
      ...useLinkStore.getState().pages,
      [page.id]: { id: page.id, title: page.title, icon: page.icon, state: "ok" },
    },
  });
  editor
    .chain()
    .focus()
    .insertContentAt(range, [
      { type: "pageLink", attrs: { id: page.id, label: page.title, anchor: "" } },
      { type: "text", text: " " },
    ])
    .run();
  await flushAllEdits();
  await store.expand(parentId);
  store.openPage(page.id);
  useWorkspaceStore.setState({ focusTitleOf: page.id });
}

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
          section: (item) => item.section,
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
