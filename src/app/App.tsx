import { useEffect } from "react";
import { AppShell } from "@/components/layout/AppShell";
import { PaletteDialog } from "@/components/search/PaletteDialog";
import { SettingsDialog } from "@/components/settings/SettingsDialog";
import { TooltipProvider } from "@/components/ui/tooltip";
import { startMenuBridge } from "@/features/commands/commands";
import { useSettingsStore } from "@/features/settings/settingsStore";
import { useUiStore } from "@/features/ui/uiStore";
import { useThemeSync } from "@/lib/theme";

interface AppProps {
  /** Called once the first render is committed, so the shell can show the window. */
  onReady?: () => void;
}

export function App({ onReady }: AppProps) {
  const theme = useSettingsStore((s) => s.settings.appearance.theme);
  const dialog = useUiStore((s) => s.dialog);
  const closeDialog = useUiStore((s) => s.closeDialog);
  useThemeSync(theme);

  useEffect(() => {
    const unlisten = startMenuBridge();
    onReady?.();
    return () => {
      void unlisten.then((stop) => stop());
    };
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
        unavailableMessage="Search becomes available once a workspace is open."
      />
      <PaletteDialog
        open={dialog === "quick-open"}
        onOpenChange={onOpenChange}
        title="Quick open"
        placeholder="Jump to a page…"
        unavailableMessage="Quick open becomes available once a workspace is open."
      />
    </TooltipProvider>
  );
}
