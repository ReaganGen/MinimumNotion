import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { Editor } from "@tiptap/core";
import { StrictMode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  resetWorkspaceStoreForTests,
  useWorkspaceStore,
} from "@/features/workspace/workspaceStore";
import type { PageDocument } from "@/lib/ipc/bindings/PageDocument";
import { ipc } from "@/lib/ipc/client";
import { PageView } from "./PageView";

vi.mock("@/lib/ipc/client", () => ({
  ipc: {
    getPage: vi.fn(),
    savePage: vi.fn(),
    savePageOverConflict: vi.fn(),
    setTitle: vi.fn(),
    writeRecovery: vi.fn(),
    discardRecovery: vi.fn(),
    snapshotText: vi.fn(),
    createPage: vi.fn(),
    listChildren: vi.fn(),
    listFavorites: vi.fn(),
    takeNotices: vi.fn(),
    importAttachment: vi.fn(),
    openExternal: vi.fn(),
  },
  assetUrl: (pageId: string, reference: string) => `forma-asset://localhost/${pageId}/${reference}`,
}));

const ID = "11111111-1111-4111-8111-111111111111";
const PARENT = "22222222-2222-4222-8222-222222222222";

function page(overrides: Partial<PageDocument> = {}): PageDocument {
  return {
    id: ID,
    parent_id: PARENT,
    title: "Notes",
    icon: null,
    tags: [],
    created_at: null,
    updated_at: null,
    body: "Hello\n",
    rev: "r1",
    body_rev: "b1",
    kind: "leaf",
    status: { state: "ok" },
    favorite: false,
    ancestors: [],
    ...overrides,
  };
}

const saved = (bodyRev: string) =>
  ({ outcome: "saved", rev: `r-${bodyRev}`, body_rev: bodyRev, updated_at: "now" }) as const;

beforeEach(() => {
  resetWorkspaceStoreForTests();
  useWorkspaceStore.setState({ status: "open", activePageId: ID });
  for (const fn of Object.values(ipc)) vi.mocked(fn).mockReset();
  vi.mocked(ipc.getPage).mockResolvedValue(page());
  vi.mocked(ipc.writeRecovery).mockResolvedValue(undefined);
  vi.mocked(ipc.discardRecovery).mockResolvedValue(undefined);
  vi.mocked(ipc.snapshotText).mockResolvedValue(undefined);
  vi.mocked(ipc.listChildren).mockResolvedValue([]);
  vi.mocked(ipc.listFavorites).mockResolvedValue([]);
  vi.mocked(ipc.takeNotices).mockResolvedValue([]);
});

async function renderPage({ strict = false } = {}) {
  render(
    strict ? (
      <StrictMode>
        <PageView pageId={ID} />
      </StrictMode>
    ) : (
      <PageView pageId={ID} />
    ),
  );
  await screen.findByRole("textbox", { name: "Page content" });
  const dom = document.querySelector(".ProseMirror") as HTMLElement & { editor: Editor };
  return dom.editor;
}

function type(editor: Editor, text: string) {
  act(() => {
    editor.commands.insertContentAt(editor.state.doc.content.size - 1, text);
  });
}

