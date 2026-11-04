import { AlertTriangle, Info, X } from "lucide-react";
import { type KeyboardEvent, useCallback, useEffect, useRef, useState } from "react";
import { CollectionView } from "@/components/collections/CollectionView";
import { EntryProperties } from "@/components/collections/EntryProperties";
import { normalizeMarkdown, roundTripMarkdown } from "@/features/editor/markdown";
import type { Conflict } from "@/features/editor/saveController";
import { registerFlusher, useWorkspaceStore } from "@/features/workspace/workspaceStore";
import type { PageDocument } from "@/lib/ipc/bindings/PageDocument";
import { ipc } from "@/lib/ipc/client";
import { Backlinks } from "./Backlinks";
import { FolderView } from "./FolderView";
import { PageEditor, type PageEditorHandle } from "./PageEditor";

const TITLE_SAVE_DELAY_MS = 600;

/** True if saving this body unchanged would rewrite parts of it (ARCHITECTURE §6.3). */
function wouldReformat(body: string): boolean {
  const trimmed = body.replace(/^\n+/, "");
  if (trimmed.trim() === "") return false;
  try {
    return roundTripMarkdown(trimmed) !== normalizeMarkdown(trimmed);
  } catch {
    return true;
  }
}

export function PageView({ pageId }: { pageId: string }) {
  const [page, setPage] = useState<PageDocument | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [conflict, setConflict] = useState<Conflict | null>(null);
  const [reformatNotice, setReformatNotice] = useState(false);
  const editorRef = useRef<PageEditorHandle>(null);
  const externalChange = useWorkspaceStore((s) => s.externalChange);
  const activeRemoved = useWorkspaceStore((s) => s.activeRemoved);
  const workspaceReadOnly = useWorkspaceStore((s) => s.info?.read_only_reason ?? null);
  const { setBreadcrumbs, reportError, openPage } = useWorkspaceStore.getState();

  const publishCrumbs = useCallback(
    (doc: PageDocument) =>
      setBreadcrumbs([...doc.ancestors, { id: doc.id, title: doc.title, icon: doc.icon }]),
    [setBreadcrumbs],
  );

  // Load the page.
  useEffect(() => {
    let cancelled = false;
    setPage(null);
    setLoadError(null);
    setConflict(null);
    ipc
      .getPage(pageId)
      .then((doc) => {
        if (cancelled) return;
        setPage(doc);
        publishCrumbs(doc);
        setReformatNotice(doc.status.state === "ok" && wouldReformat(doc.body));
      })
      .catch(
        (error: unknown) =>
          !cancelled && setLoadError(error instanceof Error ? error.message : String(error)),
      );
    return () => {
      cancelled = true;
    };
  }, [pageId, publishCrumbs]);

  const enterConflict = useCallback(
    (next: Conflict) => {
      editorRef.current?.pause();
      setConflict(next);
      // Keep the unsaved text on disk right away, whatever happens next.
      ipc.writeRecovery(pageId, next.mine, "conflict").catch(reportError);
    },
    [pageId, reportError],
  );

  // Something changed on disk: reload, or flag a conflict if there are unsaved edits.
  // biome-ignore lint/correctness/useExhaustiveDependencies: runs once per change notification
  useEffect(() => {
    if (externalChange === 0 || !page || conflict) return;
    const editor = editorRef.current;
    ipc
      .getPage(pageId)
      .then((fresh) => {
        setPage((current) => (current ? { ...fresh, body: current.body } : fresh));
        publishCrumbs(fresh);
        if (!editor || fresh.body_rev === editor.revision()) return; // metadata only
        if (editor.isDirty()) {
          enterConflict({
            mine: editor.markdown(),
            currentRev: fresh.body_rev,
            theirs: fresh.body,
          });
        } else {
          editor.replaceContent(fresh.body, fresh.body_rev);
        }
      })
      .catch(() => {
        // The page is gone; `activeRemoved` handles that.
      });
  }, [externalChange]);

  // The page's file went away outside Diaspar: close it, unless that would lose edits.
  // biome-ignore lint/correctness/useExhaustiveDependencies: runs once per removal notification
  useEffect(() => {
    if (activeRemoved === 0) return;
    const editor = editorRef.current;
    if (conflict) {
      setConflict({ ...conflict, currentRev: null, theirs: null });
    } else if (editor?.isDirty()) {
      enterConflict({ mine: editor.markdown(), currentRev: null, theirs: null });
    } else {
      openPage(null);
    }
  }, [activeRemoved]);

  async function resolve(choice: Resolution) {
    const editor = editorRef.current;
    if (!editor || !page || !conflict) return;
    const mine = editor.markdown();
    try {
      if (conflict.theirs === null) {
        // The file is gone, so "mine" makes it again, as a new page.
        if (choice === "mine") {
          const restored = await ipc
            .createPage(page.parent_id, page.title)
            .catch(() => ipc.createPage(null, page.title));
          const fresh = await ipc.getPage(restored.id);
          await ipc.savePage(restored.id, fresh.body_rev, mine);
          await ipc.discardRecovery(pageId);
          openPage(restored.id);
        } else {
          await ipc.discardRecovery(pageId);
          openPage(null);
        }
        return;
      }
      if (choice === "mine") {
        const outcome = await ipc.savePageOverConflict(pageId, mine);
        if (outcome.outcome !== "saved") throw new Error("The page changed again. Try once more.");
        editor.resume(outcome.body_rev, editor.markdown() !== mine);
      } else {
        if (choice === "both") {
          const day = new Date().toISOString().slice(0, 10);
          const copy = await ipc.createPage(page.parent_id, `${page.title} (conflict ${day})`);
          const fresh = await ipc.getPage(copy.id);
          await ipc.savePage(copy.id, fresh.body_rev, mine);
        } else {
          await ipc.snapshotText(pageId, mine, "conflict");
        }
        const current = await ipc.getPage(pageId);
        editor.replaceContent(current.body, current.body_rev);
      }
      await ipc.discardRecovery(pageId);
      setConflict(null);
    } catch (error) {
      reportError(error);
    }
  }

  if (loadError) {
    return (
      <Centered>
        <p className="text-[14px] text-muted-foreground">{loadError}</p>
        <button
          type="button"
          onClick={() => openPage(null)}
          className="mt-3 text-[13px] text-link hover:underline"
        >
          Close this page
        </button>
      </Centered>
    );
  }
  if (!page) return <div className="flex-1" />;
  if (page.kind === "folder") return <FolderView page={page} editable={!workspaceReadOnly} />;

  const invalid = page.status.state === "invalid_frontmatter" ? page.status.message : null;
  const readOnlyReason =
    workspaceReadOnly ??
    (invalid &&
      `This page's frontmatter can't be read (${invalid}). Fix it in a text editor to edit it here.`);
  const editable = !readOnlyReason;

  return (
    <div className="min-h-0 flex-1 overflow-y-auto">
      <article
        className="mx-auto w-full max-w-(--reading-width) px-12 pt-14 pb-[30vh]"
        // A collection's table follows its text, so the text area stays short.
        data-compact={page.is_collection || undefined}
      >
        {readOnlyReason && <Banner tone="warning">{readOnlyReason}</Banner>}
        {conflict && <ConflictBanner conflict={conflict} onResolve={resolve} />}
        {reformatNotice && !conflict && editable && (
          <Banner tone="info" onDismiss={() => setReformatNotice(false)}>
            Some of this page's Markdown is written in a form Diaspar normalizes. When you edit it,
            Diaspar will rewrite those parts. The original is kept in the page's history.
          </Banner>
        )}
        {/* No keys needed: EditorArea keys the whole view by page. */}
        <TitleField
          page={page}
          editable={editable && !conflict}
          onEnter={() => editorRef.current?.focus()}
          onRenamed={(title) => {
            setPage((p) => (p ? { ...p, title } : p));
            publishCrumbs({ ...page, title });
          }}
        />
        {page.collection_id && <EntryProperties pageId={page.id} editable={editable} />}
        <PageEditor
          ref={editorRef}
          page={page}
          editable={editable && !conflict}
          onConflict={enterConflict}
        />
        {page.is_collection && <CollectionView id={page.id} editable={editable} />}
        <Backlinks pageId={page.id} />
      </article>
    </div>
  );
}

