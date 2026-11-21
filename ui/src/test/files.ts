// The sources as text, for the tests that read them (the tokens and their contrast). The
// tests run in Node, whose types come with the test runner's own packages, so
// no package is added for them (plan m5, task U1, item 1).
import { readdirSync, readFileSync } from "node:fs";

/** The tests run from `ui/`, as `npm test` and `scripts/ui.sh` run them. */
const SRC = "src/";

/** A file under `ui/src/`, as text. */
export function source(path: string): string {
  return readFileSync(`${SRC}${path}`, "utf8");
}

/** Every file under `ui/src/` whose name ends in one of the endings, as paths from there. */
export function sources(endings: string[]): string[] {
  const found: string[] = [];
  const walk = (dir: string) => {
    for (const entry of readdirSync(`${SRC}${dir}`, { withFileTypes: true })) {
      const path = `${dir}${entry.name}`;
      if (entry.isDirectory()) walk(`${path}/`);
      else if (endings.some((ending) => entry.name.endsWith(ending))) found.push(path);
    }
  };
  walk("");
  return found.sort();
}

/** The declarations of one block of tokens.css, as a map. */
export function block(css: string, selector: string): Map<string, string> {
  const start = css.indexOf(`${selector} {`);
  if (start < 0) throw new Error(`no block ${selector}`);
  const body = css.slice(start, css.indexOf("\n}", start));
  const found = new Map<string, string>();
  for (const match of body.matchAll(/(--[a-z0-9-]+):\s*([^;]+);/g)) found.set(match[1]!, match[2]!.trim());
  return found;
}
