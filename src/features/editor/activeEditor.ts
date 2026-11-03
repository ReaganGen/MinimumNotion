// The editor of the open page, for AI actions that need its selection or
// where its cursor is. Only PageEditor sets it.
import type { Editor } from "@tiptap/core";

let active: { pageId: string; editor: Editor } | null = null;

export function setActiveEditor(pageId: string, editor: Editor): () => void {
  const entry = { pageId, editor };
  active = entry;
  return () => {
    if (active === entry) active = null;
  };
}

/** The text of the block holding the cursor in `pageId`'s editor, if open. */
export function cursorBlockText(pageId: string): string | null {
  if (!active || active.pageId !== pageId || active.editor.isDestroyed) return null;
  const text = active.editor.state.selection.$from.parent.textContent.trim();
  return text || null;
}

/** The selected text, as plain text with line breaks between blocks. */
export function selectedText(editor: Editor): string {
  const { from, to } = editor.state.selection;
  return editor.state.doc.textBetween(from, to, "\n").trim();
}
