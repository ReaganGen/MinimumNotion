// Media and attachments (ADR-049, DATA_MODEL §5.2). Images, audio and video
// are all `![caption](src)`; the extension picks the player. These lists match
// the ones the file dialog uses (src-tauri/src/assets.rs, tested).

export const IMAGE_EXTENSIONS = ["png", "jpg", "jpeg", "gif", "webp", "avif", "heic", "svg"];
export const AUDIO_EXTENSIONS = ["mp3", "m4a", "aac", "wav", "flac", "ogg", "oga", "opus"];
export const VIDEO_EXTENSIONS = ["mp4", "m4v", "mov", "webm", "ogv"];

export type MediaKind = "image" | "audio" | "video";

/** The lowercased extension of a reference or URL, without query or fragment. */
export function extensionOf(src: string): string {
  const path = src.split(/[?#]/, 1)[0] ?? "";
  const name = path.slice(path.lastIndexOf("/") + 1);
  const dot = name.lastIndexOf(".");
  return dot > 0 ? name.slice(dot + 1).toLowerCase() : "";
}

/** How an `![…](src)` is shown. Unknown extensions stay images, as before. */
export function mediaKind(src: string): MediaKind {
  const ext = extensionOf(src);
  if (AUDIO_EXTENSIONS.includes(ext)) return "audio";
  if (VIDEO_EXTENSIONS.includes(ext)) return "video";
  return "image";
}

/** The kind a dropped or pasted file is inserted as; anything else is a file card. */
export function fileKind(file: { name: string; type: string }): MediaKind | "file" {
  if (file.type.startsWith("image/")) return "image";
  if (file.type.startsWith("audio/")) return "audio";
  if (file.type.startsWith("video/")) return "video";
  const ext = extensionOf(file.name);
  if (IMAGE_EXTENSIONS.includes(ext)) return "image";
  if (AUDIO_EXTENSIONS.includes(ext)) return "audio";
  if (VIDEO_EXTENSIONS.includes(ext)) return "video";
  return "file";
}

/** The file name a reference points at, decoded (`_assets/My%20Talk.mov` → `My Talk.mov`). */
export function referenceName(reference: string): string {
  const path = reference.split(/[?#]/, 1)[0] ?? "";
  const last = path.slice(path.lastIndexOf("/") + 1);
  try {
    return decodeURIComponent(last);
  } catch {
    return last;
  }
}

/** A file name without its extension, for a caption. */
export function stemOf(name: string): string {
  const dot = name.lastIndexOf(".");
  return dot > 0 ? name.slice(0, dot) : name;
}

/** "2.4 MB", "830 KB", "12 bytes". */
export function formatSize(bytes: number): string {
  if (bytes < 1000) return `${bytes} ${bytes === 1 ? "byte" : "bytes"}`;
  const units = ["KB", "MB", "GB", "TB"];
  let value = bytes / 1000;
  let unit = 0;
  while (value >= 1000 && unit < units.length - 1) {
    value /= 1000;
    unit += 1;
  }
  return `${value < 10 ? value.toFixed(1) : Math.round(value)} ${units[unit]}`;
}

export type AttachmentKind = MediaKind | "file";

// The "/" menu's Media items ask the page editor that owns `editor` to pick
// and insert a file (PageEditor.tsx), because it knows how to save first.
type AttachmentHost = (kind: AttachmentKind) => Promise<void>;
const hosts = new WeakMap<object, AttachmentHost>();

export function setAttachmentHost(editor: object, host: AttachmentHost): () => void {
  hosts.set(editor, host);
  return () => {
    if (hosts.get(editor) === host) hosts.delete(editor);
  };
}

/** Lets the user pick a file of `kind` and inserts it at the cursor. */
export async function pickAttachment(editor: object, kind: AttachmentKind): Promise<void> {
  await hosts.get(editor)?.(kind);
}
