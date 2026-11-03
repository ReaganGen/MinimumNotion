import { renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { applyTheme, resolveTheme, useThemeSync } from "./theme";

type Listener = (event: MediaQueryListEvent) => void;

/** A matchMedia stand-in whose dark-mode answer can be changed during a test. */
function mockSystemTheme(initiallyDark: boolean) {
  const listeners = new Set<Listener>();
  const query = {
    matches: initiallyDark,
    addEventListener: (_: string, listener: Listener) => listeners.add(listener),
    removeEventListener: (_: string, listener: Listener) => listeners.delete(listener),
  };
  vi.spyOn(window, "matchMedia").mockReturnValue(query as unknown as MediaQueryList);
  return {
    listeners,
    setDark(dark: boolean) {
      query.matches = dark;
      for (const listener of listeners) listener({ matches: dark } as MediaQueryListEvent);
    },
  };
}

afterEach(() => {
  delete document.documentElement.dataset.theme;
});

describe("resolveTheme", () => {
  it("follows the system only for the system preference", () => {
    expect(resolveTheme("system", true)).toBe("dark");
    expect(resolveTheme("system", false)).toBe("light");
    expect(resolveTheme("light", true)).toBe("light");
    expect(resolveTheme("dark", false)).toBe("dark");
  });
});

describe("applyTheme", () => {
  it("sets data-theme and color-scheme on <html>", () => {
    applyTheme("dark");
    expect(document.documentElement.dataset.theme).toBe("dark");
    expect(document.documentElement.style.colorScheme).toBe("dark");
  });
});

describe("useThemeSync", () => {
  it("tracks live system changes when following the system", () => {
    const system = mockSystemTheme(false);
    renderHook(() => useThemeSync("system"));
    expect(document.documentElement.dataset.theme).toBe("light");

    system.setDark(true);
    expect(document.documentElement.dataset.theme).toBe("dark");
  });

  it("ignores system changes for an explicit choice", () => {
    const system = mockSystemTheme(false);
    renderHook(() => useThemeSync("dark"));
    expect(document.documentElement.dataset.theme).toBe("dark");
    expect(system.listeners.size).toBe(0);
  });

  it("stops listening when the preference changes", () => {
    const system = mockSystemTheme(false);
    const { rerender } = renderHook(({ pref }) => useThemeSync(pref), {
      initialProps: { pref: "system" as const } as { pref: "system" | "light" },
    });
    expect(system.listeners.size).toBe(1);

    rerender({ pref: "light" });
    expect(system.listeners.size).toBe(0);
    system.setDark(true);
    expect(document.documentElement.dataset.theme).toBe("light");
  });
});
