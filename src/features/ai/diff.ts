// Showing a word-level diff (from Rust) without walls of unchanged text:
// long unchanged runs keep a little context on each side.
import type { DiffSpan } from "@/lib/ipc/bindings/DiffSpan";

/** Characters of unchanged text kept next to a change. */
export const CONTEXT_CHARS = 80;

export type DiffPart = DiffSpan | { kind: "gap"; hidden: number };

/** The diff with long unchanged stretches cut down to their ends. */
export function collapseDiff(spans: DiffSpan[], context = CONTEXT_CHARS): DiffPart[] {
  const out: DiffPart[] = [];
  spans.forEach((span, i) => {
    if (span.kind !== "equal") {
      out.push(span);
      return;
    }
    const chars = Array.from(span.text);
    const first = i === 0;
    const last = i === spans.length - 1;
    const keepStart = first ? 0 : context;
    const keepEnd = last ? 0 : context;
    if (chars.length <= keepStart + keepEnd + 20) {
      out.push(span);
      return;
    }
    if (keepStart > 0) out.push({ kind: "equal", text: chars.slice(0, keepStart).join("") });
    out.push({ kind: "gap", hidden: chars.length - keepStart - keepEnd });
    if (keepEnd > 0)
      out.push({ kind: "equal", text: chars.slice(chars.length - keepEnd).join("") });
  });
  return out;
}

/** The changes that must be chosen along with `id`, and those that need it. */
export function dependents(
  changes: { change_id: string; depends_on: string[] }[],
  id: string,
): { needs: string[]; neededBy: string[] } {
  const needs = new Set<string>();
  const neededBy = new Set<string>();
  const walk = (current: string, up: boolean) => {
    for (const change of changes) {
      const linked = up
        ? change.change_id === current && change.depends_on
        : change.depends_on.includes(current) && [change.change_id];
      for (const next of linked || []) {
        const into = up ? needs : neededBy;
        if (!into.has(next)) {
          into.add(next);
          walk(next, up);
        }
      }
    }
  };
  walk(id, true);
  walk(id, false);
  return { needs: [...needs], neededBy: [...neededBy] };
}
