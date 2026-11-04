import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { TooltipProvider } from "@/components/ui/tooltip";
import {
  resetWorkspaceStoreForTests,
  useWorkspaceStore,
} from "@/features/workspace/workspaceStore";
import type { CollectionData } from "@/lib/ipc/bindings/CollectionData";
import { ipc } from "@/lib/ipc/client";
import { CollectionView } from "./CollectionView";

vi.mock("@/lib/ipc/client", () => ({
  ipc: {
    getCollection: vi.fn(),
    getViews: vi.fn(),
    saveViews: vi.fn(),
    setProperty: vi.fn(),
    updateSchema: vi.fn(),
    createPage: vi.fn(),
    removeCollection: vi.fn(),
  },
}));

const COLLECTION = "c0000000-0000-4000-8000-000000000000";

function data(overrides: Partial<CollectionData> = {}): CollectionData {
  return {
    id: COLLECTION,
    title: "Papers",
    problem: null,
    schema: {
      version: 1,
      properties: [
        {
          id: "status00",
          name: "Status",
          type: "select",
          options: [
            { id: "aaaa", name: "To read", color: "gray" },
            { id: "bbbb", name: "Reading", color: "blue" },
          ],
        },
        { id: "year0000", name: "Year", type: "number", options: [] },
        { id: "read0000", name: "Read", type: "checkbox", options: [] },
      ],
    },
    rows: [
      {
        id: "p1",
        title: "Attention",
        icon: null,
        created_at: null,
        updated_at: null,
        editable: true,
        cells: {
          status00: { kind: "select", value: "Reading", known: true },
          year0000: { kind: "number", value: 2017, from_text: false },
          read0000: { kind: "checkbox", value: false },
        },
      },
      {
        id: "p2",
        title: "BERT",
        icon: null,
        created_at: null,
        updated_at: null,
        editable: true,
        cells: {
          status00: { kind: "empty" },
          year0000: { kind: "invalid", raw: "twenty", problem: "not a number" },
          read0000: { kind: "empty" },
        },
      },
    ],
    ...overrides,
  };
}

const emptyView = { sort: [], filter: [], hidden: [], column_widths: {} };

beforeEach(() => {
  resetWorkspaceStoreForTests();
  useWorkspaceStore.setState({ status: "open" });
  for (const fn of Object.values(ipc)) vi.mocked(fn).mockReset();
  vi.mocked(ipc.getCollection).mockResolvedValue(data());
  vi.mocked(ipc.getViews).mockResolvedValue({
    active_view: "table",
    table: emptyView,
    list: emptyView,
  });
  vi.mocked(ipc.saveViews).mockResolvedValue(undefined);
  vi.mocked(ipc.setProperty).mockResolvedValue(undefined);
  vi.mocked(ipc.updateSchema).mockResolvedValue(data().schema as never);
});

async function renderView(editable = true) {
  render(
    <TooltipProvider>
      <CollectionView id={COLLECTION} editable={editable} />
    </TooltipProvider>,
  );
  return screen.findByRole("table", { name: "Papers" });
}

