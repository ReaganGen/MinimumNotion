import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  resetWorkspaceStoreForTests,
  useWorkspaceStore,
} from "@/features/workspace/workspaceStore";
import type { WorkspaceInfo } from "@/lib/ipc/bindings/WorkspaceInfo";
import { EditorArea } from "./EditorArea";

vi.mock("@/lib/ipc/client", () => ({ ipc: {} }));

const info = (read_only_reason: string | null = null): WorkspaceInfo => ({
  root: "/Users/me/Notes",
  name: "Notes",
  id: "w",
  read_only_reason,
  notices: [],
});

beforeEach(() => resetWorkspaceStoreForTests());

describe("EditorArea", () => {
  it("offers a getting-started page in an empty workspace, and makes it only on a click", async () => {
    const user = userEvent.setup();
    const addGettingStarted = vi.fn(async () => {});
    useWorkspaceStore.setState({
      status: "open",
      info: info(),
      children: { root: [] },
      addGettingStarted,
    });
    render(<EditorArea />);
    expect(addGettingStarted).not.toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "Add a getting-started page" }));
    expect(addGettingStarted).toHaveBeenCalledOnce();
  });

  it("doesn't offer it once there are pages, or when the workspace is read-only", () => {
    useWorkspaceStore.setState({ status: "open", info: info("locked"), children: { root: [] } });
    const { unmount } = render(<EditorArea />);
    expect(screen.queryByRole("button", { name: "Add a getting-started page" })).toBeNull();
    unmount();
    useWorkspaceStore.setState({
      info: info(),
      children: {
        root: [
          {
            id: "a",
            title: "A",
            icon: null,
            kind: "leaf",
            has_children: false,
            favorite: false,
            status: { state: "ok" },
            is_collection: false,
          },
        ],
      },
    });
    render(<EditorArea />);
    expect(screen.getByRole("button", { name: /New page/ })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Add a getting-started page" })).toBeNull();
  });
});
