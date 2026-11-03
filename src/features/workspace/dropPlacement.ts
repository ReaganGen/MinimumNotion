// Where a page dragged onto another page in the sidebar ends up.

export type DropZone = "before" | "inside" | "after";

/** Top and bottom quarters insert next to the row; the middle nests inside it. */
export function dropZone(offsetY: number, rowHeight: number): DropZone {
  if (offsetY < rowHeight * 0.25) return "before";
  if (offsetY > rowHeight * 0.75) return "after";
  return "inside";
}

export interface DropTarget {
  id: string;
  parentId: string | null;
}

export interface Placement {
  parentId: string | null;
  /** Position among the new siblings, or null to append. */
  position: number | null;
}

/**
 * Computes the move for dropping `draggedId` on `target`. `siblingIds` are the
 * target's siblings in display order. Returns null when the drop does nothing.
 */
export function placeDrop(
  draggedId: string,
  target: DropTarget,
  zone: DropZone,
  siblingIds: string[],
): Placement | null {
  if (draggedId === target.id) return null;
  if (zone === "inside") return { parentId: target.id, position: null };

  const targetIndex = siblingIds.indexOf(target.id);
  let position = zone === "before" ? targetIndex : targetIndex + 1;
  const draggedIndex = siblingIds.indexOf(draggedId);
  if (draggedIndex !== -1) {
    // Moving within the same parent: removing the page shifts later positions.
    if (draggedIndex < position) position -= 1;
    if (draggedIndex === position) return null;
  }
  return { parentId: target.parentId, position };
}