describe("CollectionView", () => {
  it("shows entries as a table and as a list", async () => {
    const user = userEvent.setup();
    const table = await renderView();
    expect(
      within(table)
        .getAllByRole("columnheader")
        .map((h) => h.textContent),
    ).toEqual(["Title", "Status", "Year", "Read", ""]);
    expect(within(table).getByText("Attention")).toBeInTheDocument();
    expect(within(table).getByLabelText("Invalid value")).toBeInTheDocument();
    expect(screen.getByText("2 entries")).toBeInTheDocument();

    await user.click(screen.getByRole("radio", { name: "List" }));
    expect(screen.queryByRole("table")).toBeNull();
    expect(screen.getByRole("list", { name: "Papers" })).toHaveTextContent("BERT");
    await waitFor(() =>
      expect(ipc.saveViews).toHaveBeenCalledWith(
        COLLECTION,
        expect.objectContaining({ active_view: "list" }),
      ),
    );
  });

  it("edits a number in place and refuses text that isn't one", async () => {
    const user = userEvent.setup();
    await renderView();
    await user.click(screen.getByRole("button", { name: "Year for Attention" }));
    const input = screen.getByRole("textbox", { name: "Year for Attention" });
    await user.clear(input);
    await user.type(input, "abc{Enter}");
    expect(input).toHaveAttribute("aria-invalid", "true");
    expect(ipc.setProperty).not.toHaveBeenCalled();
    await user.clear(input);
    await user.type(input, "2018{Enter}");
    expect(ipc.setProperty).toHaveBeenCalledWith("p1", "year0000", { kind: "number", value: 2018 });
  });

  it("toggles a checkbox", async () => {
    const user = userEvent.setup();
    await renderView();
    await user.click(screen.getByRole("checkbox", { name: "Read for BERT" }));
    expect(ipc.setProperty).toHaveBeenCalledWith("p2", "read0000", {
      kind: "checkbox",
      value: true,
    });
  });

  it("picks an option, or creates one and picks it", async () => {
    const user = userEvent.setup();
    await renderView();
    await user.click(screen.getByRole("button", { name: "Status for BERT" }));
    await user.click(await screen.findByRole("option", { name: "To read" }));
    expect(ipc.setProperty).toHaveBeenCalledWith("p2", "status00", {
      kind: "select",
      value: "To read",
    });

    await user.click(screen.getByRole("button", { name: "Status for BERT" }));
    await user.type(
      await screen.findByRole("textbox", { name: "Find or create an option" }),
      "Done{Enter}",
    );
    await waitFor(() =>
      expect(ipc.updateSchema).toHaveBeenCalledWith(COLLECTION, {
        op: "add_option",
        property_id: "status00",
        name: "Done",
      }),
    );
    await waitFor(() =>
      expect(ipc.setProperty).toHaveBeenLastCalledWith("p2", "status00", {
        kind: "select",
        value: "Done",
      }),
    );
  });

  it("adds a property and a new entry", async () => {
    const user = userEvent.setup();
    vi.mocked(ipc.createPage).mockResolvedValue({} as never);
    await renderView();
    await user.click(screen.getByRole("button", { name: "Add a property" }));
    await user.type(screen.getByRole("textbox", { name: "Property name" }), "Venue");
    await user.selectOptions(screen.getByRole("combobox", { name: "Type" }), "select");
    await user.click(screen.getByRole("button", { name: "Add property" }));
    expect(ipc.updateSchema).toHaveBeenCalledWith(COLLECTION, {
      op: "add_property",
      name: "Venue",
      type: "select",
    });
    await user.click(screen.getByRole("button", { name: "New" }));
    expect(ipc.createPage).toHaveBeenCalledWith(COLLECTION, "Untitled");
  });

  it("filters rows and says how many are shown", async () => {
    const user = userEvent.setup();
    await renderView();
    await user.click(screen.getByRole("button", { name: "Filter" }));
    await user.click(screen.getByRole("button", { name: "Add a filter" }));
    await user.type(screen.getByRole("textbox", { name: "Value" }), "bert");
    const table = screen.getByRole("table", { name: "Papers" });
    await waitFor(() => expect(within(table).queryByText("Attention")).toBeNull());
    expect(within(table).getByText("BERT")).toBeInTheDocument();
    expect(screen.getByText("1 of 2 entries")).toBeInTheDocument();
  });

  it("is read-only when the schema comes from a newer Diaspar", async () => {
    vi.mocked(ipc.getCollection).mockResolvedValue(
      data({ problem: "This collection was made by a newer version of Diaspar." }),
    );
    await renderView();
    expect(screen.getByText(/newer version of Diaspar/)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "New" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Add a property" })).toBeNull();
    expect(screen.getByRole("checkbox", { name: "Read for BERT" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Year for Attention" })).toBeDisabled();
  });
});
