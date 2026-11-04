// The live views of media, file cards, bookmarks and HTML embeds (ADR-049),
// rendered in a real editor.

import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { Editor } from "@tiptap/core";
import { EditorContent, useEditor } from "@tiptap/react";
import { useEffect } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ipc } from "@/lib/ipc/client";
import { answerBookmarkDialog, useBookmarkDialog } from "./bookmarkDialog";
import { EMBED_CSP, embedDocument } from "./htmlEmbed";
import { createEditorExtensions } from "./liveExtensions";
import { docToMarkdown } from "./markdown";

vi.mock("@/lib/ipc/client", () => ({
  ipc: {
    attachmentInfo: vi.fn(),
    openAttachment: vi.fn(async () => undefined),
    revealAttachment: vi.fn(async () => undefined),
    openExternal: vi.fn(async () => undefined),
    resolvePages: vi.fn(async () => []),
    resolveWikilinks: vi.fn(async () => []),
  },
  assetUrl: (page: string, reference: string) => `diaspar-asset://localhost/${page}/${reference}`,
}));

const PAGE = "11111111-1111-4111-8111-111111111111";

let current: Editor | null = null;

function Harness({ markdown, editable }: { markdown: string; editable: boolean }) {
  const editor = useEditor({
    extensions: createEditorExtensions(PAGE),
    content: markdown,
    contentType: "markdown",
    editable,
  });
  useEffect(() => {
    current = editor;
  }, [editor]);
  return <EditorContent editor={editor} />;
}

async function show(markdown: string, editable = true): Promise<Editor> {
  render(<Harness markdown={markdown} editable={editable} />);
  await waitFor(() => expect(current).not.toBeNull());
  return current as unknown as Editor;
}

beforeEach(() => {
  vi.mocked(ipc.attachmentInfo).mockResolvedValue({ name: "Report.pdf", size: 2_400_000 });
});
afterEach(() => {
  current = null;
  vi.clearAllMocks();
});

describe("audio and video", () => {
  it("play files from _assets/ through the asset protocol", async () => {
    const { container } = render(
      <Harness markdown={"![Demo](_assets/Demo.mov)\n\n![Talk](_assets/Talk.m4a)"} editable />,
    );
    const video = await waitFor(() => {
      const found = container.querySelector("video");
      expect(found).not.toBeNull();
      return found as HTMLVideoElement;
    });
    expect(video.getAttribute("src")).toBe(`diaspar-asset://localhost/${PAGE}/_assets/Demo.mov`);
    expect(video.controls).toBe(true);
    expect(video.preload).toBe("metadata");
    expect(container.querySelector("audio")?.getAttribute("src")).toContain("_assets/Talk.m4a");
    expect(screen.getByText("Demo")).toBeInTheDocument();
  });

  it("never load remote audio or video, and offer the browser instead", async () => {
    const { container } = render(
      <Harness markdown={"![Talk](https://example.com/talk.mp4)"} editable />,
    );
    const open = await screen.findByRole("button", { name: "Open in browser" });
    expect(container.querySelector("video, audio, source")).toBeNull();
    expect(screen.getByText("Video not loaded: https://example.com/talk.mp4")).toBeInTheDocument();
    fireEvent.click(open);
    expect(ipc.openExternal).toHaveBeenCalledWith("https://example.com/talk.mp4");
  });
});

