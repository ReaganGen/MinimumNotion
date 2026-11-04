import { Editor } from "@tiptap/core";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { resetLinkStoreForTests, useLinkStore } from "@/features/links/linkStore";
import {
  resetWorkspaceStoreForTests,
  useWorkspaceStore,
} from "@/features/workspace/workspaceStore";
import { ipc } from "@/lib/ipc/client";
import { answerBookmarkDialog, useBookmarkDialog } from "./bookmarkDialog";
import { createMarkdownExtensions } from "./extensions";
import { docToMarkdown } from "./markdown";
import { setAttachmentHost } from "./media";
import { insertBookmark, insertChildPage, SLASH_ITEMS, SLASH_SECTIONS } from "./slashCommands";
import { sectionGroups } from "./suggestionMenu";

vi.mock("@/lib/ipc/client", () => ({
  ipc: {
    createPage: vi.fn(),
    listChildren: vi.fn(async () => []),
    listFavorites: vi.fn(async () => []),
    resolvePages: vi.fn(async () => []),
  },
  assetUrl: () => "",
}));

const PARENT = "11111111-1111-4111-8111-111111111111";
const CHILD = "22222222-2222-4222-8222-222222222222";

let editor: Editor | null = null;
beforeEach(() => {
  resetWorkspaceStoreForTests();
  resetLinkStoreForTests();
  vi.mocked(ipc.createPage).mockReset();
});
afterEach(() => {
  editor?.destroy();
  editor = null;
});

/** An editor holding `text`, with the "/" just typed at its end. */
function typedSlash(text: string) {
  editor = new Editor({
    extensions: createMarkdownExtensions(),
    content: `${text}/`,
    contentType: "markdown",
  });
  const end = editor.state.doc.content.size - 1;
  // The cursor sits after the "/", as it does while typing.
  editor.commands.setTextSelection(end);
  return { editor, range: { from: end - 1, to: end } };
}

const item = (title: string) => {
  const found = SLASH_ITEMS.find((i) => i.title === title);
  if (!found) throw new Error(`no item ${title}`);
  return found;
};

describe("the / menu (ADR-048, ADR-049)", () => {
  it("keeps its items in section order, and shows only sections that work", () => {
    const groups = sectionGroups(SLASH_ITEMS, (i) => i.section);
    // Database arrives with Phase 10.
    expect(groups.map((g) => g.name)).toEqual(["Basic", "Media", "Embed"]);
    expect(SLASH_SECTIONS).toEqual(["Basic", "Media", "Embed", "Database"]);
    expect(groups.flatMap((g) => g.items.map((i) => i.index))).toEqual(
      SLASH_ITEMS.map((_, i) => i),
    );
    expect(SLASH_ITEMS.map((i) => i.title)).toEqual(
      expect.arrayContaining(["Page", "Callout", "Table", "Math block", "Inline math"]),
    );
    expect(groups[1]?.items.map((i) => i.item.title)).toEqual(["Image", "Audio", "Video", "File"]);
    expect(groups[2]?.items.map((i) => i.item.title)).toEqual(["Bookmark", "HTML"]);
  });

  it("HTML inserts an empty embed", () => {
    const { editor, range } = typedSlash("");
    item("HTML").run(editor, range);
    expect(editor.getJSON().content?.[0]?.type).toBe("htmlEmbed");
    expect(docToMarkdown(editor)).toBe("```html embed\n\n```\n");
  });

  it("Media items remove the / and ask the page editor for a file", async () => {
    const { editor, range } = typedSlash("Here ");
    const host = vi.fn(async () => undefined);
    const stop = setAttachmentHost(editor, host);
    item("Video").run(editor, range);
    await vi.waitFor(() => expect(host).toHaveBeenCalledWith("video"));
    expect(docToMarkdown(editor)).toBe("Here\n");
    stop();
  });

  it("Bookmark asks for the details, then inserts the card", async () => {
    const { editor, range } = typedSlash("");
    const inserted = insertBookmark(editor, range);
    await vi.waitFor(() => expect(useBookmarkDialog.getState().request).not.toBeNull());
    answerBookmarkDialog({ url: "example.com/notes", title: "", description: 'Say "hi"' });
    await inserted;
    expect(docToMarkdown(editor)).toBe(
      '[example.com/notes](https://example.com/notes "Say \\"hi\\"")\n',
    );
    expect(editor.getJSON().content?.[0]?.content?.[0]?.type).toBe("bookmark");
  });

  it("Bookmark inserts nothing when cancelled", async () => {
    const { editor, range } = typedSlash("Text ");
    const inserted = insertBookmark(editor, range);
    await vi.waitFor(() => expect(useBookmarkDialog.getState().request).not.toBeNull());
    answerBookmarkDialog(null);
    await inserted;
    expect(docToMarkdown(editor)).toBe("Text\n");
  });

  it("groups consecutive items under one heading, keeping global indexes", () => {
    const groups = sectionGroups(["a1", "a2", "b1", "a3"], (s) => s.charAt(0));
    expect(groups.map((g) => [g.name, g.items.map((i) => i.index)])).toEqual([
      ["a", [0, 1]],
      ["b", [2]],
      ["a", [3]],
    ]);
  });

  it("Callout wraps the line in a Note callout", () => {
    const { editor, range } = typedSlash("Remember this ");
    item("Callout").run(editor, range);
    expect(docToMarkdown(editor)).toBe("> [!NOTE]\n> Remember this\n");
  });

  it("Inline math inserts a formula", () => {
    const { editor, range } = typedSlash("Energy ");
    item("Inline math").run(editor, range);
    expect(docToMarkdown(editor)).toBe("Energy $x$\n");
  });

  it("Page makes a child page, links it and opens it", async () => {
    useWorkspaceStore.setState({ status: "open", activePageId: PARENT });
    vi.mocked(ipc.createPage).mockResolvedValue({
      id: CHILD,
      title: "Untitled",
      icon: null,
      kind: "leaf",
      has_children: false,
      favorite: false,
      status: { state: "ok" },
      is_collection: false,
    });
    const { editor, range } = typedSlash("See ");
    await insertChildPage(editor, range);
    expect(ipc.createPage).toHaveBeenCalledWith(PARENT, "Untitled");
    expect(docToMarkdown(editor)).toBe(`See [Untitled](diaspar://page/${CHILD})\n`);
    expect(useLinkStore.getState().pages[CHILD]?.title).toBe("Untitled");
    const state = useWorkspaceStore.getState();
    expect([state.activePageId, state.focusTitleOf, state.expanded[PARENT]]).toEqual([
      CHILD,
      CHILD,
      true,
    ]);
  });

  it("Page inserts nothing when the page can't be made", async () => {
    useWorkspaceStore.setState({ status: "open", activePageId: PARENT });
    vi.mocked(ipc.createPage).mockRejectedValue(new Error("Read-only workspace"));
    const { editor, range } = typedSlash("See ");
    await insertChildPage(editor, range);
    expect(docToMarkdown(editor)).toBe("See /\n");
    expect(useWorkspaceStore.getState().notices).toEqual(["Read-only workspace"]);
    expect(useWorkspaceStore.getState().activePageId).toBe(PARENT);
  });
});
