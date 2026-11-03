import type { Editor } from "@tiptap/core";
import { EditorContent, useEditor } from "@tiptap/react";
import { BubbleMenu } from "@tiptap/react/menus";
import { Bold, Code, Italic, Link as LinkIcon, Sparkles, Strikethrough } from "lucide-react";
import {
  forwardRef,
  type ReactNode,
  useEffect,
  useImperativeHandle,
  useMemo,
  useRef,
  useState,
} from "react";
import { askAboutSelection, explainSelection } from "@/features/ai/actions";
import { selectedText, setActiveEditor } from "@/features/editor/activeEditor";
import { createEditorExtensions } from "@/features/editor/liveExtensions";
import { docToMarkdown } from "@/features/editor/markdown";
import { findTextRange, flashKey, useRevealStore } from "@/features/editor/reveal";
import { type Conflict, SaveController } from "@/features/editor/saveController";
import { useSettingsStore } from "@/features/settings/settingsStore";
import { registerFlusher, useWorkspaceStore } from "@/features/workspace/workspaceStore";
import type { PageDocument } from "@/lib/ipc/bindings/PageDocument";
import { ipc } from "@/lib/ipc/client";
import { cn } from "@/lib/utils";

export interface PageEditorHandle {
  markdown: () => string;
  /** The body revision the next save will be based on. */
  revision: () => string | null;
  isDirty: () => boolean;
  /** Replaces the content with what's on disk (no save follows). */
  replaceContent: (body: string, bodyRev: string) => void;
  /** Stops autosave while a conflict is open. */
  pause: () => void;
  /** Continues autosave after a conflict was resolved. */
  resume: (bodyRev: string | null, stillDirty: boolean) => void;
  flush: () => Promise<void>;
  focus: () => void;
}

interface PageEditorProps {
  page: PageDocument;
  editable: boolean;
  onConflict: (conflict: Conflict) => void;
}

const PAGE_LINK = /^forma:\/\/page\/([0-9a-f-]{36})/i;

function imageFiles(list: FileList | null | undefined): File[] {
  return Array.from(list ?? []).filter((file) => file.type.startsWith("image/"));
}