describe("file cards", () => {
  it("show the name, type and size, and open through Rust", async () => {
    await show("[Report.pdf](_assets/Report.pdf)");
    expect(await screen.findByText("PDF · 2.4 MB")).toBeInTheDocument();
    expect(screen.getByText("Report.pdf")).toBeInTheDocument();
    expect(ipc.attachmentInfo).toHaveBeenCalledWith(PAGE, "_assets/Report.pdf");
    fireEvent.click(screen.getByRole("button", { name: "Open" }));
    fireEvent.click(screen.getByRole("button", { name: "Show in Finder" }));
    expect(ipc.openAttachment).toHaveBeenCalledWith(PAGE, "_assets/Report.pdf");
    expect(ipc.revealAttachment).toHaveBeenCalledWith(PAGE, "_assets/Report.pdf");
  });

  it("say when the file is missing", async () => {
    vi.mocked(ipc.attachmentInfo).mockRejectedValue(new Error("not found"));
    await show("[Gone.zip](_assets/Gone.zip)");
    expect(await screen.findByText("File not found")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Open" })).toBeDisabled();
  });
});

describe("bookmarks", () => {
  it("show what the user typed and open the browser on click", async () => {
    await show('[Diaspar](https://example.com/diaspar "Notes on your Mac")');
    fireEvent.click(await screen.findByRole("button", { name: /Diaspar/ }));
    expect(ipc.openExternal).toHaveBeenCalledWith("https://example.com/diaspar");
    expect(screen.getByText("Notes on your Mac")).toBeInTheDocument();
    expect(screen.getByText("example.com/diaspar")).toBeInTheDocument();
  });

  it("are edited through the dialog", async () => {
    const editor = await show('[Old](https://example.com/old "")');
    fireEvent.click(await screen.findByRole("button", { name: "Edit bookmark" }));
    await waitFor(() => expect(useBookmarkDialog.getState().request).not.toBeNull());
    expect(useBookmarkDialog.getState().request?.initial).toEqual({
      url: "https://example.com/old",
      title: "Old",
      description: "",
    });
    act(() =>
      answerBookmarkDialog({ url: "https://example.com/new", title: "New [1]", description: "" }),
    );
    await waitFor(() =>
      expect(docToMarkdown(editor)).toBe('[New \\[1\\]](https://example.com/new "")\n'),
    );
  });

  it("can't be edited in a read-only page", async () => {
    await show('[Diaspar](https://example.com "")', false);
    await screen.findByRole("button", { name: /Diaspar/ });
    expect(screen.queryByRole("button", { name: "Edit bookmark" })).toBeNull();
  });
});

describe("HTML embeds", () => {
  const markdown =
    '```html embed height=300\n<p onclick="alert(1)">Hi</p><script>parent.x=1</script>\n```';

  it("render in a frame with an empty sandbox and a policy that allows nothing", async () => {
    const { container } = render(<Harness markdown={markdown} editable />);
    const frame = await waitFor(() => {
      const found = container.querySelector("iframe");
      expect(found).not.toBeNull();
      return found as HTMLIFrameElement;
    });
    // An empty sandbox: no scripts, forms, popups, top navigation or same origin.
    expect(frame.getAttribute("sandbox")).toBe("");
    expect(frame.getAttribute("src")).toBeNull();
    expect(frame.style.height).toBe("300px");
    // Static: no link or form in it can be clicked or reached with Tab.
    expect(frame.style.pointerEvents).toBe("none");
    expect(frame.tabIndex).toBe(-1);
    const doc = frame.getAttribute("srcdoc") ?? "";
    expect(doc.indexOf("Content-Security-Policy")).toBeLessThan(doc.indexOf("<body>"));
    expect(doc).toContain('<p onclick="alert(1)">Hi</p><script>parent.x=1</script>');
  });

  it("switch to the code, where edits save as Markdown", async () => {
    const editor = await show(markdown);
    fireEvent.click(screen.getByRole("radio", { name: "Code" }));
    const code = screen.getByRole("textbox", { name: "HTML code" });
    fireEvent.change(code, { target: { value: "<b>Bold</b>" } });
    expect(docToMarkdown(editor)).toBe("```html embed height=300\n<b>Bold</b>\n```\n");
    const height = screen.getByLabelText("Height");
    fireEvent.change(height, { target: { value: "5000" } });
    fireEvent.blur(height);
    expect(docToMarkdown(editor)).toBe("```html embed height=2000\n<b>Bold</b>\n```\n");
  });

  it("start in the code view when empty", async () => {
    await show("```html embed\n```");
    expect(screen.getByRole("textbox", { name: "HTML code" })).toBeInTheDocument();
    expect(document.querySelector("iframe")).toBeNull();
  });
});

describe("the embed document", () => {
  it("allows no scripts, connections, frames or forms", () => {
    const directives = Object.fromEntries(
      EMBED_CSP.split(";").map((d) => {
        const [name = "", ...values] = d.trim().split(/\s+/);
        return [name, values.join(" ")];
      }),
    );
    expect(directives["default-src"]).toBe("'none'");
    for (const name of ["script-src", "connect-src", "frame-src", "media-src", "object-src"]) {
      expect(directives[name], name).toBeUndefined();
    }
    expect(directives["img-src"]).toBe("data:");
    expect(directives["form-action"]).toBe("'none'");
    expect(EMBED_CSP).not.toMatch(/unsafe-eval|https?:|\*/);
  });

  it("puts the policy before anything the embed contains", () => {
    const doc = embedDocument('<meta http-equiv="refresh" content="0;url=https://x">');
    expect(doc.startsWith("<!doctype html><html><head><meta charset")).toBe(true);
    expect(doc.indexOf(EMBED_CSP)).toBeLessThan(doc.indexOf("refresh"));
    // Links try to open a new window, which the empty sandbox forbids.
    expect(doc.indexOf('<base target="_blank">')).toBeLessThan(doc.indexOf("<body>"));
  });
});
