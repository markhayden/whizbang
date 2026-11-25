// Each text and ground the components use, in both themes, held to 4.5 to 1 for text under
// 18 px (plan m5, task U1, item 2; WCAG 2.2, 1.4.3). Text of a disabled control is exempt,
// as WCAG exempts it, and so is `--color-faint`, which is used for nothing else.
import { describe, expect, it } from "vitest";
import { block, source, sources } from "./test/files";

const tokens = source("tokens.css");

type Rgba = [number, number, number, number];

export function parse(hex: string): Rgba {
  const match = /^#([0-9a-fA-F]{6})([0-9a-fA-F]{2})?$/.exec(hex.trim());
  if (!match) throw new Error(`not a colour: ${hex}`);
  const value = parseInt(match[1]!, 16);
  const alpha = match[2] === undefined ? 1 : parseInt(match[2], 16) / 255;
  return [(value >> 16) & 255, (value >> 8) & 255, value & 255, alpha];
}

/** A colour with alpha laid over an opaque one. */
export function over(top: Rgba, under: Rgba): Rgba {
  const a = top[3];
  return [top[0] * a + under[0] * (1 - a), top[1] * a + under[1] * (1 - a), top[2] * a + under[2] * (1 - a), 1];
}

function luminance([r, g, b]: Rgba): number {
  const channel = (c: number) => {
    const s = c / 255;
    return s <= 0.04045 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b);
}

export function ratio(a: Rgba, b: Rgba): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x) as [number, number];
  return (hi + 0.05) / (lo + 0.05);
}

/**
 * The pairs, as the components draw them: text, its ground, and the surface under the
 * ground when the ground is a tint. Each is named by where it is used.
 */
const PAIRS: Array<[string, string, string | null, string]> = [
  ["ink", "canvas", null, "body text on the page"],
  ["ink", "panel", null, "panels, rows, the top bar"],
  ["ink", "raised", null, "fields, menus, the plan block"],
  ["ink", "hover", null, "a hovered row, a selected tab"],
  ["ink", "brass-soft", null, "the selected choice, the current kit"],
  ["ink", "lacquer-soft", null, "the page's refusal"],
  ["ink", "lacquer-wash", "panel", "a failed step"],
  ["ink-2", "panel", null, "a field's label, the home line"],
  ["ink-2", "raised", null, "the plan block's figures"],
  ["ink-2", "hover", null, "the DRAFT tag"],
  ["ink-2", "lacquer-wash", "panel", "a failed step's note"],
  ["muted", "canvas", null, "an empty screen's words"],
  ["muted", "panel", null, "kit lines, times, the status bar"],
  ["muted", "raised", null, "help, menu lines, placeholders"],
  ["muted", "hover", null, "the allowance's total in the top bar"],
  ["ink-2", "brass-soft", null, "the current kit's line"],
  ["brass-ink", "brass", null, "Run, the RUNNING tag, a box's label"],
  ["on-lacquer", "lacquer", null, "Stop run, the FAIL tag, a flag's words"],
  ["lacquer", "lacquer-soft", null, "Stopping…"],
  ["lacquer", "raised", null, "NO KEY in a choice list, a field's error"],
  ["lacquer", "panel", null, "no key in the status bar"],
  ["on-pass", "pass", null, "the PASS tag"],
  ["on-unsure", "unsure", null, "the UNSURE tag"],
  ["pass", "raised", null, "READY in a choice list"],
  ["pass", "panel", null, "ready in the status bar"],
  ["volt", "raised", null, "GET in a choice list"],
  ["volt", "panel", null, "FROM RASCAL"],
  ["pass", "pass-tint", "panel", "PASS in a history row, READY in a model row"],
  ["unsure", "unsure-tint", "panel", "UNSURE in a history row"],
  ["lacquer", "lacquer-tint", "panel", "NO KEY in a model row"],
  ["panel", "ink", null, "the selected number of a segmented control"],
  ["on-copper", "copper", null, "the mark of a kit in copper"],
  ["on-copper", "volt", null, "the mark of a kit in volt"],
  ["mark-label-ink", "mark-label", "canvas", "a mask's label on the canvas"],
  ["on-scrim", "scrim", "mark", "BEFORE and AFTER over a light picture"],
  ["running", "hover", null, "a running step's mark"],
  ["muted", "lacquer-wash", "panel", "a failed step's figures"],
  ["ink", "brass-wash", "panel", "an attempt's value in a row of what differs"],
  ["ink-2", "brass-wash", "panel", "a baseline's value in a row of what differs"],
  ["muted", "brass-wash", "panel", "the name of a row of what differs"],
  ["ink-2", "lacquer-soft", null, "a refused bundle's fault, a blocked chain's reason"],
  ["ink-2", "brass-wash", "panel", "a picked chain's links, a report's differences"],
  ["muted", "raised", null, "an unanswerable replay's place"],
];

