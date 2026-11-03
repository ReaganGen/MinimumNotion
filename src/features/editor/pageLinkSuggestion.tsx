// "[[" opens title search and inserts a page link (ARCHITECTURE §6.5). The
// target's ID is written into its file first, so the link never points at
// an ID that exists only in memory (ensure_persisted_id, CLAUDE.md rule 6).
import { type Editor, Extension, type Range } from "@tiptap/core";
import { PluginKey } from "@tiptap/pm/state";
import Suggestion from "@tiptap/suggestion";
import { FileText } from "lucide-react";
import { useLinkStore } from "@/features/links/linkStore";
import { useWorkspaceStore } from "@/features/workspace/workspaceStore";
import type { PageHit } from "@/lib/ipc/bindings/PageHit";
import { ipc } from "@/lib/ipc/client";
import { suggestionPopup } from "./suggestionMenu";

/** Pages matching what's typed after "[[", or recent pages before anything is. */
export async function findLinkTargets(query: string): Promise<PageHit[]> {
  if (!query.trim()) {
    return useWorkspaceStore
      .getState()
      .recent.map((r) => ({ id: r.id, title: r.title, icon: r.icon, path: [], snippet: null }));
  }
  try {
    return await ipc.quickOpen(query);
  } catch {
    return [];
  }
}

export async function insertPageLink(editor: Editor, range: Range, page: PageHit) {
  try {
    await ipc.ensurePersistedId(page.id);
  } catch (error) {
    useWorkspaceStore.getState().reportError(error);
    return;
  }
  useLinkStore.setState({
    pages: {
      ...useLinkStore.getState().pages,
      [page.id]: { id: page.id, title: page.title, icon: page.icon, state: "ok" },
    },
  });
  editor
    .chain()
    .focus()
    .insertContentAt(range, [
      { type: "pageLink", attrs: { id: page.id, label: page.title, anchor: "" } },
      { type: "text", text: " " },
    ])
    .run();
}

export const PageLinkSuggestion = Extension.create({
  name: "pageLinkSuggestion",
  addProseMirrorPlugins() {
    return [
      Suggestion<PageHit>({
        editor: this.editor,
        // Distinct from the "/" menu's key; two suggestions can't share one.
        pluginKey: new PluginKey("pageLinkSuggestion"),
        char: "[[",
        allowSpaces: true,
        allowedPrefixes: null,
        allow: ({ state, range }) => !state.doc.resolve(range.from).parent.type.spec.code,
        items: ({ query }) => findLinkTargets(query),
        command: ({ editor, range, props }) => {
          void insertPageLink(editor, range, props);
        },
        render: suggestionPopup<PageHit>({
          label: "Link to a page",
          empty: "No pages with that title",
          itemKey: (page) => page.id,
          renderItem: (page) => (
            <>
              <span className="inline-flex size-5 shrink-0 items-center justify-center text-muted-foreground">
                {page.icon ?? <FileText className="size-4" aria-hidden />}
              </span>
              <span className="min-w-0 flex-1">
                <span className="block truncate">{page.title}</span>
                {page.path.length > 0 && (
                  <span className="block truncate text-[11.5px] text-faint-foreground">
                    {page.path.join(" / ")}
                  </span>
                )}
              </span>
            </>
          ),
        }),
      }),
    ];
  },
});
