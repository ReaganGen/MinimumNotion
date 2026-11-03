import { beforeEach, describe, expect, it, vi } from "vitest";
import type { PageSummary } from "@/lib/ipc/bindings/PageSummary";
import type { WorkspaceInfo } from "@/lib/ipc/bindings/WorkspaceInfo";
import { ipc } from "@/lib/ipc/client";
import { resetWorkspaceStoreForTests, useWorkspaceStore } from "./workspaceStore";

vi.mock("@/lib/ipc/client", () => ({
  ipc: {
    openLastWorkspace: vi.fn(),
    chooseWorkspace: vi.fn(),
    listChildren: vi.fn(),
    listFavorites: vi.fn(),
    takeNotices: vi.fn(),
    createPage: vi.fn(),
  },
}));

const state = () => useWorkspaceStore.getState();

function summary(id: string, title: string, extra: Partial<PageSummary> = {}): PageSummary {
  return {
    id,
    title,
    icon: null,
    kind: "leaf",
    has_children: false,
    favorite: false,
    status: { state: "ok" },
    ...extra,
  };
}

const info = (notices: string[] = []): WorkspaceInfo => ({
  root: "/Users/me/Notes",
  name: "Notes",
  id: "w",
  read_only_reason: null,
  notices,
});

const noChanges = { content_changed: [], removed: [], tree_changed: false };

beforeEach(() => {
  resetWorkspaceStoreForTests();
  for (const fn of Object.values(ipc)) vi.mocked(fn).mockReset();
  vi.mocked(ipc.listChildren).mockResolvedValue([summary("a", "A"), summary("b", "B")]);
  vi.mocked(ipc.listFavorites).mockResolvedValue([]);
  vi.mocked(ipc.takeNotices).mockResolvedValue([]);
});

describe("workspaceStore", () => {
  it("shows the welcome screen when there's no workspace to reopen", async () => {
    vi.mocked(ipc.openLastWorkspace).mockResolvedValue(null);
    await state().openLast();
    expect(state().status).toBe("none");
  });

  it("opens the last workspace, loads the tree and opens the first page", async () => {
    vi.mocked(ipc.openLastWorkspace).mockResolvedValue(info(["Same", "Same"]));
    await state().openLast();
    expect(state().status).toBe("open");
    expect(state().children.root?.map((p) => p.title)).toEqual(["A", "B"]);
    expect(state().activePageId).toBe("a");
    expect(state().notices).toEqual(["Same"]);
  });

  it("keeps the current workspace when choosing another is cancelled", async () => {
    vi.mocked(ipc.openLastWorkspace).mockResolvedValue(info());
    await state().openLast();
    vi.mocked(ipc.chooseWorkspace).mockResolvedValue(null);
    await state().choose();
    expect(state().status).toBe("open");
    expect(state().info?.root).toBe("/Users/me/Notes");
  });

  it("tells the open page about outside changes instead of closing it", async () => {
    useWorkspaceStore.setState({ status: "open", activePageId: "a" });
    await state().applyChanges({ ...noChanges, content_changed: ["a"] });
    expect(state().externalChange).toBe(1);

    await state().applyChanges({ ...noChanges, removed: ["a"] });
    expect(state().activeRemoved).toBe(1);
    // The page view decides; unsaved edits mustn't vanish with the file.
    expect(state().activePageId).toBe("a");
  });

  it("ignores changes to other pages", async () => {
    useWorkspaceStore.setState({ status: "open", activePageId: "a" });
    await state().applyChanges({ ...noChanges, content_changed: ["b"] });
    expect(state().externalChange).toBe(0);
    expect(ipc.listChildren).not.toHaveBeenCalled();
  });

  it("adds new notices from a rescan once each", async () => {
    useWorkspaceStore.setState({ status: "open", notices: ["Old"] });
    vi.mocked(ipc.takeNotices).mockResolvedValue(["Old", "New", "New"]);
    await state().applyChanges({ ...noChanges, tree_changed: true });
    expect(state().notices).toEqual(["Old", "New"]);
    state().dismissNotice("Old");
    expect(state().notices).toEqual(["New"]);
  });

  it("reports the same error once", () => {
    state().reportError(new Error("Disk full"));
    state().reportError(new Error("Disk full"));
    expect(state().notices).toEqual(["Disk full"]);
  });

  it("creates a page inside another, expands the parent and opens the new page", async () => {
    useWorkspaceStore.setState({ status: "open" });
    vi.mocked(ipc.createPage).mockResolvedValue(summary("c", "Untitled"));
    const id = await state().createPage("a");
    expect(id).toBe("c");
    expect(ipc.createPage).toHaveBeenCalledWith("a", "Untitled");
    expect(state().expanded.a).toBe(true);
    expect(state().activePageId).toBe("c");
  });

  it("loads a branch only when it's first expanded", async () => {
    useWorkspaceStore.setState({ status: "open" });
    state().toggleExpanded("a");
    await vi.waitFor(() => expect(ipc.listChildren).toHaveBeenCalledWith("a"));
    state().toggleExpanded("a");
    state().toggleExpanded("a");
    expect(ipc.listChildren).toHaveBeenCalledTimes(1);
  });
});
