import { useEffect } from "react";
import type { ThemePreference } from "@/lib/ipc/bindings/ThemePreference";

export type ResolvedTheme = "light" | "dark";

const DARK_QUERY = "(prefers-color-scheme: dark)";

export function resolveTheme(preference: ThemePreference, systemIsDark: boolean): ResolvedTheme {
  if (preference === "system") return systemIsDark ? "dark" : "light";
  return preference;
}

export function systemPrefersDark(): boolean {
  return window.matchMedia?.(DARK_QUERY).matches ?? false;
}

export function applyTheme(theme: ResolvedTheme) {
  const root = document.documentElement;
  root.dataset.theme = theme;
  root.style.colorScheme = theme;
}

/**
 * Keeps `<html data-theme>` in sync with the preference. With "system", it
 * follows OS appearance changes live. For explicit choices the shell also sets
 * the native window appearance, so `prefers-color-scheme` agrees either way.
 */
export function useThemeSync(preference: ThemePreference) {
  useEffect(() => {
    applyTheme(resolveTheme(preference, systemPrefersDark()));
    if (preference !== "system" || !window.matchMedia) return;

    const query = window.matchMedia(DARK_QUERY);
    const onChange = (event: MediaQueryListEvent) => applyTheme(event.matches ? "dark" : "light");
    query.addEventListener("change", onChange);
    return () => query.removeEventListener("change", onChange);
  }, [preference]);
}
