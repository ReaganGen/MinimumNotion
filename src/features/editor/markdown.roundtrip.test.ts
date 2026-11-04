// The executable definition of the Markdown dialect (DATA_MODEL §5).
// Each fixture in tests/fixtures/markdown/ must survive parse → serialize.
// A `name.expected.md` file gives the canonical output when it differs from
// the input; otherwise the output must equal the input byte for byte.
import { describe, expect, it, vi } from "vitest";
import { normalizeMarkdown, roundTripMarkdown } from "./markdown";
import {
  escapeInlineText,
  escapeLineStart,
  sanitizeAutolinks,
  textEncoderIsPatched,
} from "./markdownRules";

const files = import.meta.glob("/tests/fixtures/markdown/*.md", {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

const inputs = Object.entries(files).filter(([path]) => !path.endsWith(".expected.md"));

describe("Markdown round trip", () => {
  it("has fixtures", () => {
    expect(inputs.length).toBeGreaterThan(15);
  });

  it.each(inputs)("%s", (path, input) => {
    const expected = files[path.replace(/\.md$/, ".expected.md")] ?? normalizeMarkdown(input);
    const output = roundTripMarkdown(input);
    expect(output).toBe(expected);
    // Saving again must not change anything (the canonical form is a fixed point).
    expect(roundTripMarkdown(output)).toBe(output);
  });
});

describe("the round trip", () => {
  it("builds no DOM, so nothing in the page is requested", () => {
    const create = vi.spyOn(document, "createElement");
    try {
      roundTripMarkdown("![a](https://example.com/a.png)\n\n![v](https://example.com/v.mp4)\n");
      const tags = create.mock.calls.map(([tag]) => String(tag).toLowerCase());
      expect(tags).not.toContain("img");
      expect(tags).not.toContain("video");
    } finally {
      create.mockRestore();
    }
  });
});

describe("Markdown escaping", () => {
  it("replaces the library's text encoder", () => {
    // Fails if an @tiptap/markdown update renames the private method we replace.
    expect(textEncoderIsPatched).toBe(true);
  });

  it("escapes only where the text would otherwise be reparsed", () => {
    expect(escapeInlineText("Tom & Jerry, a < b")).toBe("Tom & Jerry, a < b");
    expect(escapeInlineText("snake_case")).toBe("snake_case");
    expect(escapeInlineText("*emph*")).toBe("\\*emph\\*");
    expect(escapeInlineText("&amp;")).toBe("\\&amp;");
    expect(escapeInlineText("<div>")).toBe("\\<div>");
    expect(escapeInlineText("$x$ and $5")).toBe("\\$x$ and $5");
  });

  it("escapes block markers at the start of a line only", () => {
    expect(escapeLineStart("# tag")).toBe("\\# tag");
    expect(escapeLineStart("12. item")).toBe("12\\. item");
    expect(escapeLineStart("- item")).toBe("\\- item");
    expect(escapeLineStart("> quote")).toBe("\\> quote");
    expect(escapeLineStart("a # b")).toBe("a # b");
  });
});

describe("autolinks", () => {
  const link = (href: string, autolink: string | null) => ({
    type: "link",
    attrs: { href, title: null, autolink },
  });

  it("keeps the autolink form while the text equals the URL", () => {
    const doc = {
      type: "doc",
      content: [
        {
          type: "paragraph",
          content: [
            { type: "text", text: "https://a.com", marks: [link("https://a.com", "angle")] },
          ],
        },
      ],
    };
    expect(sanitizeAutolinks(doc)).toEqual(doc);
  });

  it("drops the autolink form when the text was edited or partly formatted", () => {
    const edited = {
      type: "paragraph",
      content: [{ type: "text", text: "my site", marks: [link("https://a.com", "bare")] }],
    };
    const partlyBold = {
      type: "paragraph",
      content: [
        { type: "text", text: "https://", marks: [link("https://a.com", "angle")] },
        { type: "text", text: "a.com", marks: [link("https://a.com", "angle"), { type: "bold" }] },
      ],
    };
    const result = sanitizeAutolinks({ type: "doc", content: [edited, partlyBold] });
    const autolinkValues = JSON.stringify(result).match(/"autolink":("[a-z]+"|null)/g);
    expect(autolinkValues).toEqual(['"autolink":null', '"autolink":null', '"autolink":null']);
  });
});