export const PageEditor = forwardRef<PageEditorHandle, PageEditorProps>(function PageEditor(
  { page, editable, onConflict },
  ref,
) {
  const setSaveStatus = useWorkspaceStore((s) => s.setSaveStatus);
  const onConflictRef = useRef(onConflict);
  onConflictRef.current = onConflict;
  // Set when unsaved text was put in a recovery buffer; a later successful
  // save makes that buffer stale.
  const keptRecovery = useRef(false);
  // Created by the effect below, once per mount. The latest body revision is
  // kept apart so a controller made by a re-run effect starts from it.
  const controllerRef = useRef<SaveController | null>(null);
  const revisionRef = useRef(page.body_rev);
  const editorRef = useRef<Editor | null>(null);

  function adopt(bodyRev: string) {
    revisionRef.current = bodyRev;
    controllerRef.current?.reset(bodyRev);
  }

  async function insertImages(files: File[], at?: number) {
    const editor = editorRef.current;
    const controller = controllerRef.current;
    if (!editor || !controller || files.length === 0) return;
    try {
      // Adding the first attachment can turn the page file into a folder,
      // so save first and pick up any change Forma made to the body.
      await controller.flush();
      for (const file of files) {
        const src = await ipc.importAttachment(page.id, file);
        const fresh = await ipc.getPage(page.id);
        if (fresh.body_rev !== controller.revision && !controller.isDirty) {
          editor.commands.setContent(fresh.body, { contentType: "markdown", emitUpdate: false });
          adopt(fresh.body_rev);
        }
        const content = { type: "image", attrs: { src, alt: file.name.replace(/\.[^.]+$/, "") } };
        if (at === undefined) editor.chain().focus().insertContent(content).run();
        else editor.chain().focus().insertContentAt(at, content).run();
      }
    } catch (error) {
      useWorkspaceStore.getState().reportError(error);
    }
  }

  const extensions = useMemo(() => createEditorExtensions(page.id), [page.id]);
  const editor = useEditor(
    {
      extensions,
      content: page.body,
      contentType: "markdown",
      editable,
      shouldRerenderOnTransaction: false,
      onUpdate: ({ editor, transaction }) => {
        // Only document changes count: Tiptap also emits updates that change
        // nothing, and opening a page must never write it (architecture rule 14).
        if (!transaction.docChanged) return;
        controllerRef.current?.markDirty(() => docToMarkdown(editor));
      },
      // Leaving the text (for the title, the sidebar, another app) saves at once.
      onBlur: () => void controllerRef.current?.flush(),
      editorProps: {
        attributes: { class: "forma-editor", "aria-label": "Page content" },
        handlePaste: (_view, event) => {
          const files = imageFiles(event.clipboardData?.files);
          if (files.length === 0) return false;
          event.preventDefault();
          void insertImages(files);
          return true;
        },
        handleDrop: (view, event, _slice, moved) => {
          const files = imageFiles(event.dataTransfer?.files);
          if (moved || files.length === 0) return false;
          event.preventDefault();
          const at = view.posAtCoords({ left: event.clientX, top: event.clientY })?.pos;
          void insertImages(files, at);
          return true;
        },
        handleClick: (view, pos, event) => {
          if (!event.metaKey) return false;
          const link = view.state.doc
            .resolve(pos)
            .marks()
            .find((mark) => mark.type.name === "link");
          const href: string | undefined = link?.attrs.href;
          if (!href) return false;
          const pageLink = PAGE_LINK.exec(href);
          if (pageLink?.[1]) useWorkspaceStore.getState().openPage(pageLink[1]);
          else ipc.openExternal(href).catch(useWorkspaceStore.getState().reportError);
          return true;
        },
      },
    },
    [page.id],
  );
  editorRef.current = editor;

  useEffect(() => (editor ? setActiveEditor(page.id, editor) : undefined), [editor, page.id]);

  // A citation asked to show text in this page: scroll to it and highlight it.
  const reveal = useRevealStore((s) => s.target);
  const flashTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  useEffect(() => () => clearTimeout(flashTimer.current), []);
  useEffect(() => {
    if (!editor || !reveal || reveal.pageId !== page.id) return;
    useRevealStore.getState().done();
    const range = findTextRange(editor.state.doc, reveal.text);
    if (!range) return;
    editor.view.dispatch(editor.state.tr.setMeta(flashKey, range));
    const { node } = editor.view.domAtPos(range.from);
    (node instanceof Element ? node : node.parentElement)?.scrollIntoView({ block: "center" });
    clearTimeout(flashTimer.current);
    flashTimer.current = setTimeout(() => {
      if (!editor.isDestroyed) editor.view.dispatch(editor.state.tr.setMeta(flashKey, null));
    }, 2400);
  }, [editor, reveal, page.id]);

  useEffect(() => {
    // `false`: by default setEditable emits an update, which would look like an edit.
    editor?.setEditable(editable, false);
  }, [editor, editable]);

  useEffect(() => {
    // A controller per mount: React runs effects twice in development (and
    // again on hot reload), and reusing a disposed controller drops edits.
    const controller = new SaveController({
      baseRev: revisionRef.current,
      save: (body, base) => ipc.savePage(page.id, base, body),
      onStatus: (status, error) => setSaveStatus(status, error),
      onConflict: (conflict) => onConflictRef.current(conflict),
      onSaved: (bodyRev) => {
        revisionRef.current = bodyRev;
        if (!keptRecovery.current) return;
        keptRecovery.current = false;
        ipc.discardRecovery(page.id).catch(() => undefined);
      },
    });
    controllerRef.current = controller;
    setSaveStatus("saved");
    // Saves what's typed before the editor goes away (page switch, quit). If
    // that fails, the text is kept as a recovery buffer instead of being lost.
    const saveOrKeep = async () => {
      controller.freezeBody();
      await controller.flush();
      const body = controller.pendingBody();
      if (body === null) return;
      try {
        await ipc.writeRecovery(page.id, body, "unsaved");
        keptRecovery.current = true;
      } catch (error) {
        useWorkspaceStore.getState().reportError(error);
      }
    };
    const unregister = registerFlusher(saveOrKeep);
    return () => {
      unregister();
      if (controllerRef.current === controller) controllerRef.current = null;
      void saveOrKeep().finally(() => controller.dispose());
    };
  }, [setSaveStatus, page.id]);

  useImperativeHandle(ref, () => ({
    markdown: () => (editor ? docToMarkdown(editor) : page.body),
    revision: () => controllerRef.current?.revision ?? revisionRef.current,
    isDirty: () => controllerRef.current?.isDirty ?? false,
    replaceContent: (body, bodyRev) => {
      editor?.commands.setContent(body, { contentType: "markdown", emitUpdate: false });
      adopt(bodyRev);
    },
    pause: () => controllerRef.current?.pause(),
    resume: (bodyRev, stillDirty) => {
      if (bodyRev) revisionRef.current = bodyRev;
      controllerRef.current?.resume(bodyRev, stillDirty);
    },
    flush: async () => controllerRef.current?.flush(),
    focus: () => {
      // Focus now: Tiptap's focus command waits a frame outside WebKit, and
      // keys typed meanwhile would still go to the title.
      editor?.view.focus();
      editor?.commands.focus("start");
    },
  }));

  return (
    <>
      {editor && editable && <FormattingMenu editor={editor} page={page} />}
      <EditorContent editor={editor} />
    </>
  );
});

