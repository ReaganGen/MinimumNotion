import { Editor } from "@tiptap/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { resetLinkStoreForTests, useLinkStore } from "@/features/links/linkStore";
import {
  resetWorkspaceStoreForTests,
  useWorkspaceStore,
} from "@/features/workspace/workspaceStore";
import { ipc } from "@/lib/ipc/client";
import { createMarkdownExtensions } from "./extensions";
import { docToMarkdown } from "./markdown";
import { findLinkTargets, insertPageLink } from "./pageLinkSuggestion";

vi.mock("@/lib/ipc/client", () => ({
  ipc: { quickOpen: vi.fn(), ensurePersistedId: vi.fn(), resolvePages: vi.fn() },
  assetUrl: () => "",
}));

const TARGET = "77777777-7777-4777-8777-777777777777";
const hit = { id: TARGET, title: "Reading list", icon: null, path: [], snippet: null };

beforeEach(() => {
  resetWorkspaceStoreForTests();
  resetLinkStoreForTests();
  for (const fn of Object.values(ipc)) vi.mocked(fn).mockReset();
});

function editorWith(text: string) {
  return new Editor({
    extensions: createMarkdownExtensions(),
    content: text,
    contentType: "markdown",
  });
}

describe("[[ page links", () => {
  it("offers recent pages first, then title matches", async () => {
    useWorkspaceStore.getState().setBreadcrumbs([{ id: "r", title: "Recent one", icon: null }]);
    expect((await findLinkTargets("")).map((p) => p.title)).toEqual(["Recent one"]);
    vi.mocked(ipc.quickOpen).mockResolvedValue([hit]);
    expect(await findLinkTargets("read")).toEqual([hit]);
    expect(ipc.quickOpen).toHaveBeenCalledWith("read");
  });

  it("persists the target's ID, then replaces the typed text with a link", async () => {
    vi.mocked(ipc.ensurePersistedId).mockResolvedValue(undefined);
    const editor = editorWith("See [[read");
    const end = editor.state.doc.content.size - 1;
    await insertPageLink(editor, { from: end - 6, to: end }, hit);
    expect(ipc.ensurePersistedId).toHaveBeenCalledWith(TARGET);
    expect(docToMarkdown(editor)).toBe(`See [Reading list](forma://page/${TARGET})\n`);
    expect(useLinkStore.getState().pages[TARGET]?.title).toBe("Reading list");
    editor.destroy();
  });

  it("inserts nothing when the ID can't be saved", async () => {
    vi.mocked(ipc.ensurePersistedId).mockRejectedValue(new Error("Read-only workspace"));
    const editor = editorWith("See [[read");
    const end = editor.state.doc.content.size - 1;
    await insertPageLink(editor, { from: end - 6, to: end }, hit);
    expect(docToMarkdown(editor)).toBe("See \\[\\[read\n");
    expect(useWorkspaceStore.getState().notices).toEqual(["Read-only workspace"]);
    editor.destroy();
  });
});
