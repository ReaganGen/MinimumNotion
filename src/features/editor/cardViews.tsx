// Live views of file cards and bookmarks (ADR-049). Saving goes through
// cards.ts; these only show the card and run its buttons through Rust.
import { NodeViewWrapper, type ReactNodeViewProps, ReactNodeViewRenderer } from "@tiptap/react";
import { FileText, FolderSearch, Globe, Pencil } from "lucide-react";
import { useEffect, useState } from "react";
import { useWorkspaceStore } from "@/features/workspace/workspaceStore";
import type { AttachmentInfo } from "@/lib/ipc/bindings/AttachmentInfo";
import { ipc } from "@/lib/ipc/client";
import { bookmarkAttrs, editBookmark } from "./bookmarkDialog";
import { Attachment, Bookmark, unescapeMarkdown } from "./cards";
import { extensionOf, formatSize, referenceName } from "./media";

function report(error: unknown) {
  useWorkspaceStore.getState().reportError(error);
}

function AttachmentView({ node, extension, selected }: ReactNodeViewProps) {
  const pageId: string = extension.options.pageId;
  const src = String(node.attrs.src ?? "");
  const name = unescapeMarkdown(String(node.attrs.label ?? "")) || referenceName(src);
  const [info, setInfo] = useState<AttachmentInfo | "missing" | null>(null);

  useEffect(() => {
    let live = true;
    ipc.attachmentInfo(pageId, src).then(
      (found) => live && setInfo(found),
      () => live && setInfo("missing"),
    );
    return () => {
      live = false;
    };
  }, [pageId, src]);

  const type = extensionOf(src).toUpperCase();
  const meta =
    info === "missing"
      ? "File not found"
      : [type, info ? formatSize(info.size) : null].filter(Boolean).join(" · ");
  const missing = info === "missing";

  return (
    <NodeViewWrapper
      as="span"
      className="file-card"
      data-selected={selected || undefined}
      data-missing={missing || undefined}
      contentEditable={false}
    >
      <FileText className="file-card-icon" aria-hidden />
      <span className="file-card-text">
        <span className="file-card-name">{name}</span>
        <span className="file-card-meta">{meta}</span>
      </span>
      <button
        type="button"
        className="file-card-button"
        disabled={missing}
        onClick={() => ipc.openAttachment(pageId, src).catch(report)}
      >
        Open
      </button>
      <button
        type="button"
        className="file-card-button"
        disabled={missing}
        aria-label="Show in Finder"
        title="Show in Finder"
        onClick={() => ipc.revealAttachment(pageId, src).catch(report)}
      >
        <FolderSearch className="size-4" aria-hidden />
      </button>
    </NodeViewWrapper>
  );
}

/** `example.com/path`, for the card. */
function shortUrl(href: string): string {
  try {
    const url = new URL(href);
    return `${url.host}${url.pathname === "/" ? "" : url.pathname}`;
  } catch {
    return href;
  }
}

function BookmarkView({ node, editor, selected, updateAttributes }: ReactNodeViewProps) {
  const href = String(node.attrs.href ?? "");
  const title = unescapeMarkdown(String(node.attrs.label ?? "")) || shortUrl(href);
  const description = unescapeMarkdown(String(node.attrs.description ?? ""));

  async function edit() {
    const fields = await editBookmark({ url: href, title, description });
    const attrs = fields && bookmarkAttrs(fields);
    if (attrs) updateAttributes(attrs);
  }

  return (
    <NodeViewWrapper
      as="span"
      className="bookmark-card"
      data-selected={selected || undefined}
      contentEditable={false}
    >
      <button
        type="button"
        className="bookmark-main"
        title={`Open ${href} in the browser`}
        onClick={() => ipc.openExternal(href).catch(report)}
      >
        <span className="bookmark-title">{title}</span>
        {description && <span className="bookmark-description">{description}</span>}
        <span className="bookmark-url">
          <Globe className="size-3.5" aria-hidden />
          {shortUrl(href)}
        </span>
      </button>
      {editor.isEditable && (
        <button
          type="button"
          className="bookmark-edit"
          aria-label="Edit bookmark"
          title="Edit bookmark"
          onClick={() => void edit()}
        >
          <Pencil className="size-3.5" aria-hidden />
        </button>
      )}
    </NodeViewWrapper>
  );
}

export const LiveAttachment = Attachment.extend({
  addOptions() {
    return { pageId: "" };
  },
  addNodeView() {
    return ReactNodeViewRenderer(AttachmentView, { as: "span" });
  },
});

export const LiveBookmark = Bookmark.extend({
  addNodeView() {
    return ReactNodeViewRenderer(BookmarkView, { as: "span" });
  },
});
