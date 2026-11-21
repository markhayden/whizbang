// The tokens are the Paper file's, by their exact values, and no component holds a colour
// of its own (plan m5, task U1, item 2).
import { describe, expect, it } from "vitest";
import { block, source, sources } from "./test/files";

const tokens = source("tokens.css");

/** The Paper file's colour tokens, read from it with its tokens tool (light, then dark). */
const PAPER: Array<[string, string, string]> = [
  ["--color-panel", "#FFFFFF", "#131518"],
  ["--color-canvas", "#F3F4F6", "#0A0B0D"],
  ["--color-ink", "#0E1014", "#F5F7F9"],
  ["--color-muted", "#6A707A", "#8B929C"],
  ["--color-line", "#E2E5E9", "#2A2E34"],
  ["--color-lacquer", "#E5212F", "#FF4552"],
  ["--color-pass", "#12A150", "#34D77E"],
  ["--color-raised", "#F9FAFB", "#1A1D21"],
  ["--color-hover", "#EEF0F3", "#22252A"],
  ["--color-line-strong", "#CBD0D6", "#3B4048"],
  ["--color-ink-2", "#363B44", "#C8CDD5"],
  ["--color-faint", "#A2A8B1", "#5B616A"],
  ["--color-brass", "#FFB800", "#FFC51F"],
  ["--color-brass-soft", "#FFF3CC", "#382B00"],
  ["--color-brass-ink", "#1F1600", "#1A1200"],
  ["--color-lacquer-soft", "#FFE5E7", "#3A1216"],
  ["--color-copper", "#FF6A13", "#FF8636"],
  ["--color-volt", "#00A9E0", "#33D1FF"],
  ["--color-unsure", "#7B5CFF", "#9F8CFF"],
];

/** The Paper file's other tokens, which have one value. */
const PAPER_ONE: Array<[string, string]> = [
  ["--text-xs", "11px"],
  ["--text-sm", "12px"],
  ["--text-md", "13px"],
  ["--text-lg", "15px"],
  ["--text-xl", "18px"],
  ["--text-2xl", "24px"],
  ["--text-3xl", "34px"],
  ["--font-weight-regular", "400"],
  ["--font-weight-medium", "500"],
  ["--font-weight-bold", "700"],
  ["--tracking-tight", "-0.02em"],
  ["--tracking-caps", "0.06em"],
  ["--leading-ui", "18px"],
  ["--opacity-muted", "60%"],
  ["--breakpoint-desktop", "1440px"],
  ["--spacing-1", "4px"],
  ["--spacing-1-5", "6px"],
  ["--spacing-2", "8px"],
  ["--spacing-3", "12px"],
  ["--spacing-4", "16px"],
  ["--spacing-6", "24px"],
  ["--spacing-8", "32px"],
  ["--radius-xs", "2px"],
  ["--radius-sm", "4px"],
  ["--radius-md", "6px"],
  ["--radius-lg", "10px"],
];

/** A colour written as a value: hexadecimal, a colour function, or a named colour. */
const COLOUR = /#[0-9a-fA-F]{3,8}\b|\b(?:rgba?|hsla?|hwb|lab|lch|oklab|oklch|color)\(|:\s*(?:white|black|red|green|blue|gray|grey|silver|yellow|orange|purple)\b/;

/** Every source but the tokens, the tests and their helpers. */
const SOURCES: Record<string, string> = Object.fromEntries(
  sources([".css", ".ts", ".tsx"])
    .filter((path) => path !== "tokens.css" && !/\.test\.tsx?$/.test(path) && !path.startsWith("test/"))
    .map((path) => [path, source(path)]),
);

describe("tokens", () => {
  const dark = block(tokens, ":root");
  const light = block(tokens, '[data-theme="light"]');

  it("holds each colour token of the Paper file by its exact value, dark under :root and light under the light theme", () => {
    for (const [name, lightValue, darkValue] of PAPER) {
      expect(dark.get(name), name).toBe(darkValue);
      expect(light.get(name), name).toBe(lightValue);
    }
  });

  it("holds the Paper file's type, spacing and radius tokens by their exact values", () => {
    for (const [name, value] of PAPER_ONE) expect(dark.get(name), name).toBe(value);
  });

  it("names every colour of the light theme in the dark one, and the reverse", () => {
    const colours = (set: Map<string, string>) => [...set.keys()].filter((name) => name.startsWith("--color-") || name.startsWith("--shadow-")).sort();
    expect(colours(light)).toEqual(colours(dark));
  });

  it("finds the sources it reads", () => {
    expect(Object.keys(SOURCES).length).toBeGreaterThan(30);
    expect(Object.keys(SOURCES)).toContain("components/components.css");
  });

  it("fails on a colour written anywhere but the tokens", () => {
    const offending: string[] = [];
    for (const [path, text] of Object.entries(SOURCES)) {
      text.split("\n").forEach((line, index) => {
        if (COLOUR.test(line)) offending.push(`${path}:${index + 1}: ${line.trim()}`);
      });
    }
    expect(offending).toEqual([]);
  });

  it("would find a colour written in a component", () => {
    expect(COLOUR.test(".x { color: #1A0806; }")).toBe(true);
    expect(COLOUR.test("background: rgba(0, 0, 0, 0.5)")).toBe(true);
    expect(COLOUR.test("color: white;")).toBe(true);
    expect(COLOUR.test("#app { height: 100%; }")).toBe(false);
    expect(COLOUR.test("mask={`url(#canvas-mask-${index})`}")).toBe(false);
  });
});
