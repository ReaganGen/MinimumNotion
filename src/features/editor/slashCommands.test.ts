import { Editor } from "@tiptap/core";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { resetLinkStoreForTests, useLinkStore } from "@/features/links/linkStore";
import {
  resetWorkspaceStoreForTests,
  useWorkspaceStore,
} from "@/features/workspace/workspaceStore";
import { ipc } from "@/lib/ipc/client";
import { createMarkdownExtensions } from "./extensions";
import { docToMarkdown } from "./markdown";
import { insertChildPage, SLASH_ITEMS, SLASH_SECTIONS } from "./slashCommands";
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

describe("the / menu (ADR-048)", () => {
  it("keeps its items in section order, and shows only sections that work", () => {
    const groups = sectionGroups(SLASH_ITEMS, (i) => i.section);
    expect(groups.map((g) => g.name)).toEqual(["Basic"]);
    expect(SLASH_SECTIONS).toEqual(["Basic", "Media", "Embed", "Database"]);
    expect(groups[0]?.items.map((i) => i.index)).toEqual(SLASH_ITEMS.map((_, i) => i));
    expect(SLASH_ITEMS.map((i) => i.title)).toEqual(
      expect.arrayContaining(["Page", "Callout", "Table", "Math block", "Inline math"]),
    );
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