/**
 * Pairs of the light theme under 4.5 to 1, by the Paper file's light tokens as they are.
 * The page shows the dark theme only; the light set is kept so light can return, and these
 * are the design's to settle before it does. A pair that is fixed must leave this list,
 * and a new one fails.
 */
const LIGHT_SHORTFALLS: string[] = [
  "muted on hover (the allowance's total in the top bar): 4.37",
  "lacquer on lacquer-soft (Stopping…): 3.84",
  "lacquer on raised (NO KEY in a choice list, a field's error): 4.38",
  "on-pass on pass (the PASS tag): 3.37",
  "on-unsure on unsure (the UNSURE tag): 4.36",
  "pass on raised (READY in a choice list): 3.22",
  "pass on panel (ready in the status bar): 3.37",
  "volt on raised (GET in a choice list): 2.59",
  "volt on panel (FROM RASCAL): 2.71",
  "pass on pass-tint over panel (PASS in a history row, READY in a model row): 2.87",
  "unsure on unsure-tint over panel (UNSURE in a history row): 3.65",
  "lacquer on lacquer-tint over panel (NO KEY in a model row): 3.84",
  "running on hover (a running step's mark): 2.48",
  "muted on lacquer-wash over panel (a failed step's figures): 4.18",
];

function shortfalls(theme: Map<string, string>): string[] {
  const found: string[] = [];
  for (const [text, ground, surface, where] of PAIRS) {
    const value = (name: string) => {
      const hex = theme.get(`--color-${name}`);
      if (!hex) throw new Error(`no token --color-${name}`);
      return parse(hex);
    };
    let bg = value(ground);
    if (bg[3] < 1) {
      if (!surface) throw new Error(`${ground} has alpha and no surface under it`);
      bg = over(bg, value(surface));
    }
    const r = ratio(value(text), bg);
    if (r < 4.5) found.push(`${text} on ${ground}${surface ? ` over ${surface}` : ""} (${where}): ${r.toFixed(2)}`);
  }
  return found;
}

/** Each colour the styles give text (`color:`, not `border-color:`), by its token. */
export function textColours(css: string): Set<string> {
  return new Set([...css.matchAll(/(?<![-\w])color:\s*var\(--color-([a-z0-9-]+)\)/g)].map((m) => m[1]!));
}

/** Text colours exempt from the pairs: faint, kept for disabled and decorative text. */
const EXEMPT = new Set(["faint"]);

describe("contrast", () => {
  it("checks a pair for every colour the styles give text, but the exempt one", () => {
    const used = new Set<string>();
    for (const path of sources([".css"]).filter((p) => p !== "tokens.css")) for (const name of textColours(source(path))) used.add(name);
    const covered = new Set(PAIRS.map(([text]) => text));
    const missing = [...used].filter((name) => !covered.has(name) && !EXEMPT.has(name)).sort();
    expect(missing).toEqual([]);
    expect(used.size).toBeGreaterThan(10);
  });

  it("reads text colours and not border colours", () => {
    expect([...textColours(".a { color: var(--color-ink); border-color: var(--color-brass); }")]).toEqual(["ink"]);
  });

  it("computes the ratios WCAG gives", () => {
    expect(ratio(parse("#000000"), parse("#FFFFFF"))).toBeCloseTo(21, 5);
    expect(ratio(parse("#777777"), parse("#FFFFFF"))).toBeCloseTo(4.48, 2);
    expect(over(parse("#FFFFFF80"), parse("#000000"))[0]).toBeCloseTo(128, 0);
  });

  it("holds every pair of the dark theme, the one the page shows, to 4.5 to 1", () => {
    expect(shortfalls(block(tokens, ":root"))).toEqual([]);
  });

  it("finds in the light theme only the shortfalls it records", () => {
    expect(shortfalls(block(tokens, '[data-theme="light"]'))).toEqual(LIGHT_SHORTFALLS);
  });

  it("keeps the faint text colour for disabled and decorative text: under 4.5 to 1 on every surface", () => {
    const dark = block(tokens, ":root");
    for (const ground of ["panel", "raised", "hover"]) {
      expect(ratio(parse(dark.get("--color-faint")!), parse(dark.get(`--color-${ground}`)!))).toBeLessThan(4.5);
    }
  });
});
