import { useEffect } from "react";
import { RecoveryDialog } from "@/components/editor/RecoveryDialog";
import { AppShell } from "@/components/layout/AppShell";
import { PaletteDialog } from "@/components/search/PaletteDialog";
import { SettingsDialog } from "@/components/settings/SettingsDialog";
import { TooltipProvider } from "@/components/ui/tooltip";
import { startMenuBridge } from "@/features/commands/commands";
import { useSettingsStore } from "@/features/settings/settingsStore";
import { useUiStore } from "@/features/ui/uiStore";
import { flushAllEdits, useWorkspaceStore } from "@/features/workspace/workspaceStore";
import { ipc, onFlushBeforeQuit, onWorkspaceChanged } from "@/lib/ipc/client";
import { useThemeSync } from "@/lib/theme";

interface AppProps {
  /** Called once the first render is committed, so the shell can show the window. */
  onReady?: () => void;
}

/** Subscribes to a Tauri event for the component's lifetime. */
function useSubscription(subscribe: () => Promise<() => void>) {
  // biome-ignore lint/correctness/useExhaustiveDependencies: subscribe once on mount
  useEffect(() => {
    const unlisten = subscribe();
    return () => {
      void unlisten.then((stop) => stop());
    };
  }, []);
}

export function App({ onReady }: AppProps) {
  const theme = useSettingsStore((s) => s.settings.appearance.theme);
  const dialog = useUiStore((s) => s.dialog);
  const closeDialog = useUiStore((s) => s.closeDialog);
  useThemeSync(theme);

  useSubscription(startMenuBridge);
  useSubscription(() =>
    onWorkspaceChanged((changes) => void useWorkspaceStore.getState().applyChanges(changes)),
  );
  // Quitting waits until every open editor has saved (src-tauri/src/main.rs).
  useSubscription(() =>
    onFlushBeforeQuit(async () => {
      await flushAllEdits();
      await ipc.confirmQuit();
    }),
  );

  useEffect(() => {
    onReady?.();
    void useWorkspaceStore.getState().openLast();
  }, [onReady]);

  const onOpenChange = (open: boolean) => {
    if (!open) closeDialog();
  };

  return (
    <TooltipProvider delayDuration={600} skipDelayDuration={300}>
      <AppShell />
      <SettingsDialog open={dialog === "settings"} onOpenChange={onOpenChange} />
      <PaletteDialog
        open={dialog === "search"}
        onOpenChange={onOpenChange}
        title="Search"
        placeholder="Search pages…"
        unavailableMessage="Search arrives in the next build."
      />
      <PaletteDialog
        open={dialog === "quick-open"}
        onOpenChange={onOpenChange}
        title="Quick open"
        placeholder="Jump to a page…"
        unavailableMessage="Quick open arrives in the next build."
      />
      <RecoveryDialog />
    </TooltipProvider>
  );
}
