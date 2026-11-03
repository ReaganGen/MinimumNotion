import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  resetWorkspaceStoreForTests,
  useWorkspaceStore,
} from "@/features/workspace/workspaceStore";
import type { PageHit } from "@/lib/ipc/bindings/PageHit";
import { ipc } from "@/lib/ipc/client";
import { PaletteDialog } from "./PaletteDialog";

vi.mock("@/lib/ipc/client", () => ({
  ipc: { search: vi.fn(), quickOpen: vi.fn() },
}));

const hit = (id: string, title: string, extra: Partial<PageHit> = {}): PageHit => ({
  id,
  title,
  icon: null,
  path: [],
  snippet: null,
  ...extra,
});

beforeEach(() => {
  resetWorkspaceStoreForTests();
  useWorkspaceStore.setState({ status: "open" });
  vi.mocked(ipc.search).mockReset();
  vi.mocked(ipc.quickOpen).mockReset();
});

describe("PaletteDialog", () => {
  it("searches, moves with the arrow keys and opens with Return", async () => {
    const user = userEvent.setup();
    const onOpenChange = vi.fn();
    vi.mocked(ipc.search).mockResolvedValue([
      hit("a", "Protein design", {
        path: ["Research"],
        snippet: "about \u0001protein\u0002 folds",
      }),
      hit("b", "Reading list"),
    ]);
    render(<PaletteDialog open onOpenChange={onOpenChange} mode="search" />);

    const input = screen.getByRole("combobox", { name: "Search" });
    await user.type(input, "protein");
    const options = await screen.findAllByRole("option");
    expect(ipc.search).toHaveBeenLastCalledWith("protein");
    expect(options[0]).toHaveAttribute("aria-selected", "true");
    expect(screen.getByText("Research")).toBeInTheDocument();
    // The snippet's match is a <mark>, not HTML from the index.
    const marks = options[0]?.querySelectorAll("mark") ?? [];
    expect([...marks].map((m) => m.textContent)).toContain("protein");

    await user.keyboard("{ArrowDown}");
    expect(screen.getAllByRole("option")[1]).toHaveAttribute("aria-selected", "true");
    await user.keyboard("{ArrowDown}");
    expect(screen.getAllByRole("option")[0]).toHaveAttribute("aria-selected", "true");
    await user.keyboard("{ArrowUp}{Enter}");
    expect(useWorkspaceStore.getState().activePageId).toBe("b");
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  it("ignores an older answer that arrives after a newer one", async () => {
    const user = userEvent.setup();
    let answerFirst: (hits: PageHit[]) => void = () => {};
    vi.mocked(ipc.search)
      .mockImplementationOnce(
        () =>
          new Promise((resolve) => {
            answerFirst = resolve;
          }),
      )
      .mockResolvedValueOnce([hit("new", "Newer result")]);
    render(<PaletteDialog open onOpenChange={() => {}} mode="search" />);
    const input = screen.getByRole("combobox", { name: "Search" });
    await user.type(input, "a");
    await waitFor(() => expect(ipc.search).toHaveBeenCalledTimes(1));
    await user.type(input, "b");
    expect(await screen.findByText("Newer result")).toBeInTheDocument();
    answerFirst([hit("old", "Older result")]);
    await new Promise((r) => setTimeout(r, 20));
    expect(screen.queryByText("Older result")).toBeNull();
  });

  it("says when nothing matches", async () => {
    const user = userEvent.setup();
    vi.mocked(ipc.search).mockResolvedValue([]);
    render(<PaletteDialog open onOpenChange={() => {}} mode="search" />);
    await user.type(screen.getByRole("combobox", { name: "Search" }), "zzz");
    expect(await screen.findByText("No pages match.")).toBeInTheDocument();
  });

  it("quick open lists recent pages before anything is typed", async () => {
    const user = userEvent.setup();
    useWorkspaceStore.getState().setBreadcrumbs([{ id: "x", title: "Inbox", icon: null }]);
    useWorkspaceStore.getState().setBreadcrumbs([{ id: "y", title: "Journal", icon: null }]);
    vi.mocked(ipc.quickOpen).mockResolvedValue([hit("x", "Inbox")]);
    render(<PaletteDialog open onOpenChange={() => {}} mode="quick-open" />);
    expect(screen.getByText("Recent")).toBeInTheDocument();
    expect(screen.getAllByRole("option").map((o) => o.textContent)).toEqual(["Journal", "Inbox"]);
    await user.type(screen.getByRole("combobox", { name: "Quick open" }), "inb");
    await waitFor(() => expect(ipc.quickOpen).toHaveBeenCalledWith("inb"));
    expect(screen.queryByText("Recent")).toBeNull();
  });

  it("explains that a workspace is needed", () => {
    useWorkspaceStore.setState({ status: "none" });
    render(<PaletteDialog open onOpenChange={() => {}} mode="search" />);
    expect(screen.getByText("Open a workspace to search it.")).toBeInTheDocument();
    expect(screen.getByRole("combobox", { name: "Search" })).toBeDisabled();
  });
});
