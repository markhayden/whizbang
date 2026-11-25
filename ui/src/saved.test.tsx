// Saved-run comparisons, saved examples and Home's library (plan m7, task U1): the pure
// helpers, the compare route, and the examples list holding a person's words as text.
import { beforeEach, describe, expect, it, vi } from "vitest";
import { act } from "preact/test-utils";
import type { Client } from "./api";
import { ExampleList } from "./components/ExampleList";
import { draftOf, localStore } from "./drafts";
import { hrefOf, parseRoute } from "./routes";
import { draftOfExample, fellBackWords, PIN_KEY, readPin, rowsShown, savePin, shownName, tileLine, type CompareRow, type Example, type OpenedExample } from "./saved";
import { HOSTILE, click, render } from "./test/render";

const PICTURE = `sha256:${"cd".repeat(32)}`;

beforeEach(() => localStorage.clear());

async function settle() {
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 0));
  });
}

describe("a comparison", () => {
  it("is addressed by its baseline, then its attempt, and back", () => {
    expect(parseRoute("#/compare/1795-1/1795-2")).toEqual({ name: "compare", baseline: "1795-1", attempt: "1795-2" });
    expect(parseRoute("#/compare/1795-1")).toEqual({ name: "compare", baseline: "1795-1" });
    expect(parseRoute("#/compare/1795-1/a/b").name).toBe("unknown");
    expect(hrefOf({ name: "compare", baseline: "1795-1", attempt: "1795-2" })).toBe("#/compare/1795-1/1795-2");
  });

  it("shows what differs, or every row", () => {
    const rows: CompareRow[] = [
      { section: "asked", name: "prompt", baseline: "a", attempt: "b", differs: true },
      { section: "inputs", name: "image", baseline: PICTURE, attempt: PICTURE, differs: false },
    ];
    expect(rowsShown(rows, false).map((row) => row.name)).toEqual(["prompt"]);
    expect(rowsShown(rows, true)).toHaveLength(2);
  });

  it("keeps the pinned baseline in the browser's storage, and forgets it once cleared", () => {
    expect(readPin(localStore())).toBeNull();
    savePin(localStore(), { run: "1795-1", action: "remove_prompt" });
    expect(readPin(localStore())).toEqual({ run: "1795-1", action: "remove_prompt" });
    savePin(localStore(), null);
    expect(readPin(localStore())).toBeNull();
    localStorage.setItem(PIN_KEY, "not json");
    expect(readPin(localStore())).toBeNull();
  });
});

describe("the library", () => {
  it("names a picture by the name a person gave, else by its identity, and says its runs or its import", () => {
    expect(shownName({ id: PICTURE, name: "porch.png" })).toBe("porch.png");
    expect(shownName({ id: PICTURE, name: null })).toContain("…");
    expect(tileLine({ id: PICTURE, name: null, runs: 2, imported: false })).toBe("2 runs");
    expect(tileLine({ id: PICTURE, name: null, runs: 1, imported: true })).toBe("1 run");
    expect(tileLine({ id: PICTURE, name: "x", runs: 0, imported: true })).toBe("imported · not run yet");
  });
});

const OPENED: OpenedExample = {
  task: "remove_prompt",
  kit: "rascal",
  title: "Remove what you name",
  values: { image: PICTURE, prompt: "the hose" },
  example: { id: "0123456789abcdef", name: "Porch hose", notes: "", run: "1795-1" },
  fell_back: [{ argument: "backend", value: "sam3-host", capability: "segment", kind: "not_in_config", why: "this config names no backend sam3-host now" }],
};

describe("an example", () => {
  it("opens as a draft that names it and what fell back", () => {
    const draft = draftOfExample(OPENED, 7);
    expect(draft).toMatchObject({ kit: "rascal", task: "remove_prompt", values: { image: PICTURE, prompt: "the hose" }, at: 7 });
    expect(draft.example).toEqual({ id: "0123456789abcdef", name: "Porch hose", fellBack: OPENED.fell_back });
    expect(Object.keys(draft.pictures)).toEqual([PICTURE]);
    expect(fellBackWords(OPENED.fell_back[0]!, "segmenter")).toBe("The example asked for the segmenter sam3-host; this config names no such backend now, so this draft uses the config's default.");
  });

  it("is listed with its words as text, opened as a draft, and one whose task is gone is not opened", async () => {
    const examples: Example[] = [
      { id: "0123456789abcdef", name: HOSTILE, notes: HOSTILE, task: "remove_prompt", title: "Remove what you name", kit: "rascal", known: true, run: "1795-1", at_ms: 1, picture: PICTURE, inputs: [], backends: {} },
      { id: "fedcba9876543210", name: "Pool chairs, old judge", notes: "", task: "check-legacy", title: null, kit: null, known: false, run: "1795-2", at_ms: 0, picture: null, inputs: [], backends: {} },
    ];
    const request = vi.fn((method: string, path: string) => {
      if (method === "GET" && path === "/api/examples") return Promise.resolve({ examples });
      if (method === "POST" && path === "/api/examples/0123456789abcdef/open") return Promise.resolve(OPENED);
      return Promise.reject(new Error(path));
    });
    const client = { request, blobUrl: vi.fn(() => new Promise(() => undefined)) } as unknown as Client;
    const go = vi.fn();
    const view = render(<ExampleList client={client} go={go} kitName={(kit) => (kit === "rascal" ? "Rascal" : (kit ?? ""))} />);
    await settle();
    expect(view.all("img[onerror]")).toHaveLength(0);
    expect((window as unknown as { __ran?: number }).__ran).toBeUndefined();
    expect(view.text()).toContain(HOSTILE);
    expect(view.text()).toContain("2 of up to 1,000");
    expect(view.text()).toContain("this build has no task “check-legacy”");
    expect(view.all(".examples__row--gone button.examples__name")).toHaveLength(0);
    click(view.get(".examples__row:not(.examples__row--gone) .examples__name"));
    await settle();
    expect(go).toHaveBeenCalledWith("#/k/rascal/remove_prompt");
    expect(draftOf(localStore(), "remove_prompt")?.example?.name).toBe("Porch hose");
    // Nothing was started: the only calls were the list and the open.
    expect(request.mock.calls.map(([method, path]) => `${method} ${path}`)).toEqual(["GET /api/examples", "POST /api/examples/0123456789abcdef/open"]);
    view.unmount();
  });
});
