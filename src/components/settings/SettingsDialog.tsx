import type { LucideIcon } from "lucide-react";
import { Database, Folder, Palette, Sparkles } from "lucide-react";
import { type ReactNode, useState } from "react";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { useSettingsStore } from "@/features/settings/settingsStore";
import type { ThemePreference } from "@/lib/ipc/bindings/ThemePreference";
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
          {section === "workspace" && (
            <PendingSection title="Workspace">
              Choosing and opening a workspace folder comes in the next build. Your pages will be
              plain Markdown files in a folder you pick.
            </PendingSection>
          )}
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
  const readOnly = useSettingsStore((s) => s.readOnly);

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
      {readOnly && (
        <p className="mt-4 text-[13px] text-muted-foreground">
          These settings were saved by a newer version of Forma, so changes made here won't be kept.
        </p>
      )}
    </section>
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
