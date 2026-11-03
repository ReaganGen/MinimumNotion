// What page links and wikilinks point at right now (ARCHITECTURE §6.5).
// Editors ask for the IDs and texts they show; requests made in the same
// tick go to the backend as one batch, and a workspace change refreshes
// everything known.
import { create } from "zustand";
import type { PageLinkInfo } from "@/lib/ipc/bindings/PageLinkInfo";
import type { WikiResolution } from "@/lib/ipc/bindings/WikiResolution";
import { ipc } from "@/lib/ipc/client";

interface LinkStore {
  pages: Record<string, PageLinkInfo>;
  wikis: Record<string, WikiResolution>;
}

export const useLinkStore = create<LinkStore>()(() => ({ pages: {}, wikis: {} }));

const wantedPages = new Set<string>();
const wantedWikis = new Set<string>();
let scheduled = false;

function schedule() {
  if (scheduled) return;
  scheduled = true;
  queueMicrotask(flush);
}

function flush() {
  scheduled = false;
  const ids = [...wantedPages];
  const texts = [...wantedWikis];
  wantedPages.clear();
  wantedWikis.clear();
  if (ids.length > 0) {
    ipc
      .resolvePages(ids)
      .then((infos) => {
        const pages = { ...useLinkStore.getState().pages };
        for (const info of infos) pages[info.id] = info;
        useLinkStore.setState({ pages });
      })
      .catch(() => {
        // Links keep their saved labels until the next refresh.
      });
  }
  if (texts.length > 0) {
    ipc
      .resolveWikilinks(texts)
      .then((found) => {
        const wikis = { ...useLinkStore.getState().wikis };
        for (const resolution of found) wikis[resolution.text] = resolution;
        useLinkStore.setState({ wikis });
      })
      .catch(() => {});
  }
}

export function requestPage(id: string) {
  if (id in useLinkStore.getState().pages) return;
  wantedPages.add(id);
  schedule();
}

export function requestWiki(text: string) {
  if (text in useLinkStore.getState().wikis) return;
  wantedWikis.add(text);
  schedule();
}

/** Re-resolves every link shown so far (titles, trash and renames change them). */
export function refreshLinks() {
  const { pages, wikis } = useLinkStore.getState();
  for (const id of Object.keys(pages)) wantedPages.add(id);
  for (const text of Object.keys(wikis)) wantedWikis.add(text);
  if (wantedPages.size + wantedWikis.size > 0) schedule();
}

/** The title a page link's label is refreshed to on save, when known. */
export function currentTitle(id: string): string | undefined {
  const info = useLinkStore.getState().pages[id];
  return info?.state === "ok" ? (info.title ?? undefined) : undefined;
}

/** The wikilink text without brackets, alias or heading: `[[A/B#h|x]]` → `A/B#h`. */
export function wikiTarget(source: string): string {
  return source
    .replace(/^\[\[|\]\]$/g, "")
    .split("|")[0]
    ?.trim() as string;
}

export function resetLinkStoreForTests() {
  useLinkStore.setState({ pages: {}, wikis: {} });
  wantedPages.clear();
  wantedWikis.clear();
  scheduled = false;
}
