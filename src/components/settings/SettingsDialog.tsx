import type { LucideIcon } from "lucide-react";
import { Database, Folder, Palette, Sparkles } from "lucide-react";
import { type ReactNode, useState } from "react";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { useSettingsStore } from "@/features/settings/settingsStore";
import { type SettingsSection, useUiStore } from "@/features/ui/uiStore";
import { flushAllEdits, useWorkspaceStore } from "@/features/workspace/workspaceStore";
import type { ArchiveResult } from "@/lib/ipc/bindings/ArchiveResult";
import type { ThemePreference } from "@/lib/ipc/bindings/ThemePreference";
import { ipc } from "@/lib/ipc/client";
import { cn } from "@/lib/utils";
import { AiSection } from "./AiSection";

const SECTIONS: { id: SettingsSection; label: string; icon: LucideIcon }[] = [
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
  const section = useUiStore((s) => s.settingsSection);
  const setSection = useUiStore((s) => s.setSettingsSection);

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
          {section === "ai" && <AiSection />}
          {section === "data" && <DataSection />}
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

function SettingsButton({
  onClick,
  children,
  disabled,
}: {
  onClick: () => void;
  children: ReactNode;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      className="rounded-md px-3 py-1.5 text-[13px] text-foreground shadow-popover hover:bg-accent disabled:opacity-50"
    >
      {children}
    </button>
  );
}

function DataSection() {
  const hasWorkspace = useWorkspaceStore((s) => s.status === "open");
  const [state, setState] = useState<"idle" | "working" | "done" | string>("idle");
  const [archive, setArchive] = useState<{
    busy: boolean;
    message: string | null;
    error?: boolean;
  }>({
    busy: false,
    message: null,
  });
  const [log, setLog] = useState<string | null>(null);
  const [copied, setCopied] = useState<string | null>(null);

  async function rebuild() {
    setState("working");
    try {
      await ipc.rebuildIndex();
      setState("done");
    } catch (error) {
      setState(error instanceof Error ? error.message : String(error));
    }
  }

  async function run(action: () => Promise<string | null>) {
    setArchive({ busy: true, message: null });
    try {
      // What's being typed reaches the disk first.
      await flushAllEdits();
      setArchive({ busy: false, message: await action() });
    } catch (error) {
      setArchive({ busy: false, message: (error as Error).message, error: true });
    }
  }

  const described = (verb: string, result: ArchiveResult | null) =>
    result &&
    `${verb} ${result.files.toLocaleString("en-US")} files (${formatBytes(result.bytes)}) to ${result.path}.`;

  async function copyDiagnostics() {
    try {
      const report = await ipc.diagnostics();
      await copyText(report);
      setCopied("Copied. Paste it into a bug report.");
    } catch (error) {
      setCopied(`Couldn't copy: ${(error as Error).message}`);
    }
  }

  return (
    <section aria-labelledby="settings-data">
      <h2 id="settings-data" className="text-[15px] font-semibold">
        Data
      </h2>
      <div className="mt-5 border-t pt-4">
        <p className="text-[14px]">Search index</p>
        <p className="mt-0.5 text-[13px] leading-5 text-muted-foreground">
          Search, backlinks and collections use an index Forma keeps outside your workspace. It's
          rebuilt from your files alone, so rebuilding is always safe.
        </p>
        <div className="mt-3 flex items-center gap-3">
          <SettingsButton
            onClick={() => void rebuild()}
            disabled={!hasWorkspace || state === "working"}
          >
            {state === "working" ? "Rebuilding…" : "Rebuild search index"}
          </SettingsButton>
          <span role="status" className="text-[13px] text-muted-foreground">
            {state === "done" ? "Rebuilt." : state === "idle" || state === "working" ? "" : state}
          </span>
        </div>
      </div>

      <div className="mt-5 border-t pt-4">
        <p className="text-[14px]">Export and backup</p>
        <dl className="mt-1 grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-[13px] leading-5">
          <dt className="text-foreground">Export</dt>
          <dd className="text-muted-foreground">
            A copy of your pages and attachments as plain files, without Forma's own data.
          </dd>
          <dt className="text-foreground">Back up</dt>
          <dd className="text-muted-foreground">
            A zip of the whole workspace, with page history, the trash and AI change records.
          </dd>
          <dt className="text-foreground">Restore</dt>
          <dd className="text-muted-foreground">
            Unzips a backup into a new folder and opens it. Nothing that exists is replaced.
          </dd>
        </dl>
        <div className="mt-3 flex flex-wrap items-center gap-2">
          <SettingsButton
            disabled={!hasWorkspace || archive.busy}
            onClick={() => void run(async () => described("Exported", await ipc.exportWorkspace()))}
          >
            Export…
          </SettingsButton>
          <SettingsButton
            disabled={!hasWorkspace || archive.busy}
            onClick={() =>
              void run(async () => described("Backed up", await ipc.backupWorkspace()))
            }
          >
            Back up…
          </SettingsButton>
          <SettingsButton
            disabled={archive.busy}
            onClick={() =>
              void run(async () => {
                const info = await useWorkspaceStore.getState().restoreBackup();
                return info ? `Restored and opened ${info.root}.` : null;
              })
            }
          >
            Restore…
          </SettingsButton>
        </div>
        {(archive.busy || archive.message) && (
          <p
            role="status"
            className={cn(
              "mt-2 text-[13px] leading-5 break-words",
              archive.error ? "text-destructive" : "text-muted-foreground",
            )}
          >
            {archive.busy ? "Working…" : archive.message}
          </p>
        )}
      </div>

      <div className="mt-5 border-t pt-4">
        <p className="text-[14px]">Diagnostics</p>
        <p className="mt-0.5 text-[13px] leading-5 text-muted-foreground">
          “Copy diagnostics” copies Forma's version, macOS version, a settings summary and recent
          log lines with every file path and quoted name removed. It never includes note text, page
          titles, server addresses or keys.
        </p>
        <div className="mt-3 flex flex-wrap items-center gap-2">
          <SettingsButton
            onClick={() =>
              log === null
                ? void ipc
                    .readLog()
                    .then((text) => setLog(text || "The log is empty."))
                    .catch((e) => setLog(`The log can't be read: ${(e as Error).message}`))
                : setLog(null)
            }
          >
            {log === null ? "Show log" : "Hide log"}
          </SettingsButton>
          <SettingsButton onClick={() => void copyDiagnostics()}>Copy diagnostics</SettingsButton>
          {copied && (
            <span role="status" className="text-[13px] text-muted-foreground">
              {copied}
            </span>
          )}
        </div>
        {log !== null && (
          <pre
            role="log"
            aria-label="Log"
            className="selectable mt-3 max-h-64 overflow-auto rounded-md bg-muted px-3 py-2 font-mono text-[11.5px] leading-4 whitespace-pre-wrap break-all"
          >
            {log}
          </pre>
        )}
      </div>
    </section>
  );
}

function formatBytes(bytes: number): string {
  if (bytes < 1024 * 1024) return `${Math.max(1, Math.round(bytes / 1024))} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

/** The clipboard API, or a selected text area where it isn't allowed. */
async function copyText(text: string) {
  try {
    await navigator.clipboard.writeText(text);
  } catch {
    const area = document.createElement("textarea");
    area.value = text;
    document.body.append(area);
    area.select();
    const ok = document.execCommand("copy");
    area.remove();
    if (!ok) throw new Error("the clipboard isn't available");
  }
}
