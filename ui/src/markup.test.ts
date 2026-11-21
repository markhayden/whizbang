// No source of the interface puts text into the page as markup: every text a component is
// given is put in as text (plan m5, task U1, item 3). A source that reaches for a way to
// write markup fails here, so the one wrapper that W3 adds for a tool's record (shown only
// inside the isolated frame) must be allowed below by its path, on purpose and reviewed.
import { describe, expect, it } from "vitest";
import { source, sources } from "./test/files";

/** The ways to write markup into a document. */
const MARKUP = /dangerouslySetInnerHTML|\binnerHTML\b|\bouterHTML\s*=|insertAdjacentHTML|document\.write\s*\(|createContextualFragment|\bsrcdoc\b/;

/** Sources allowed to write markup, each with its reason. None yet. */
const ALLOWED = new Map<string, string>();

describe("markup", () => {
  it("is written by no source of the interface", () => {
    const offending: string[] = [];
    for (const path of sources([".ts", ".tsx"]).filter((p) => p !== "markup.test.ts" && !ALLOWED.has(p))) {
      source(path)
        .split("\n")
        .forEach((line, index) => {
          if (MARKUP.test(line)) offending.push(`${path}:${index + 1}: ${line.trim()}`);
        });
    }
    expect(offending).toEqual([]);
  });

  it("would find each way of writing it", () => {
    for (const line of [
      "<div dangerouslySetInnerHTML={{ __html: text }} />",
      "element.innerHTML = text;",
      "element.outerHTML = text;",
      'element.insertAdjacentHTML("beforeend", text);',
      "document.write(text);",
      "range.createContextualFragment(text);",
      "<iframe srcdoc={text} />",
    ]) {
      expect(MARKUP.test(line), line).toBe(true);
    }
    expect(MARKUP.test("element.textContent = text;")).toBe(false);
  });
});
