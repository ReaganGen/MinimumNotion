import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { defaultSettings } from "@/features/settings/defaults";
import { resetSettingsStoreForTests, useSettingsStore } from "@/features/settings/settingsStore";
import { ipc } from "@/lib/ipc/client";
import { SettingsDialog } from "./SettingsDialog";

vi.mock("@/lib/ipc/client", () => ({
  ipc: {
    saveSettings: vi.fn(),
    applyWindowTheme: vi.fn(),
    getSettings: vi.fn(),
    appReady: vi.fn(),
  },
}));

beforeEach(() => {
  resetSettingsStoreForTests();
  vi.mocked(ipc.saveSettings).mockReset();
  vi.mocked(ipc.saveSettings).mockImplementation(async (update) => ({
    ...defaultSettings(),
    ...update,
  }));
  vi.mocked(ipc.applyWindowTheme).mockReset();
  vi.mocked(ipc.applyWindowTheme).mockResolvedValue(undefined);
});

function renderOpen() {
  const onOpenChange = vi.fn();
  render(<SettingsDialog open onOpenChange={onOpenChange} />);
  return { dialog: screen.getByRole("dialog", { name: "Settings" }), onOpenChange };
}

describe("SettingsDialog", () => {
  it("changes and saves the theme", async () => {
    const user = userEvent.setup();
    renderOpen();
    expect(screen.getByRole("radio", { name: "System" })).toHaveAttribute("aria-checked", "true");

    await user.click(screen.getByRole("radio", { name: "Dark" }));
    expect(useSettingsStore.getState().settings.appearance.theme).toBe("dark");
    expect(ipc.saveSettings).toHaveBeenCalledWith(
      expect.objectContaining({ appearance: { theme: "dark" } }),
    );
  });

  it("shows unavailable sections as text only, with no controls", async () => {
    const user = userEvent.setup();
    const { dialog } = renderOpen();

    for (const name of ["Workspace", "AI", "Data"]) {
      await user.click(within(dialog).getByRole("button", { name }));
      expect(within(dialog).getByRole("heading", { name })).toBeInTheDocument();
      expect(within(dialog).queryByRole("radio")).toBeNull();
      expect(within(dialog).queryByRole("textbox")).toBeNull();
      // Only the section navigation and the close button are clickable.
      const buttons = within(dialog)
        .getAllByRole("button")
        .map((b) => b.textContent);
      expect(buttons).toEqual(["Appearance", "Workspace", "AI", "Data", "Close"]);
    }
  });

  it("closes on Escape", async () => {
    const user = userEvent.setup();
    const { onOpenChange } = renderOpen();
    await user.keyboard("{Escape}");
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  it("shows why settings are read-only, and still switches the window theme", async () => {
    const user = userEvent.setup();
    const reason = "Forma couldn't open its settings (permission denied).";
    useSettingsStore
      .getState()
      .hydrate({ settings: defaultSettings(), notice: null, read_only_reason: reason });
    renderOpen();
    expect(screen.getByText(reason)).toBeInTheDocument();

    await user.click(screen.getByRole("radio", { name: "Dark" }));
    expect(ipc.applyWindowTheme).toHaveBeenCalledWith("dark");
    expect(ipc.saveSettings).not.toHaveBeenCalled();
  });

  it("shows no read-only message when settings are writable", () => {
    renderOpen();
    expect(screen.queryByText(/won't be saved/)).toBeNull();
  });
});
