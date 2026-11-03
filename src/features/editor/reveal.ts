// Showing a passage in its page: a citation in an AI answer opens the page,
// scrolls to the cited text and highlights it for a moment. The highlight is
// a decoration, so the document (and the file) never changes.
import { Extension } from "@tiptap/core";
import type { Node as ProseMirrorNode } from "@tiptap/pm/model";
import { Plugin, PluginKey } from "@tiptap/pm/state";
import { Decoration, DecorationSet } from "@tiptap/pm/view";
import { create } from "zustand";

interface RevealState {
  /** Text to show once its page is open. */
  target: { pageId: string; text: string } | null;
  reveal: (pageId: string, text: string) => void;
  done: () => void;
}

export const useRevealStore = create<RevealState>()((set) => ({
  target: null,
  reveal: (pageId, text) => set({ target: { pageId, text } }),
  done: () => set({ target: null }),
}));

export const flashKey = new PluginKey<DecorationSet>("forma-flash");

/** Highlights a range given as the `flashKey` meta; `null` clears it. */
export const Flash = Extension.create({
  name: "flash",
  addProseMirrorPlugins() {
    return [
      new Plugin<DecorationSet>({
        key: flashKey,
        state: {
          init: () => DecorationSet.empty,
          apply(tr, set) {
            const range = tr.getMeta(flashKey) as { from: number; to: number } | null | undefined;
            if (range === null) return DecorationSet.empty;
            if (range) {
              return DecorationSet.create(tr.doc, [
                Decoration.inline(range.from, range.to, { class: "forma-flash" }),
              ]);
            }
            return set.map(tr.mapping, tr.doc);
          },
        },
        props: { decorations: (state) => flashKey.getState(state) },
      }),
    ];
  },
});

/**
 * Where `needle` first occurs in the document's text, ignoring differences
 * in whitespace and case. Long needles are tried by their start: the text
 * Rust extracted can differ from the editor's later on (link titles, math).
 */
export function findTextRange(
  doc: ProseMirrorNode,
  needle: string,
): { from: number; to: number } | null {
  let flat = "";
  const positions: number[] = [];
  const push = (char: string, pos: number) => {
    const space = /\s/.test(char);
    if (space && (flat === "" || flat.endsWith(" "))) return;
    flat += space ? " " : char.toLowerCase();
    positions.push(pos);
  };
  doc.descendants((node, pos) => {
    if (node.isText && node.text) {
      for (let i = 0; i < node.text.length; i++) push(node.text[i] ?? "", pos + i);
    } else if (node.isBlock) {
      push(" ", pos);
    }
    return true;
  });
  const wanted = needle.replace(/\s+/g, " ").trim().toLowerCase().replace(/…$/, "");
  for (const length of [80, 40, 20]) {
    const piece = wanted.slice(0, length).trim();
    if (piece.length < 3) break;
    const at = flat.indexOf(piece);
    if (at >= 0) {
      const from = positions[at] ?? 0;
      const last = positions[at + piece.length - 1] ?? from;
      // Through the end of the block the match ends in, so a cut-short
      // excerpt still highlights its whole paragraph.
      return { from, to: Math.max(last + 1, doc.resolve(last + 1).end()) };
    }
    if (wanted.length <= length) break;
  }
  return null;
}
