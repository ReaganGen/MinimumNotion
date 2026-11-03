// Text and focus-ring contrast in both themes, computed from tokens.css
// (WCAG 2.2: 4.5:1 for text, 3:1 for the focus ring). Translucent colors are
// composited over the surface they're used on.
import { describe, expect, it } from "vitest";
import css from "./tokens.css?raw";

type Rgba = [number, number, number, number];

function block(selector: string): Record<string, string> {
  const start = css.indexOf(`${selector} {`);
  const body = css.slice(start, css.indexOf("\n}", start));
  const out: Record<string, string> = {};
  for (const m of body.matchAll(/--([\w-]+):\s*([^;]+);/g)) out[m[1] ?? ""] = (m[2] ?? "").trim();
  return out;
}

function parse(value: string): Rgba {
  const hex = /^#([0-9a-f]{6})$/i.exec(value);
  if (hex?.[1]) {
    const n = Number.parseInt(hex[1], 16);
    return [(n >> 16) & 255, (n >> 8) & 255, n & 255, 1];
  }
  const rgb = /^rgb\((\d+) (\d+) (\d+)(?: \/ ([\d.]+))?\)$/.exec(value);
  if (rgb) return [Number(rgb[1]), Number(rgb[2]), Number(rgb[3]), rgb[4] ? Number(rgb[4]) : 1];
  throw new Error(`unparsed color ${value}`);
}

function over([r, g, b, a]: Rgba, [br, bg, bb]: Rgba): Rgba {
  return [r * a + br * (1 - a), g * a + bg * (1 - a), b * a + bb * (1 - a), 1];
}

function luminance([r, g, b]: Rgba): number {
  const channel = (c: number) => {
    const s = c / 255;
    return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b);
}

function ratio(fg: Rgba, bg: Rgba): number {
  const [a, b] = [luminance(over(fg, bg)), luminance(bg)].sort((x, y) => y - x);
  return ((a ?? 0) + 0.05) / ((b ?? 0) + 0.05);
}

const light = block(":root");
const dark = { ...light, ...block('[data-theme="dark"]') };

describe.each([
  ["light", light],
  ["dark", dark],
])("%s theme", (_, tokens) => {
  const color = (name: string) => parse(tokens[name] ?? "");
  const surface = (name: string) => over(color(name), color("background"));
  const surfaces = ["background", "sidebar", "popover"] as const;

  it.each(["foreground", "muted-foreground", "faint-foreground", "link", "destructive"])(
    "%s text is readable on every surface",
    (text) => {
      for (const name of surfaces) {
        expect(ratio(color(text), surface(name)), `${text} on ${name}`).toBeGreaterThanOrEqual(4.5);
      }
    },
  );

  it("sidebar and button text are readable", () => {
    expect(ratio(color("sidebar-foreground"), surface("sidebar"))).toBeGreaterThanOrEqual(4.5);
    expect(ratio(color("primary-foreground"), color("primary"))).toBeGreaterThanOrEqual(4.5);
    expect(ratio(color("primary-foreground"), color("destructive-fill"))).toBeGreaterThanOrEqual(
      4.5,
    );
    // Muted text also sits on hover and chip fills.
    const chip = over(color("accent"), surface("background"));
    expect(ratio(color("muted-foreground"), chip)).toBeGreaterThanOrEqual(4.5);
  });

  it("the focus ring stands out", () => {
    for (const name of surfaces) {
      expect(ratio(color("ring"), surface(name)), `ring on ${name}`).toBeGreaterThanOrEqual(3);
    }
  });

  it("collection tags are readable", () => {
    for (const tag of [
      "gray",
      "brown",
      "orange",
      "yellow",
      "green",
      "blue",
      "purple",
      "pink",
      "red",
    ]) {
      expect(ratio(color(`tag-${tag}-fg`), color(`tag-${tag}-bg`)), tag).toBeGreaterThanOrEqual(
        4.5,
      );
    }
  });
});
