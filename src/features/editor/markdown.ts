// Body Markdown ⇄ editor document conversion (ARCHITECTURE §6.1). Rust owns
// frontmatter; this module only ever sees the body.
import { Editor } from "@tiptap/core";
import { createMarkdownExtensions } from "./extensions";
import { sanitizeAutolinks } from "./markdownRules";

/** Canonical file form: LF line endings and exactly one trailing newline (DATA_MODEL §5.3). */
export function normalizeMarkdown(markdown: string): string {
  const lf = markdown.replace(/\r\n?/g, "\n").replace(/\s+$/, "");
  return lf === "" ? "" : `${lf}\n`;
}

/**
 * Parses `markdown` into a schema-validated document and serializes it again,
 * exactly as saving an unchanged page would. Used by the round-trip tests and
 * by the open-time fidelity check.
 */
export function roundTripMarkdown(markdown: string): string {
  const editor = new Editor({
    extensions: createMarkdownExtensions(),
    content: markdown.replace(/\r\n?/g, "\n"),
    contentType: "markdown",
  });
  try {
    return docToMarkdown(editor);
  } finally {
    editor.destroy();
  }
}

/** Serializes the editor's document the way a save does. */
export function docToMarkdown(editor: Editor): string {
  const manager = editor.markdown;
  if (!manager) throw new Error("the Markdown extension is not installed");
  return normalizeMarkdown(manager.serialize(sanitizeAutolinks(editor.getJSON())));
}