function TitleField({
  page,
  editable,
  onEnter,
  onRenamed,
}: {
  page: PageDocument;
  editable: boolean;
  onEnter: () => void;
  onRenamed: (title: string) => void;
}) {
  const [value, setValue] = useState(page.title === "Untitled" ? "" : page.title);
  const saved = useRef<string | null>(page.title);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const reportError = useWorkspaceStore((s) => s.reportError);
  const latest = useRef(value);
  latest.current = value;
  const onRenamedRef = useRef(onRenamed);
  onRenamedRef.current = onRenamed;
  const fieldRef = useRef<HTMLTextAreaElement>(null);

  // A page the user just made starts with the cursor in its title.
  useEffect(() => {
    if (useWorkspaceStore.getState().focusTitleOf !== page.id) return;
    useWorkspaceStore.setState({ focusTitleOf: null });
    fieldRef.current?.focus();
  }, [page.id]);

  const save = useCallback(async () => {
    clearTimeout(timer.current);
    const title = latest.current.trim() || "Untitled";
    if (title === saved.current) return;
    saved.current = title;
    try {
      await ipc.setTitle(page.id, title);
      onRenamedRef.current(title);
    } catch (error) {
      saved.current = null;
      reportError(error);
    }
  }, [page.id, reportError]);

  useEffect(() => registerFlusher(save), [save]);
  useEffect(() => () => void save(), [save]);

  function onKeyDown(event: KeyboardEvent<HTMLTextAreaElement>) {
    if (event.key === "Enter") {
      event.preventDefault();
      void save();
      onEnter();
    }
  }

  return (
    <textarea
      ref={fieldRef}
      aria-label="Page title"
      placeholder="Untitled"
      rows={1}
      readOnly={!editable}
      value={value}
      onChange={(e) => {
        setValue(e.target.value.replace(/\n/g, " "));
        clearTimeout(timer.current);
        timer.current = setTimeout(() => void save(), TITLE_SAVE_DELAY_MS);
      }}
      onBlur={() => void save()}
      onKeyDown={onKeyDown}
      className="selectable field-sizing-content mb-4 w-full resize-none bg-transparent text-[40px] leading-tight font-bold tracking-tight text-foreground outline-none placeholder:text-faint-foreground"
    />
  );
}

