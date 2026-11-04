// KaTeX math with Diaspar's Markdown rules (DATA_MODEL §5.2). The stock
// tokenizers would read currency such as "$5 and $10" as math, so these
// follow the pandoc rule instead.
import {
  BlockMath as BaseBlockMath,
  InlineMath as BaseInlineMath,
} from "@tiptap/extension-mathematics";

/**
 * Inline `$…$`: the opening `$` is followed by a non-space, the closing `$`
 * is preceded by a non-space and not followed by a digit, and `$$` never
 * starts inline math.
 */
export const INLINE_MATH = /^\$(?!\$)(?=\S)((?:\\.|[^$\\\n])*?\S)\$(?!\d)/;

/** Block math: `$$` alone on a line, the formula, then `$$` alone on a line. */
export const BLOCK_MATH = /^\$\$[ \t]*\n([\s\S]*?)\n[ \t]*\$\$[ \t]*(?:\n|$)/;

export const InlineMath = BaseInlineMath.extend({
  markdownTokenizer: {
    name: "inlineMath",
    level: "inline",
    start: (src: string) => src.indexOf("$"),
    tokenize: (src: string) => {
      const match = INLINE_MATH.exec(src);
      if (!match) return undefined;
      return { type: "inlineMath", raw: match[0], latex: match[1] };
    },
  },
});

export const BlockMath = BaseBlockMath.extend({
  markdownTokenizer: {
    name: "blockMath",
    level: "block",
    start: (src: string) => src.indexOf("$$"),
    tokenize: (src: string) => {
      const match = BLOCK_MATH.exec(src);
      if (!match) return undefined;
      return { type: "blockMath", raw: match[0], latex: match[1] };
    },
  },
  renderMarkdown: (node) => `$$\n${node.attrs?.latex ?? ""}\n$$`,
});
