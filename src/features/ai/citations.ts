// Citations in answers: `[S2]`, `[S1, S3]`, `【S4】`. A key counts only if it
// names a source Diaspar actually gave the model for that answer (ARCHITECTURE
// §11); any other key is shown as unverified and never becomes a link.
import type { Source } from "@/lib/ipc/bindings/Source";

export type CitationPart = string | { raw: string; keys: string[] };

const CITATION = /[[【]\s*(S\d+(?:\s*[,，、;；]\s*S\d+)*)\s*[\]】]/gi;

function normalize(key: string): string {
  return `S${Number.parseInt(key.slice(1), 10)}`;
}

/** Text with each citation marker split out. */
export function splitCitations(text: string): CitationPart[] {
  const parts: CitationPart[] = [];
  let last = 0;
  for (const match of text.matchAll(CITATION)) {
    const at = match.index ?? 0;
    if (at > last) parts.push(text.slice(last, at));
    const keys = (match[1] ?? "")
      .split(/[^Ss\d]+/)
      .filter(Boolean)
      .map(normalize);
    parts.push({ raw: match[0], keys });
    last = at + match[0].length;
  }
  if (last < text.length) parts.push(text.slice(last));
  return parts;
}

/** Every key the text cites, once each, in order. */
export function citedKeys(text: string): string[] {
  const keys: string[] = [];
  for (const part of splitCitations(text)) {
    if (typeof part === "string") continue;
    for (const key of part.keys) if (!keys.includes(key)) keys.push(key);
  }
  return keys;
}

/** The sources an answer cites, and the keys it cites that weren't provided. */
export function checkCitations(
  text: string,
  sources: Source[],
): { cited: Source[]; unverified: string[] } {
  const byKey = new Map(sources.map((s) => [s.key, s]));
  const cited: Source[] = [];
  const unverified: string[] = [];
  for (const key of citedKeys(text)) {
    const source = byKey.get(key);
    if (source) cited.push(source);
    else unverified.push(key);
  }
  return { cited, unverified };
}

/** The number after the highest key in `sources`, so new keys don't repeat old ones. */
export function nextSourceNumber(sources: Source[]): number {
  return sources.reduce((max, s) => Math.max(max, Number.parseInt(s.key.slice(1), 10) || 0), 0) + 1;
}
