import { describe, expect, it } from "vitest";
import {
  AUDIO_EXTENSIONS,
  extensionOf,
  fileKind,
  formatSize,
  IMAGE_EXTENSIONS,
  mediaKind,
  referenceName,
  stemOf,
  VIDEO_EXTENSIONS,
} from "./media";

const assetsRs = Object.values(
  import.meta.glob("/src-tauri/src/assets.rs", { query: "?raw", import: "default", eager: true }),
)[0] as string;

function rustList(name: string): string[] {
  const match = new RegExp(`pub const ${name}: &\\[&str\\] =\\s*&\\[([^\\]]*)\\]`).exec(assetsRs);
  return [...(match?.[1] ?? "").matchAll(/"([^"]+)"/g)].map((m) => m[1] ?? "");
}

describe("media kinds", () => {
  it("match the extensions the file dialog offers", () => {
    expect(rustList("IMAGE_EXTENSIONS")).toEqual(IMAGE_EXTENSIONS);
    expect(rustList("AUDIO_EXTENSIONS")).toEqual(AUDIO_EXTENSIONS);
    expect(rustList("VIDEO_EXTENSIONS")).toEqual(VIDEO_EXTENSIONS);
  });

  it("follow the extension, ignoring case, query and fragment", () => {
    expect(mediaKind("_assets/Talk.MOV")).toBe("video");
    expect(mediaKind("_assets/a%20b.m4a")).toBe("audio");
    expect(mediaKind("https://example.com/v.mp4?x=1#t=3")).toBe("video");
    expect(mediaKind("_assets/photo.png")).toBe("image");
    expect(mediaKind("_assets/unknown.xyz")).toBe("image");
    expect(extensionOf("_assets/.hidden")).toBe("");
  });

  it("sort dropped files by type, then by extension", () => {
    expect(fileKind({ name: "a.png", type: "image/png" })).toBe("image");
    expect(fileKind({ name: "a", type: "audio/mpeg" })).toBe("audio");
    expect(fileKind({ name: "clip.mov", type: "" })).toBe("video");
    expect(fileKind({ name: "Report.pdf", type: "application/pdf" })).toBe("file");
    expect(fileKind({ name: "notes", type: "" })).toBe("file");
  });

  it("name and size files for the card", () => {
    expect(referenceName("_assets/My%20Talk%202.mov")).toBe("My Talk 2.mov");
    expect(referenceName("_assets/bad%zz.pdf")).toBe("bad%zz.pdf");
    expect(stemOf("My Talk.mov")).toBe("My Talk");
    expect(stemOf(".env")).toBe(".env");
    expect(formatSize(1)).toBe("1 byte");
    expect(formatSize(999)).toBe("999 bytes");
    expect(formatSize(2_400_000)).toBe("2.4 MB");
    expect(formatSize(830_000)).toBe("830 KB");
  });
});
