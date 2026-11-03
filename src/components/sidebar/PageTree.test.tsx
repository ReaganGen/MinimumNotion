import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { resetAiChatStoreForTests, useAiChatStore } from "@/features/ai/aiStore";
import { defaultSettings } from "@/features/settings/defaults";
import { resetSettingsStoreForTests, useSettingsStore } from "@/features/settings/settingsStore";
import {
  resetWorkspaceStoreForTests,
  useWorkspaceStore,
} from "@/features/workspace/workspaceStore";
import type { PageSummary } from "@/lib/ipc/bindings/PageSummary";
import { ipc } from "@/lib/ipc/client";
import { PageTree } from "./PageTree";

vi.mock("@/lib/ipc/client", () => ({
  ipc: {
    listChildren: vi.fn(),
    setTitle: vi.fn(),
    createPage: vi.fn(),
    listFavorites: vi.fn(),
    movePage: vi.fn(),
    saveSettings: vi.fn(),
  },
}));

function summary(id: string, title: string, extra: Partial<PageSummary> = {}): PageSummary {
  return {
    id,
    title,
    icon: null,
    kind: "leaf",
    has_children: false,
    favorite: false,
    status: { state: "ok" },
    is_collection: false,
    ...extra,
  };
}

beforeEach(() => {
  resetWorkspaceStoreForTests();
  resetSettingsStoreForTests();
  resetAiChatStoreForTests();
  for (const fn of Object.values(ipc)) vi.mocked(fn).mockReset();
  useWorkspaceStore.setState({
    status: "open",
    children: {
      root: [summary("a", "Projects", { has_children: true }), summary("b", "Journal")],
    },
  });
});

describe("PageTree", () => {
  it("lists pages and opens one on click", async () => {
    const user = userEvent.setup();
    render(<PageTree />);
    const list = screen.getByRole("list", { name: "Pages" });
    expect(within(list).getAllByRole("listitem")).toHaveLength(2);

    await user.click(screen.getByRole("button", { name: "Journal" }));
    expect(useWorkspaceStore.getState().activePageId).toBe("b");
    expect(screen.getByRole("button", { name: "Journal" })).toHaveAttribute("aria-current", "page");
  });

  it("expands a page to load and show what's inside", async () => {
    const user = userEvent.setup();
    vi.mocked(ipc.listChildren).mockResolvedValue([summary("c", "Forma")]);
    render(<PageTree />);

    const toggle = screen.getByRole("button", { name: "Expand Projects" });
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    await user.click(toggle);
    expect(ipc.listChildren).toHaveBeenCalledWith("a");
    expect(await screen.findByRole("button", { name: "Forma" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Collapse Projects" })).toHaveAttribute(
      "aria-expanded",
      "true",
    );
  });

  it("says when an expanded page has nothing inside", async () => {
    const user = userEvent.setup();
    vi.mocked(ipc.listChildren).mockResolvedValue([]);
    render(<PageTree />);
    await user.click(screen.getByRole("button", { name: "Expand Projects" }));
    expect(await screen.findByText("No pages inside")).toBeInTheDocument();
  });

  it("renames a page in place with a double-click", async () => {
    const user = userEvent.setup();
    vi.mocked(ipc.setTitle).mockResolvedValue(undefined);
    render(<PageTree />);

    await user.dblClick(screen.getByRole("button", { name: "Journal" }));
    const input = screen.getByRole("textbox", { name: "Page title" });
    await user.clear(input);
    await user.type(input, "Diary{Enter}");
    expect(ipc.setTitle).toHaveBeenCalledWith("b", "Diary");
    await waitFor(() => expect(screen.queryByRole("textbox")).toBeNull());
  });

  it("cancels a rename with Escape and ignores an empty title", async () => {
    const user = userEvent.setup();
    render(<PageTree />);

    await user.dblClick(screen.getByRole("button", { name: "Journal" }));
    await user.type(screen.getByRole("textbox", { name: "Page title" }), "x{Escape}");
    await user.dblClick(screen.getByRole("button", { name: "Journal" }));
    await user.clear(screen.getByRole("textbox", { name: "Page title" }));
    await user.keyboard("{Enter}");
    expect(ipc.setTitle).not.toHaveBeenCalled();
  });

  it("moves a page with Move to…, never into itself", async () => {
    const user = userEvent.setup();
    vi.mocked(ipc.listChildren).mockImplementation(async (parentId) =>
      parentId === null
        ? [summary("a", "Projects", { has_children: true }), summary("b", "Journal")]
        : [summary("c", "Forma")],
    );
    vi.mocked(ipc.movePage).mockResolvedValue(undefined);
    render(<PageTree />);

    await user.click(screen.getByRole("button", { name: "Actions for Journal" }));
    await user.click(await screen.findByRole("menuitem", { name: "Move to…" }));
    const dialog = await screen.findByRole("dialog", { name: "Move “Journal”" });
    // The page itself isn't a destination; where it is now can't be chosen.
    expect(within(dialog).queryByRole("button", { name: "Journal" })).toBeNull();
    expect(within(dialog).getByRole("button", { name: /Top level/ })).toBeDisabled();

    await user.click(within(dialog).getByRole("button", { name: "Expand Projects" }));
    await user.click(await within(dialog).findByRole("button", { name: "Forma" }));
    expect(ipc.movePage).toHaveBeenCalledWith("b", "c", null);
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });

  it("offers AI actions only when AI is on", async () => {
    const user = userEvent.setup();
    vi.mocked(ipc.saveSettings).mockImplementation(
      async () => useSettingsStore.getState().settings,
    );
    const { unmount } = render(<PageTree />);
    await user.click(screen.getByRole("button", { name: "Actions for Journal" }));
    expect(await screen.findByRole("menuitem", { name: "Move to…" })).toBeInTheDocument();
    expect(screen.queryByRole("menuitem", { name: /Ask AI/ })).toBeNull();
    await user.keyboard("{Escape}");
    unmount();

    const settings = defaultSettings();
    useSettingsStore.setState({ settings: { ...settings, ai: { ...settings.ai, enabled: true } } });
    render(<PageTree />);
    await user.click(screen.getByRole("button", { name: "Actions for Journal" }));
    await user.click(await screen.findByRole("menuitem", { name: "Ask AI about this page" }));
    expect(useAiChatStore.getState().page).toEqual({ id: "b", title: "Journal" });
    expect(useSettingsStore.getState().settings.layout.ai_panel.visible).toBe(true);
  });

  it("shows an empty workspace honestly", () => {
    useWorkspaceStore.setState({ children: { root: [] } });
    render(<PageTree />);
    expect(screen.getByText("No pages yet.")).toBeInTheDocument();
  });
});
