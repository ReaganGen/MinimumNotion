import { Kbd } from "@/components/ui/kbd";
import { MENU_COMMANDS, SHORTCUTS } from "@/features/commands/commands";

/**
 * The main column. Until documents exist (Phase 2) it shows an honest welcome
 * state: what this build does, and the shortcuts that already work.
 */
export function EditorArea() {
  return (
    <div className="min-h-0 flex-1 overflow-y-auto">
      <div className="mx-auto w-full max-w-(--reading-width) px-12 pt-[12vh] pb-24">
        <h1 className="text-[40px] leading-tight font-bold tracking-tight text-foreground">
          Welcome to Forma
        </h1>
        <p className="mt-2 text-base text-muted-foreground">A quieter place to think.</p>

        <p className="selectable mt-8 text-[15px] leading-7 text-foreground/80">
          This build is the app shell: layout, themes, menus and settings. Pages, the editor and
          search come in the next builds, so there's nothing to write in yet.
        </p>

        <h2 className="mt-10 text-xs font-medium text-faint-foreground">Keyboard shortcuts</h2>
        <dl className="mt-2 max-w-xs divide-y divide-border">
          {MENU_COMMANDS.map((id) => (
            <div key={id} className="flex items-center justify-between py-1.5 text-[14px]">
              <dt className="text-muted-foreground">{SHORTCUTS[id].label}</dt>
              <dd>
                <Kbd className="text-muted-foreground">{SHORTCUTS[id].keys}</Kbd>
              </dd>
            </div>
          ))}
        </dl>
      </div>
    </div>
  );
}
