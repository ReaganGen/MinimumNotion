import "./styles/globals.css";
import { isTauri } from "@tauri-apps/api/core";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "@/app/App";
import { defaultSettings } from "@/features/settings/defaults";
import { useSettingsStore } from "@/features/settings/settingsStore";
import type { SettingsSnapshot } from "@/lib/ipc/bindings/SettingsSnapshot";
import { ipc } from "@/lib/ipc/client";
import { applyTheme, resolveTheme, systemPrefersDark } from "@/lib/theme";

async function loadSettings(): Promise<SettingsSnapshot> {
  try {
    return await ipc.getSettings();
  } catch (error) {
    const reason = `Settings couldn't be loaded (${(error as Error).message}). Defaults are in use, and changes won't be saved.`;
    return { settings: defaultSettings(), notice: reason, read_only_reason: reason };
  }
}

async function bootstrap() {
  // `vite dev` opened in a normal browser: stand in for the Rust side so the UI can be previewed.
  // Removed from production builds because `import.meta.env.DEV` is false there.
  if (import.meta.env.DEV && !isTauri()) {
    const { installBrowserPreview } = await import("@/lib/ipc/browserPreview");
    installBrowserPreview();
  }

  const snapshot = await loadSettings();
  useSettingsStore.getState().hydrate(snapshot);
  // Apply the theme before the first render. The window stays hidden until `app_ready`.
  applyTheme(resolveTheme(snapshot.settings.appearance.theme, systemPrefersDark()));

  const root = document.getElementById("root");
  if (!root) throw new Error("missing #root element");
  // `onReady` runs after the first commit. Don't wait for requestAnimationFrame here:
  // WebKit doesn't run frame callbacks for a window that isn't visible yet.
  createRoot(root).render(
    <StrictMode>
      <App onReady={signalReady} />
    </StrictMode>,
  );
}

let readySignalled = false;
function signalReady() {
  if (readySignalled) return;
  readySignalled = true;
  ipc.appReady().catch(() => {
    // The shell shows the window on its own after a timeout.
  });
}

void bootstrap();
