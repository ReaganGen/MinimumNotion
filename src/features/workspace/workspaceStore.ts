// The open workspace and the page tree the sidebar shows. Only expanded
// branches are loaded (ARCHITECTURE §7); a `workspace://changed` event
// reloads just those.
import { create } from "zustand";
import type { SaveStatus } from "@/features/editor/saveController";
import type { PageRef } from "@/lib/ipc/bindings/PageRef";
import type { PageSummary } from "@/lib/ipc/bindings/PageSummary";
import type { WorkspaceChanges } from "@/lib/ipc/bindings/WorkspaceChanges";
import type { WorkspaceInfo } from "@/lib/ipc/bindings/WorkspaceInfo";
import { ipc } from "@/lib/ipc/client";
import { GETTING_STARTED_BODY, GETTING_STARTED_TITLE } from "./gettingStarted";

export const ROOT = "root";
const RECENT_LIMIT = 12;
const key = (parentId: string | null) => parentId ?? ROOT;

export type WorkspaceStatus = "loading" | "none" | "open";

interface WorkspaceState {
  status: WorkspaceStatus;
  info: WorkspaceInfo | null;
  /** The last error, shown on the welcome screen or as a notice. */
  error: string | null;
  notices: string[];
  children: Record<string, PageSummary[]>;
  expanded: Record<string, boolean>;
  favorites: PageSummary[];
  activePageId: string | null;
  /** Bumped whenever the active page's file changes outside the editor. */
  externalChange: number;
  /** Bumped when the active page disappears; the page view decides what to keep. */
  activeRemoved: number;
  /** Bumped on every change notification (backlinks and collections refetch). */
  changeCount: number;
  /** A page just created by the user, whose title should take the focus. */
  focusTitleOf: string | null;
  /** Ancestors of the active page plus the page itself, for the top bar. */
  breadcrumbs: PageRef[];
  /** Pages opened this session, most recent first (⌘P with no query). */
  recent: PageRef[];
  saveStatus: SaveStatus | null;
  saveError: string | null;

  openLast: () => Promise<void>;
  choose: () => Promise<void>;
  createDefault: () => Promise<void>;
  /** Unzips a backup into a new folder and opens it. Null when cancelled; throws on failure. */
  restoreBackup: () => Promise<WorkspaceInfo | null>;
  loadChildren: (parentId: string | null) => Promise<void>;
  refresh: () => Promise<void>;
  applyChanges: (changes: WorkspaceChanges) => Promise<void>;
  toggleExpanded: (id: string) => void;
  expand: (id: string) => Promise<void>;
  openPage: (id: string | null) => void;
  createPage: (parentId: string | null) => Promise<string | null>;
  /** The optional first page, made only when the user asks for it. */
  addGettingStarted: () => Promise<void>;
  dismissNotice: (message: string) => void;
  reportError: (error: unknown) => void;
  setBreadcrumbs: (crumbs: PageRef[]) => void;
  setSaveStatus: (status: SaveStatus | null, error?: string) => void;
}

/** Edits must be saved before the editor goes away (page switch, quit). */
type Flusher = () => Promise<void>;
const flushers = new Set<Flusher>();
export function registerFlusher(flush: Flusher): () => void {
  flushers.add(flush);
  return () => flushers.delete(flush);
}
export async function flushAllEdits(): Promise<void> {
  await Promise.allSettled([...flushers].map((flush) => flush()));
}

