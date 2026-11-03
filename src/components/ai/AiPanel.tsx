import { Sparkles, X } from "lucide-react";
import { IconButton } from "@/components/layout/IconButton";
import { runCommand, SHORTCUTS } from "@/features/commands/commands";

/** The AI assistant panel. Phase 1 has no AI, so it explains that instead of showing fake controls. */
export function AiPanel() {
  return (
    <div className="flex h-full flex-col bg-background">
      <div
        data-tauri-drag-region
        className="flex h-(--titlebar-height) shrink-0 items-center justify-between pr-2 pl-4"
      >
        <h2 data-tauri-drag-region className="text-[13px] font-medium text-foreground">
          AI assistant
        </h2>
        <IconButton
          label="Close AI assistant"
          shortcut={SHORTCUTS["toggle-ai-panel"].keys}
          onClick={() => runCommand("toggle-ai-panel")}
        >
          <X />
        </IconButton>
      </div>

      <div className="flex flex-1 flex-col items-center justify-center px-10 pb-16 text-center">
        <Sparkles className="size-5 text-faint-foreground" aria-hidden />
        <p className="mt-3 text-[13px] font-medium text-foreground">Not available yet</p>
        <p className="mt-1 text-[13px] leading-5 text-muted-foreground">
          The assistant comes in a later build. AI in Forma is always optional, and everything works
          without it.
        </p>
      </div>
    </div>
  );
}
