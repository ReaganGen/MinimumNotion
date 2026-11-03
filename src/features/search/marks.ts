// Search snippets mark matches with U+0001 … U+0002 (src-tauri index). They're
// split into text runs here and rendered as elements, never as HTML.

export interface Run {
  text: string;
  match: boolean;
}

export function splitMarks(snippet: string): Run[] {
  const runs: Run[] = [];
  let match = false;
  let current = "";
  for (const char of snippet) {
    if (char === "\u0001" || char === "\u0002") {
      if (current) runs.push({ text: current, match });
      current = "";
      match = char === "\u0001";
    } else {
      current += char;
    }
  }
  if (current) runs.push({ text: current, match });
  return runs;
}

/** Runs of `title` with every occurrence of the query's words marked. */
export function markTitle(title: string, query: string): Run[] {
  const terms = query
    .toLowerCase()
    .split(/\s+/)
    .filter((t) => t.length > 0);
  if (terms.length === 0) return [{ text: title, match: false }];
  const lower = title.toLowerCase();
  // Lowercasing can change lengths for a few scripts; skip marking then.
  if (lower.length !== title.length) return [{ text: title, match: false }];
  const marked = new Array<boolean>(title.length).fill(false);
  for (const term of terms) {
    for (let at = lower.indexOf(term); at !== -1; at = lower.indexOf(term, at + 1)) {
      marked.fill(true, at, at + term.length);
    }
  }
  const runs: Run[] = [];
  for (let i = 0; i < title.length; i++) {
    const last = runs.at(-1);
    if (last && last.match === marked[i]) last.text += title[i];
    else runs.push({ text: title[i] as string, match: marked[i] as boolean });
  }
  return runs;
}