function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export const useWorkspaceStore = create<WorkspaceState>()((set, get) => {
  async function adopt(info: WorkspaceInfo | null) {
    if (!info) {
      set({ status: "none", info: null });
      return;
    }
    set({
      status: "open",
      info,
      error: null,
      notices: [...new Set(info.notices)],
      children: {},
      expanded: {},
      favorites: [],
      activePageId: null,
      recent: [],
    });
    await get().refresh();
    const first = get().children[ROOT]?.[0];
    if (first) get().openPage(first.id);
  }

  async function attempt(action: () => Promise<WorkspaceInfo | null>, keepCurrent: boolean) {
    try {
      const info = await action();
      if (info || !keepCurrent) await adopt(info);
    } catch (error) {
      set({ error: message(error), status: keepCurrent && get().info ? "open" : "none" });
    }
  }

  return {
    status: "loading",
    info: null,
    error: null,
    notices: [],
    children: {},
    expanded: {},
    favorites: [],
    activePageId: null,
    externalChange: 0,
    activeRemoved: 0,
    changeCount: 0,
    focusTitleOf: null,
    breadcrumbs: [],
    recent: [],
    saveStatus: null,
    saveError: null,

    openLast: () => attempt(ipc.openLastWorkspace, false),
    choose: async () => {
      await flushAllEdits();
      await attempt(ipc.chooseWorkspace, true);
    },
    createDefault: async () => {
      await flushAllEdits();
      await attempt(ipc.createDefaultWorkspace, true);
    },
    restoreBackup: async () => {
      await flushAllEdits();
      // The caller reports the outcome, so a cancel or failure leaves the
      // open workspace and its error state alone.
      const info = await ipc.restoreBackup();
      if (info) await adopt(info);
      return info;
    },

    loadChildren: async (parentId) => {
      try {
        const pages = await ipc.listChildren(parentId);
        set({ children: { ...get().children, [key(parentId)]: pages } });
      } catch (error) {
        get().reportError(error);
      }
    },

    refresh: async () => {
      const loaded = Object.keys(get().children);
      const parents = new Set<string>([ROOT, ...loaded]);
      for (const [id, open] of Object.entries(get().expanded)) if (open) parents.add(id);
      const results = await Promise.allSettled(
        [...parents].map(async (parent) => {
          const pages = await ipc.listChildren(parent === ROOT ? null : parent);
          return [parent, pages] as const;
        }),
      );
      const children: Record<string, PageSummary[]> = {};
      for (const result of results) {
        // A parent that no longer exists just drops out of the cache.
        if (result.status === "fulfilled") children[result.value[0]] = result.value[1];
      }
      const favorites = await ipc.listFavorites().catch(() => get().favorites);
      set({ children, favorites });
    },

    applyChanges: async (changes) => {
      set({ changeCount: get().changeCount + 1 });
      const active = get().activePageId;
      if (active && changes.removed.includes(active)) {
        // Not closed here: unsaved edits must survive the file going away.
        set({ activeRemoved: get().activeRemoved + 1 });
      } else if (active && (changes.content_changed.includes(active) || changes.tree_changed)) {
        // A tree change can also mean Forma rewrote the open page (it became a
        // bundle, and relative links moved), so the page view re-checks then too.
        set({ externalChange: get().externalChange + 1 });
      }
      if (changes.tree_changed || changes.removed.length > 0) await get().refresh();
      // Rescans can find something worth telling (e.g. a copied page).
      const fresh = await ipc.takeNotices().catch((): string[] => []);
      const shown = get().notices;
      const added = fresh.filter((n, i) => !shown.includes(n) && fresh.indexOf(n) === i);
      if (added.length > 0) set({ notices: [...shown, ...added] });
    },

    toggleExpanded: (id) => {
      const open = !get().expanded[id];
      set({ expanded: { ...get().expanded, [id]: open } });
      if (open && !get().children[id]) void get().loadChildren(id);
    },

    expand: async (id) => {
      set({ expanded: { ...get().expanded, [id]: true } });
      await get().loadChildren(id);
    },

    openPage: (id) => {
      if (id === get().activePageId) return;
      // The editor flushes on unmount; flushing here too covers fast switches.
      void flushAllEdits();
      set({ activePageId: id, breadcrumbs: [], saveStatus: null, saveError: null });
    },

    createPage: async (parentId) => {
      try {
        const page = await ipc.createPage(parentId, "Untitled");
        if (parentId) await get().expand(parentId);
        await get().refresh();
        set({ activePageId: page.id, focusTitleOf: page.id });
        return page.id;
      } catch (error) {
        get().reportError(error);
        return null;
      }
    },

    addGettingStarted: async () => {
      try {
        const page = await ipc.createPage(null, GETTING_STARTED_TITLE);
        const { body_rev } = await ipc.getPage(page.id);
        await ipc.savePage(page.id, body_rev, GETTING_STARTED_BODY);
        await get().refresh();
        set({ activePageId: page.id });
      } catch (error) {
        get().reportError(error);
      }
    },

    dismissNotice: (notice) => set({ notices: get().notices.filter((n) => n !== notice) }),
    // The same message twice says nothing new, so notices are kept unique.
    reportError: (error) => {
      const notice = message(error);
      if (!get().notices.includes(notice)) set({ notices: [...get().notices, notice] });
    },
    setBreadcrumbs: (breadcrumbs) => {
      const page = breadcrumbs.at(-1);
      const recent = page
        ? [page, ...get().recent.filter((r) => r.id !== page.id)].slice(0, RECENT_LIMIT)
        : get().recent;
      set({ breadcrumbs, recent });
    },
    setSaveStatus: (saveStatus, error) => set({ saveStatus, saveError: error ?? null }),
  };
});

/** Test helper. */
export function resetWorkspaceStoreForTests() {
  flushers.clear();
  useWorkspaceStore.setState({
    status: "loading",
    info: null,
    error: null,
    notices: [],
    children: {},
    expanded: {},
    favorites: [],
    activePageId: null,
    externalChange: 0,
    activeRemoved: 0,
    changeCount: 0,
    focusTitleOf: null,
    breadcrumbs: [],
    recent: [],
    saveStatus: null,
    saveError: null,
  });
}
