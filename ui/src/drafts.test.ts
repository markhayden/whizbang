// Drafts kept in the page's own storage, and storage that refuses (plan m5, Home's Continue).
import { describe, expect, it } from "vitest";
import { DRAFTS_KEY, draftOf, dropDraft, readDrafts, saveDraft, type Store } from "./drafts";

function memory(): Store & { held: Map<string, string> } {
  const held = new Map<string, string>();
  return { held, getItem: (key) => held.get(key) ?? null, setItem: (key, value) => void held.set(key, value) };
}

describe("drafts", () => {
  it("keeps one draft a task, newest first, and forgets it once run", () => {
    const store = memory();
    saveDraft(store, { kit: "rascal", task: "a", values: { prompt: "x" }, pictures: {}, at: 1 });
    saveDraft(store, { kit: "squint", task: "b", values: {}, pictures: {}, at: 2 });
    saveDraft(store, { kit: "rascal", task: "a", values: { prompt: "y" }, pictures: {}, at: 3 });
    expect(readDrafts(store).map((d) => [d.task, d.values.prompt])).toEqual([
      ["a", "y"],
      ["b", undefined],
    ]);
    expect(draftOf(store, "b")?.kit).toBe("squint");
    dropDraft(store, "a");
    expect(readDrafts(store).map((d) => d.task)).toEqual(["b"]);
  });

  it("reads nothing from storage that is missing, garbled or refuses, and never throws", () => {
    expect(readDrafts(null)).toEqual([]);
    const store = memory();
    store.held.set(DRAFTS_KEY, "{not json");
    expect(readDrafts(store)).toEqual([]);
    store.held.set(DRAFTS_KEY, JSON.stringify([{ kit: 1 }, { kit: "r", task: "t", values: {}, at: 1 }]));
    expect(readDrafts(store).map((d) => d.task)).toEqual(["t"]);
    const refusing: Store = {
      getItem: () => {
        throw new Error("denied");
      },
      setItem: () => {
        throw new Error("full");
      },
    };
    expect(readDrafts(refusing)).toEqual([]);
    expect(() => saveDraft(refusing, { kit: "r", task: "t", values: {}, pictures: {}, at: 1 })).not.toThrow();
  });
});