function FormattingMenu({ editor, page }: { editor: Editor; page: PageDocument }) {
  const [linking, setLinking] = useState(false);
  const [href, setHref] = useState("");
  const aiEnabled = useSettingsStore((s) => s.settings.ai.enabled);
  // The selection as the AI panel attaches it; the title is the current one.
  const selection = () => ({
    pageId: page.id,
    title: useWorkspaceStore.getState().breadcrumbs.at(-1)?.title ?? page.title,
    text: selectedText(editor),
  });

  function applyLink() {
    const url = href.trim();
    if (url) editor.chain().focus().extendMarkRange("link").setLink({ href: url }).run();
    else editor.chain().focus().extendMarkRange("link").unsetLink().run();
    setLinking(false);
  }

  return (
    <BubbleMenu editor={editor} className="forma-popover flex items-center gap-0.5 p-1">
      {linking ? (
        <input
          // biome-ignore lint/a11y/noAutofocus: opened by an explicit click on "Link"
          autoFocus
          aria-label="Link address"
          placeholder="Paste a link…"
          value={href}
          onChange={(e) => setHref(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") applyLink();
            if (e.key === "Escape") setLinking(false);
          }}
          className="h-7 w-64 rounded bg-transparent px-2 text-[13px] outline-none"
        />
      ) : (
        <>
          <MarkButton
            label="Bold"
            active={editor.isActive("bold")}
            onClick={() => editor.chain().focus().toggleBold().run()}
          >
            <Bold />
          </MarkButton>
          <MarkButton
            label="Italic"
            active={editor.isActive("italic")}
            onClick={() => editor.chain().focus().toggleItalic().run()}
          >
            <Italic />
          </MarkButton>
          <MarkButton
            label="Strikethrough"
            active={editor.isActive("strike")}
            onClick={() => editor.chain().focus().toggleStrike().run()}
          >
            <Strikethrough />
          </MarkButton>
          <MarkButton
            label="Inline code"
            active={editor.isActive("code")}
            onClick={() => editor.chain().focus().toggleCode().run()}
          >
            <Code />
          </MarkButton>
          <MarkButton
            label="Link"
            active={editor.isActive("link")}
            onClick={() => {
              setHref(editor.getAttributes("link").href ?? "");
              setLinking(true);
            }}
          >
            <LinkIcon />
          </MarkButton>
          {aiEnabled && (
            <>
              <span className="mx-0.5 h-4 w-px bg-border" aria-hidden />
              <TextButton
                label="Explain the selection with AI"
                onClick={() => explainSelection(selection())}
              >
                <Sparkles /> Explain
              </TextButton>
              <TextButton
                label="Ask AI about the selection"
                onClick={() => askAboutSelection(selection())}
              >
                Ask AI
              </TextButton>
            </>
          )}
        </>
      )}
    </BubbleMenu>
  );
}

function TextButton({
  label,
  onClick,
  children,
}: {
  label: string;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      title={label}
      onMouseDown={(e) => e.preventDefault()}
      onClick={onClick}
      className="inline-flex h-7 items-center gap-1 rounded px-1.5 text-[12.5px] text-muted-foreground hover:bg-accent hover:text-foreground [&_svg]:size-3.5"
    >
      {children}
    </button>
  );
}

function MarkButton({
  label,
  active,
  onClick,
  children,
}: {
  label: string;
  active: boolean;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      aria-pressed={active}
      onMouseDown={(e) => e.preventDefault()}
      onClick={onClick}
      className={cn(
        "inline-flex size-7 items-center justify-center rounded text-muted-foreground hover:bg-accent hover:text-foreground [&_svg]:size-4",
        active && "bg-selected text-foreground",
      )}
    >
      {children}
    </button>
  );
}