describe("PageView", () => {
  it("shows the page and doesn't write it just for opening it", async () => {
    await renderPage();
    expect(screen.getByRole("textbox", { name: "Page title" })).toHaveValue("Notes");
    expect(screen.getByText("Hello")).toBeInTheDocument();
    await new Promise((resolve) => setTimeout(resolve, 900));
    expect(ipc.savePage).not.toHaveBeenCalled();
  });

  it("autosaves edits against the body revision it loaded", async () => {
    vi.mocked(ipc.savePage).mockResolvedValue(saved("b2"));
    const editor = await renderPage();
    type(editor, " world");
    await waitFor(() => expect(ipc.savePage).toHaveBeenCalledTimes(1), { timeout: 2000 });
    expect(ipc.savePage).toHaveBeenCalledWith(ID, "b1", "Hello world\n");
    await waitFor(() => expect(useWorkspaceStore.getState().saveStatus).toBe("saved"));
  });

  it("keeps saving when React runs effects twice (development mode)", async () => {
    vi.mocked(ipc.savePage).mockResolvedValue(saved("b2"));
    const editor = await renderPage({ strict: true });
    type(editor, " world");
    await waitFor(() => expect(ipc.savePage).toHaveBeenCalledTimes(1), { timeout: 2000 });
    expect(ipc.savePage).toHaveBeenCalledWith(ID, "b1", "Hello world\n");
  });

  it("pauses on a conflict, keeps a recovery copy, and can take the other version", async () => {
    const user = userEvent.setup();
    vi.mocked(ipc.savePage).mockResolvedValue({
      outcome: "conflict",
      current_body_rev: "b9",
      current_body: "Changed elsewhere\n",
    });
    const editor = await renderPage();
    type(editor, " mine");

    await screen.findByText(
      "This page was changed outside Forma while you were editing.",
      {},
      {
        timeout: 2000,
      },
    );
    expect(ipc.writeRecovery).toHaveBeenCalledWith(ID, "Hello mine\n", "conflict");
    expect(editor.isEditable).toBe(false);

    vi.mocked(ipc.getPage).mockResolvedValue(page({ body: "Changed elsewhere\n", body_rev: "b9" }));
    await user.click(screen.getByRole("button", { name: "Use theirs" }));
    await waitFor(() => expect(screen.queryByText(/changed outside Forma/)).toBeNull());
    // "Theirs" still keeps "mine" in the page's history.
    expect(ipc.snapshotText).toHaveBeenCalledWith(ID, "Hello mine\n", "conflict");
    expect(ipc.discardRecovery).toHaveBeenCalledWith(ID);
    expect(editor.getText()).toBe("Changed elsewhere");
    expect(editor.isEditable).toBe(true);
  });

  it("keeps unsaved edits when the file is deleted outside Forma", async () => {
    const user = userEvent.setup();
    const editor = await renderPage();
    type(editor, " unsaved");
    act(() => useWorkspaceStore.setState({ activeRemoved: 1 }));

    await screen.findByText("This page's file was deleted outside Forma.");
    expect(useWorkspaceStore.getState().activePageId).toBe(ID);
    expect(ipc.writeRecovery).toHaveBeenCalledWith(ID, "Hello unsaved\n", "conflict");

    const RESTORED = "33333333-3333-4333-8333-333333333333";
    vi.mocked(ipc.createPage).mockResolvedValue({
      id: RESTORED,
      title: "Notes",
      icon: null,
      kind: "leaf",
      has_children: false,
      favorite: false,
      status: { state: "ok" },
    });
    vi.mocked(ipc.getPage).mockResolvedValue(page({ id: RESTORED, body: "", body_rev: "e" }));
    vi.mocked(ipc.savePage).mockResolvedValue(saved("b3"));
    await user.click(screen.getByRole("button", { name: "Restore with my text" }));

    await waitFor(() => expect(useWorkspaceStore.getState().activePageId).toBe(RESTORED));
    expect(ipc.createPage).toHaveBeenCalledWith(PARENT, "Notes");
    expect(ipc.savePage).toHaveBeenCalledWith(RESTORED, "e", "Hello unsaved\n");
    // The old page is gone, so nothing may be saved to its ID.
    expect(ipc.savePage).not.toHaveBeenCalledWith(ID, expect.anything(), expect.anything());
  });

  it("just closes a deleted page that has no unsaved edits", async () => {
    await renderPage();
    act(() => useWorkspaceStore.setState({ activeRemoved: 1 }));
    await waitFor(() => expect(useWorkspaceStore.getState().activePageId).toBeNull());
    expect(ipc.writeRecovery).not.toHaveBeenCalled();
  });

  it("puts the cursor in the title of a page the user just made", async () => {
    useWorkspaceStore.setState({ focusTitleOf: ID });
    vi.mocked(ipc.getPage).mockResolvedValue(page({ title: "Untitled", body: "" }));
    await renderPage();
    expect(screen.getByRole("textbox", { name: "Page title" })).toHaveFocus();
    expect(screen.getByRole("textbox", { name: "Page title" })).toHaveValue("");
    expect(useWorkspaceStore.getState().focusTitleOf).toBeNull();
  });

  it("renames the page from the title field", async () => {
    const user = userEvent.setup();
    vi.mocked(ipc.setTitle).mockResolvedValue(undefined);
    await renderPage();
    const title = screen.getByRole("textbox", { name: "Page title" });
    await user.clear(title);
    await user.type(title, "Ideas{Enter}");
    expect(ipc.setTitle).toHaveBeenCalledWith(ID, "Ideas");
    expect(ipc.setTitle).toHaveBeenCalledTimes(1);
    await waitFor(() =>
      expect(useWorkspaceStore.getState().breadcrumbs.at(-1)?.title).toBe("Ideas"),
    );
  });
});
