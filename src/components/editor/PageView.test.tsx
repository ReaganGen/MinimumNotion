import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { Editor } from "@tiptap/core";
import { StrictMode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { pickAttachment } from "@/features/editor/media";
import { resetLinkStoreForTests } from "@/features/links/linkStore";
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
    pickAttachment: vi.fn(),
    attachmentInfo: vi.fn(),
    openExternal: vi.fn(),
    backlinks: vi.fn(),
    resolvePages: vi.fn(),
    resolveWikilinks: vi.fn(),
    ensurePersistedId: vi.fn(),
    quickOpen: vi.fn(),
    createFolder: vi.fn(),
    turnFolderIntoPage: vi.fn(),
  },
  assetUrl: (pageId: string, reference: string) =>
    `diaspar-asset://localhost/${pageId}/${reference}`,
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
    is_collection: false,
    collection_id: null,
    ...overrides,
  };
}

const saved = (bodyRev: string) =>
  ({ outcome: "saved", rev: `r-${bodyRev}`, body_rev: bodyRev, updated_at: "now" }) as const;

beforeEach(() => {
  resetWorkspaceStoreForTests();
  resetLinkStoreForTests();
  useWorkspaceStore.setState({ status: "open", activePageId: ID });
  for (const fn of Object.values(ipc)) vi.mocked(fn).mockReset();
  vi.mocked(ipc.getPage).mockResolvedValue(page());
  vi.mocked(ipc.writeRecovery).mockResolvedValue(undefined);
  vi.mocked(ipc.discardRecovery).mockResolvedValue(undefined);
  vi.mocked(ipc.snapshotText).mockResolvedValue(undefined);
  vi.mocked(ipc.listChildren).mockResolvedValue([]);
  vi.mocked(ipc.listFavorites).mockResolvedValue([]);
  vi.mocked(ipc.takeNotices).mockResolvedValue([]);
  vi.mocked(ipc.backlinks).mockResolvedValue([]);
  vi.mocked(ipc.resolvePages).mockResolvedValue([]);
  vi.mocked(ipc.resolveWikilinks).mockResolvedValue([]);
  vi.mocked(ipc.attachmentInfo).mockResolvedValue({ name: "Report.pdf", size: 1000 });
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

describe("attachments (ADR-049)", () => {
  it("a file picked from the / menu becomes a card in its own paragraph", async () => {
    vi.mocked(ipc.savePage).mockResolvedValue(saved("b2"));
    vi.mocked(ipc.pickAttachment).mockResolvedValue("_assets/Q3%20report.pdf");
    const editor = await renderPage();
    act(() => {
      editor.commands.focus("end");
    });
    await act(() => pickAttachment(editor, "file"));
    expect(ipc.pickAttachment).toHaveBeenCalledWith(ID, "file");
    await waitFor(() => expect(ipc.savePage).toHaveBeenCalled(), { timeout: 2000 });
    expect(ipc.savePage).toHaveBeenLastCalledWith(
      ID,
      "b1",
      "Hello\n\n[Q3 report.pdf](_assets/Q3%20report.pdf)\n",
    );
  });

  it("a cancelled dialog inserts nothing", async () => {
    vi.mocked(ipc.pickAttachment).mockResolvedValue(null);
    const editor = await renderPage();
    await act(() => pickAttachment(editor, "video"));
    await new Promise((resolve) => setTimeout(resolve, 900));
    expect(ipc.savePage).not.toHaveBeenCalled();
    expect(ipc.getPage).toHaveBeenCalledTimes(1);
  });

  it("a pasted file is stored and shown by kind", async () => {
    vi.mocked(ipc.savePage).mockResolvedValue(saved("b2"));
    vi.mocked(ipc.importAttachment)
      .mockResolvedValueOnce("_assets/Report.pdf")
      .mockResolvedValueOnce("_assets/Clip.mov");
    const editor = await renderPage();
    act(() => {
      editor.commands.focus("end");
    });
    const files = [
      new File(["%PDF"], "Report.pdf", { type: "application/pdf" }),
      new File(["mov"], "Clip.mov", { type: "video/quicktime" }),
    ];
    const event = new Event("paste", { bubbles: true, cancelable: true });
    Object.defineProperty(event, "clipboardData", {
      value: { files, types: ["Files"], items: [], getData: () => "" },
    });
    act(() => {
      editor.view.dom.dispatchEvent(event);
    });
    await waitFor(() => expect(ipc.importAttachment).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(ipc.savePage).toHaveBeenCalled(), { timeout: 2000 });
    expect(ipc.savePage).toHaveBeenLastCalledWith(
      ID,
      "b1",
      "Hello\n\n[Report.pdf](_assets/Report.pdf)\n\n![Clip](_assets/Clip.mov)\n",
    );
  });
});

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
      "This page was changed outside Diaspar while you were editing.",
      {},
      {
        timeout: 2000,
      },
    );
    expect(ipc.writeRecovery).toHaveBeenCalledWith(ID, "Hello mine\n", "conflict");
    expect(editor.isEditable).toBe(false);

    vi.mocked(ipc.getPage).mockResolvedValue(page({ body: "Changed elsewhere\n", body_rev: "b9" }));
    await user.click(screen.getByRole("button", { name: "Use theirs" }));
    await waitFor(() => expect(screen.queryByText(/changed outside Diaspar/)).toBeNull());
    // "Theirs" still keeps "mine" in the page's history.
    expect(ipc.snapshotText).toHaveBeenCalledWith(ID, "Hello mine\n", "conflict");
    expect(ipc.discardRecovery).toHaveBeenCalledWith(ID);
    expect(editor.getText()).toBe("Changed elsewhere");
    expect(editor.isEditable).toBe(true);
  });

  it("keeps unsaved edits when the file is deleted outside Diaspar", async () => {
    const user = userEvent.setup();
    const editor = await renderPage();
    type(editor, " unsaved");
    act(() => useWorkspaceStore.setState({ activeRemoved: 1 }));

    await screen.findByText("This page's file was deleted outside Diaspar.");
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
      is_collection: false,
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

  it("shows a page link with its target's current title and opens it on click", async () => {
    const user = userEvent.setup();
    const TARGET = "44444444-4444-4444-8444-444444444444";
    vi.mocked(ipc.getPage).mockResolvedValue(
      page({ body: `See [Old title](diaspar://page/${TARGET}).\n` }),
    );
    vi.mocked(ipc.resolvePages).mockResolvedValue([
      { id: TARGET, title: "New title", icon: null, state: "ok" },
    ]);
    await renderPage();
    const link = await screen.findByText("New title");
    expect(link).toHaveAttribute("data-state", "ok");
    await user.click(link);
    expect(useWorkspaceStore.getState().activePageId).toBe(TARGET);
  });

  it("marks a link to a trashed page and doesn't follow it", async () => {
    const user = userEvent.setup();
    const GONE = "55555555-5555-4555-8555-555555555555";
    vi.mocked(ipc.getPage).mockResolvedValue(page({ body: `[Gone](diaspar://page/${GONE})\n` }));
    vi.mocked(ipc.resolvePages).mockResolvedValue([
      { id: GONE, title: "Gone", icon: null, state: "trashed" },
    ]);
    await renderPage();
    await waitFor(() => expect(screen.getByText("Gone")).toHaveAttribute("data-state", "trashed"));
    await user.click(screen.getByText("Gone"));
    expect(useWorkspaceStore.getState().activePageId).toBe(ID);
  });

  it("lists the pages that link here", async () => {
    const user = userEvent.setup();
    vi.mocked(ipc.backlinks).mockResolvedValue([
      {
        id: "66666666-6666-4666-8666-666666666666",
        title: "Source",
        icon: null,
        kind: "leaf",
        path: ["Projects"],
        snippet: null,
      },
    ]);
    await renderPage();
    const section = await screen.findByRole("region", { name: "Backlinks" });
    expect(section).toHaveTextContent("1 page links here");
    await user.click(screen.getByRole("button", { name: /Source/ }));
    expect(useWorkspaceStore.getState().activePageId).toBe("66666666-6666-4666-8666-666666666666");
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

  it("shows a folder's contents instead of an editor, and writes nothing (ADR-047)", async () => {
    const user = userEvent.setup();
    const folder = page({ title: "Archive", kind: "folder", body: "", rev: null });
    vi.mocked(ipc.getPage).mockResolvedValueOnce(folder).mockResolvedValue(page());
    vi.mocked(ipc.listChildren).mockResolvedValue([
      {
        id: "33333333-3333-4333-8333-333333333333",
        title: "Old plan",
        icon: null,
        kind: "leaf",
        has_children: false,
        favorite: false,
        status: { state: "ok" },
        is_collection: false,
      },
    ]);
    vi.mocked(ipc.turnFolderIntoPage).mockResolvedValue(undefined);
    render(<PageView pageId={ID} />);
    expect(await screen.findByRole("heading", { name: "Archive" })).toBeInTheDocument();
    expect(await screen.findByRole("button", { name: "Old plan" })).toBeInTheDocument();
    expect(screen.queryByRole("textbox", { name: "Page content" })).toBeNull();
    expect(ipc.savePage).not.toHaveBeenCalled();

    await user.click(screen.getByRole("button", { name: "Turn into page" }));
    expect(ipc.turnFolderIntoPage).toHaveBeenCalledWith(ID);
  });
});
