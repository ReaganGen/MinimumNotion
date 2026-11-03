import type { LucideIcon } from "lucide-react";
import { Database, Folder, Palette, Sparkles } from "lucide-react";
import { type ReactNode, useState } from "react";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { useSettingsStore } from "@/features/settings/settingsStore";
import { useWorkspaceStore } from "@/features/workspace/workspaceStore";
import type { ThemePreference } from "@/lib/ipc/bindings/ThemePreference";
import { ipc } from "@/lib/ipc/client";
import { cn } from "@/lib/utils";

type SectionId = "appearance" | "workspace" | "ai" | "data";

const SECTIONS: { id: SectionId; label: string; icon: LucideIcon }[] = [
  { id: "appearance", label: "Appearance", icon: Palette },
  { id: "workspace", label: "Workspace", icon: Folder },
  { id: "ai", label: "AI", icon: Sparkles },
  { id: "data", label: "Data", icon: Database },
];

const THEMES: { value: ThemePreference; label: string }[] = [
  { value: "system", label: "System" },
  { value: "light", label: "Light" },
  { value: "dark", label: "Dark" },
];

interface SettingsDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

export function SettingsDialog({ open, onOpenChange }: SettingsDialogProps) {
  const [section, setSection] = useState<SectionId>("appearance");

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="h-[min(520px,85vh)] max-w-[720px] grid-cols-[188px_1fr] gap-0 overflow-hidden p-0 sm:max-w-[720px]">
        <nav
          aria-label="Settings sections"
          className="flex flex-col gap-0.5 border-r bg-sidebar p-2"
        >
          <DialogTitle className="px-2 pt-1.5 pb-2 text-xs font-medium text-faint-foreground">
            Settings
          </DialogTitle>
          {SECTIONS.map(({ id, label, icon: Icon }) => (
            <button
              key={id}
              type="button"
              aria-current={section === id ? "page" : undefined}
              onClick={() => setSection(id)}
              className={cn(
                "flex h-7 items-center gap-2 rounded-md px-2 text-left text-[13.5px] text-sidebar-foreground hover:bg-accent",
                section === id && "bg-selected text-foreground",
              )}
            >
              <Icon className="size-4 text-muted-foreground" aria-hidden />
              {label}
            </button>
          ))}
        </nav>

        <div className="min-h-0 overflow-y-auto px-8 py-7">
          <DialogDescription className="sr-only">
            Forma preferences. Changes are saved automatically.
          </DialogDescription>
          {section === "appearance" && <AppearanceSection />}
          {section === "workspace" && <WorkspaceSection />}
          {section === "ai" && (
            <PendingSection title="AI">
              Connecting OpenAI, Anthropic, or a local model through Ollama or LM Studio comes in a
              later build. AI is optional, and Forma will always tell you before your notes are sent
              anywhere.
            </PendingSection>
          )}
          {section === "data" && (
            <PendingSection title="Data">
              Export, backup and restore come in a later build.
            </PendingSection>
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}

function AppearanceSection() {
  const theme = useSettingsStore((s) => s.settings.appearance.theme);
  const setTheme = useSettingsStore((s) => s.setTheme);
  const readOnlyReason = useSettingsStore((s) => s.readOnlyReason);

  return (
    <section aria-labelledby="settings-appearance">
      <h2 id="settings-appearance" className="text-[15px] font-semibold">
        Appearance
      </h2>
      <div className="mt-5 flex items-center justify-between gap-6 border-t pt-4">
        <div>
          <p id="theme-label" className="text-[14px]">
            Theme
          </p>
          <p className="text-[13px] text-muted-foreground">
            Follow your Mac, or always use light or dark.
          </p>
        </div>
        <ToggleGroup
          type="single"
          variant="outline"
          size="sm"
          aria-labelledby="theme-label"
          value={theme}
          onValueChange={(value) => value && setTheme(value as ThemePreference)}
        >
          {THEMES.map(({ value, label }) => (
            <ToggleGroupItem
              key={value}
              value={value}
              className="px-3 text-[13px] data-[state=on]:bg-selected data-[state=on]:text-foreground"
            >
              {label}
            </ToggleGroupItem>
          ))}
        </ToggleGroup>
      </div>
      {readOnlyReason && (
        <p className="selectable mt-4 text-[13px] text-muted-foreground">{readOnlyReason}</p>
      )}
    </section>
  );
}

function WorkspaceSection() {
  const info = useWorkspaceStore((s) => s.info);
  const { choose, reportError } = useWorkspaceStore.getState();
  return (
    <section aria-labelledby="settings-workspace">
      <h2 id="settings-workspace" className="text-[15px] font-semibold">
        Workspace
      </h2>
      <div className="mt-5 border-t pt-4">
        <p className="text-[14px]">Location</p>
        <p className="selectable mt-0.5 text-[13px] break-all text-muted-foreground">
          {info?.root ?? "No workspace is open."}
        </p>
        {info?.read_only_reason && (
          <p className="mt-2 text-[13px] text-destructive">{info.read_only_reason}</p>
        )}
        <div className="mt-3 flex gap-2">
          <SettingsButton onClick={() => void choose()}>
            {info ? "Open another folder…" : "Choose a folder…"}
          </SettingsButton>
          {info && (
            <SettingsButton onClick={() => ipc.revealWorkspace().catch(reportError)}>
              Show in Finder
            </SettingsButton>
          )}
        </div>
        <p className="mt-4 text-[13px] leading-5 text-muted-foreground">
          Pages are Markdown files in this folder, arranged like your page tree. Forma keeps its own
          data (history, trash, order) in the hidden .forma folder inside it.
        </p>
      </div>
    </section>
  );
}

function SettingsButton({ onClick, children }: { onClick: () => void; children: ReactNode }) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="rounded-md px-3 py-1.5 text-[13px] text-foreground shadow-popover hover:bg-accent"
    >
      {children}
    </button>
  );
}

function PendingSection({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section>
      <h2 className="text-[15px] font-semibold">{title}</h2>
      <p className="selectable mt-5 border-t pt-4 text-[13.5px] leading-6 text-muted-foreground">
        {children}
      </p>
    </section>
  );
}