type Resolution = "mine" | "theirs" | "both" | "discard";

function ConflictBanner({
  conflict,
  onResolve,
}: {
  conflict: Conflict;
  onResolve: (choice: Resolution) => void;
}) {
  const deleted = conflict.theirs === null;
  return (
    <Banner tone="warning">
      <p className="font-medium text-foreground">
        {deleted
          ? "This page's file was deleted outside Diaspar."
          : "This page was changed outside Diaspar while you were editing."}
      </p>
      <p className="mt-0.5">
        Your text is safe: it's kept in a recovery copy until you choose. Saving is paused.
      </p>
      <div className="mt-2 flex flex-wrap gap-2">
        <BannerButton onClick={() => onResolve("mine")}>
          {deleted ? "Restore with my text" : "Keep mine"}
        </BannerButton>
        {!deleted && <BannerButton onClick={() => onResolve("theirs")}>Use theirs</BannerButton>}
        {!deleted && <BannerButton onClick={() => onResolve("both")}>Keep both</BannerButton>}
        {deleted && (
          <BannerButton onClick={() => onResolve("discard")}>Discard my text</BannerButton>
        )}
      </div>
    </Banner>
  );
}

function Banner({
  tone,
  children,
  onDismiss,
}: {
  tone: "info" | "warning";
  children: React.ReactNode;
  onDismiss?: () => void;
}) {
  const Icon = tone === "warning" ? AlertTriangle : Info;
  return (
    <div
      role="status"
      className="mb-6 flex gap-2 rounded-md bg-muted px-3 py-2.5 text-[13px] leading-5 text-muted-foreground"
    >
      <Icon
        className={
          tone === "warning" ? "mt-0.5 size-4 shrink-0 text-destructive" : "mt-0.5 size-4 shrink-0"
        }
        aria-hidden
      />
      <div className="selectable min-w-0 flex-1">{children}</div>
      {onDismiss && (
        <button
          type="button"
          aria-label="Dismiss"
          onClick={onDismiss}
          className="self-start rounded-sm p-0.5 hover:bg-accent"
        >
          <X className="size-3.5" aria-hidden />
        </button>
      )}
    </div>
  );
}

function BannerButton({ onClick, children }: { onClick: () => void; children: React.ReactNode }) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="rounded-md bg-background px-2.5 py-1 text-[12.5px] text-foreground shadow-popover hover:bg-accent"
    >
      {children}
    </button>
  );
}

function Centered({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex flex-1 flex-col items-center justify-center px-10 text-center">
      {children}
    </div>
  );
}
