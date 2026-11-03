import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { TooltipProvider } from "@/components/ui/tooltip";
import { defaultSettings } from "@/features/settings/defaults";
import { resetSettingsStoreForTests, useSettingsStore } from "@/features/settings/settingsStore";
import { useUiStore } from "@/features/ui/uiStore";
import { ipc } from "@/lib/ipc/client";
import { AppShell } from "./AppShell";

vi.mock("@/lib/ipc/client", () => ({
  ipc: { saveSettings: vi.fn(), getSettings: vi.fn(), appReady: vi.fn() },
}));

beforeEach(() => {
  resetSettingsStoreForTests();
  useUiStore.setState({ dialog: null });
  vi.mocked(ipc.saveSettings).mockReset();
  vi.mocked(ipc.saveSettings).mockImplementation(async (update) => ({
    ...defaultSettings(),
    ...update,
  }));
});

function renderShell() {
  render(
    <TooltipProvider>
      <AppShell />
    </TooltipProvider>,
  );
}

const panel = (name: string) => screen.getByRole("complementary", { name, hidden: true });

describe("AppShell", () => {
  it("starts with the sidebar open and the AI panel hidden", () => {
    renderShell();
    expect(panel("Sidebar")).toHaveAttribute("data-state", "open");
    expect(panel("AI assistant")).toHaveAttribute("data-state", "closed");
    expect(panel("AI assistant")).toHaveAttribute("inert");
  });

  it("hides and shows the sidebar from its own buttons", async () => {
    const user = userEvent.setup();
    renderShell();

    await user.click(screen.getByRole("button", { name: "Hide sidebar" }));
    expect(panel("Sidebar")).toHaveAttribute("data-state", "closed");

    await user.click(screen.getByRole("button", { name: "Show sidebar" }));
    expect(panel("Sidebar")).toHaveAttribute("data-state", "open");
    expect(useSettingsStore.getState().settings.layout.sidebar.visible).toBe(true);
  });

  it("opens and closes the AI panel", async () => {
    const user = userEvent.setup();
    renderShell();

    await user.click(screen.getByRole("button", { name: "Open AI assistant" }));
    expect(panel("AI assistant")).toHaveAttribute("data-state", "open");
    expect(screen.getByText("Not available yet")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Close AI assistant" }));
    expect(panel("AI assistant")).toHaveAttribute("data-state", "closed");
  });

  it("opens search and settings from the sidebar", async () => {
    const user = userEvent.setup();
    renderShell();
    await user.click(screen.getByRole("button", { name: /^Search/ }));
    expect(useUiStore.getState().dialog).toBe("search");
    await user.click(screen.getByRole("button", { name: /^Settings/ }));
    expect(useUiStore.getState().dialog).toBe("settings");
  });

  it("shows a settings notice and lets the user dismiss it", async () => {
    const user = userEvent.setup();
    useSettingsStore
      .getState()
      .hydrate({ settings: defaultSettings(), notice: "Settings were reset.", read_only: false });
    renderShell();

    expect(screen.getByRole("status")).toHaveTextContent("Settings were reset.");
    await user.click(screen.getByRole("button", { name: "Dismiss" }));
    expect(screen.queryByRole("status")).toBeNull();
  });
});
