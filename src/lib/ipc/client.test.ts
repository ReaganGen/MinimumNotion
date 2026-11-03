import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, describe, expect, it } from "vitest";
import { defaultSettings } from "@/features/settings/defaults";
import { IpcError, ipc } from "./client";

afterEach(() => clearMocks());

describe("ipc client", () => {
  it("passes the update under the `update` argument", async () => {
    let received: unknown;
    mockIPC((command, args) => {
      received = { command, args };
      return defaultSettings();
    });
    const update = { appearance: { theme: "dark" as const }, layout: defaultSettings().layout };
    await ipc.saveSettings(update);
    expect(received).toEqual({ command: "save_settings", args: { update } });
  });

  it("passes the theme under the `theme` argument", async () => {
    let received: unknown;
    mockIPC((command, args) => {
      received = { command, args };
      return null;
    });
    await ipc.applyWindowTheme("dark");
    expect(received).toEqual({ command: "apply_window_theme", args: { theme: "dark" } });
  });

  it("turns backend error payloads into IpcError", async () => {
    mockIPC(() => {
      throw { code: "read_only", message: "settings are read-only" };
    });
    const error = await ipc.getSettings().catch((e: unknown) => e);
    expect(error).toBeInstanceOf(IpcError);
    expect(error).toMatchObject({ code: "read_only", message: "settings are read-only" });
  });

  it("wraps unexpected errors as internal", async () => {
    mockIPC(() => {
      throw "boom";
    });
    await expect(ipc.appReady()).rejects.toMatchObject({ code: "internal", message: "boom" });
  });
});
