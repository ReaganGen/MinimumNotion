// The bookmark dialog (BookmarkDialog.tsx): one at a time, opened by the "/"
// menu or a bookmark's Edit button, answered with what the user typed.
import { create } from "zustand";
import { bookmarkUrl, escapeLinkTitle } from "./cards";
import { escapeInlineText } from "./markdownRules";

export interface BookmarkFields {
  url: string;
  title: string;
  description: string;
}

interface BookmarkDialogState {
  request: { initial: BookmarkFields; resolve: (fields: BookmarkFields | null) => void } | null;
}

export const useBookmarkDialog = create<BookmarkDialogState>(() => ({ request: null }));

/** Opens the dialog; resolves with the fields, or `null` if it was cancelled. */
export function editBookmark(initial: BookmarkFields): Promise<BookmarkFields | null> {
  return new Promise((resolve) => {
    useBookmarkDialog.getState().request?.resolve(null);
    useBookmarkDialog.setState({ request: { initial, resolve } });
  });
}

export function answerBookmarkDialog(fields: BookmarkFields | null) {
  const request = useBookmarkDialog.getState().request;
  useBookmarkDialog.setState({ request: null });
  request?.resolve(fields);
}

/**
 * The bookmark node's attributes for what was typed, or `null` if the URL
 * isn't http or https. Without a title, the address is the link text, so
 * other apps never show an empty link.
 */
export function bookmarkAttrs(fields: BookmarkFields) {
  const href = bookmarkUrl(fields.url);
  if (!href) return null;
  const url = new URL(href);
  const title =
    fields.title.replace(/\s+/g, " ").trim() || `${url.host}${url.pathname}`.replace(/\/$/, "");
  return {
    label: escapeInlineText(title),
    href,
    description: escapeLinkTitle(fields.description),
  };
}
