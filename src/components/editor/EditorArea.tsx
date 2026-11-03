import { FolderOpen, Plus } from "lucide-react";
import { Kbd } from "@/components/ui/kbd";
import { SHORTCUTS } from "@/features/commands/commands";
import { ROOT, useWorkspaceStore } from "@/features/workspace/workspaceStore";
import { PageView } from "./PageView";

/** The main column: welcome screen, empty state, or the open page. */
export function EditorArea() {
  const status = useWorkspaceStore((s) => s.status);
  const activePageId = useWorkspaceStore((s) => s.activePageId);

  if (status === "loading") return <div className="flex-1" />;
  if (status === "none") return <Welcome />;
  if (activePageId) return <PageView key={activePageId} pageId={activePageId} />;
  return <NoPageOpen />;
}

function Welcome() {
  const error = useWorkspaceStore((s) => s.error);
  const { createDefault, choose } = useWorkspaceStore.getState();
  return (
    <div className="min-h-0 flex-1 overflow-y-auto">
      <div className="mx-auto w-full max-w-(--reading-width) px-12 pt-[14vh] pb-24">
        <h1 className="text-[40px] leading-tight font-bold tracking-tight text-foreground">
          Welcome to Forma
        </h1>
        <p className="mt-2 text-base text-muted-foreground">A quieter place to think.</p>
        <p className="selectable mt-8 max-w-xl text-[15px] leading-7 text-foreground/80">
          Your pages are plain Markdown files in a folder you choose, organized the way your page
          tree is. Nothing leaves your Mac, and you can open the files with any other app.
        </p>
        <div className="mt-8 flex flex-col gap-2 sm:flex-row">
          <WelcomeButton onClick={() => void createDefault()} primary>
            <Plus aria-hidden /> Create a workspace in Documents
          </WelcomeButton>
          <WelcomeButton onClick={() => void choose()}>
            <FolderOpen aria-hidden /> Open a folder…
          </WelcomeButton>
        </div>
        {error && (
          <p role="alert" className="selectable mt-5 text-[13.5px] text-destructive">
            {error}
          </p>
        )}
      </div>
    </div>
  );
}

function WelcomeButton({
  onClick,
  primary,
  children,
}: {
  onClick: () => void;
  primary?: boolean;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={
        primary
          ? "inline-flex h-9 items-center gap-2 rounded-md bg-primary px-4 text-[14px] text-primary-foreground hover:bg-primary/90 [&_svg]:size-4"
          : "inline-flex h-9 items-center gap-2 rounded-md px-4 text-[14px] text-foreground shadow-popover hover:bg-accent [&_svg]:size-4"
      }
    >
      {children}
    </button>
  );
}

function NoPageOpen() {
  const createPage = useWorkspaceStore((s) => s.createPage);
  const empty = useWorkspaceStore((s) => s.children[ROOT]?.length === 0);
  const readOnly = useWorkspaceStore((s) => s.info?.read_only_reason != null);
  return (
    <div className="flex flex-1 flex-col items-center justify-center px-10 pb-16 text-center">
      <p className="text-[14px] text-muted-foreground">
        Choose a page in the sidebar, or start a new one.
      </p>
      <button
        type="button"
        onClick={() => void createPage(null)}
        className="mt-3 inline-flex items-center gap-2 rounded-md px-3 py-1.5 text-[13.5px] text-foreground hover:bg-accent"
      >
        New page <Kbd className="text-muted-foreground">{SHORTCUTS["new-page"].keys}</Kbd>
      </button>
      {empty && !readOnly && (
        <button
          type="button"
          onClick={() => void useWorkspaceStore.getState().addGettingStarted()}
          className="mt-1 inline-flex items-center gap-2 rounded-md px-3 py-1.5 text-[13.5px] text-muted-foreground hover:bg-accent hover:text-foreground"
        >
          Add a getting-started page
        </button>
      )}
    </div>
  );
}
