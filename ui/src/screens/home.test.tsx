// Home: a picture picked (from the library or from search) starts a task with it.
import { beforeEach, describe, expect, it, vi } from "vitest";
import { act } from "preact/test-utils";
import { ApiError, type Client, type Hello, type Picture } from "../api";
import { DRAFTS_KEY, draftOf, localStore, saveDraft } from "../drafts";
import { HOSTILE, click, render } from "../test/render";
import { Home } from "./homepage/Home";
import { draftWith, startersOf } from "./homepage/picked";

const PORCH: Picture = { id: `sha256:${"ab".repeat(32)}`, name: "porch.png", width: 64, height: 48 };

const HELLO: Hello = {
  version: "0.1.0",
  kits: [
    {
      id: "rascal",
      name: "Rascal",
      tasks: [
        { name: "remove_prompt", title: "Remove what you name", fields: [{ name: "image", kind: "picture", artifact: "image", required: true }, { name: "prompt", kind: "text" }] },
        { name: "words_only", title: HOSTILE, fields: [{ name: "prompt", kind: "text" }] },
      ],
    },
    { id: "squint", name: "Squint", tasks: [{ name: "brief-compliance", title: "Check against a brief", fields: [{ name: "candidate", kind: "picture", required: true }, { name: "references", kind: "pictures", artifact: "image" }] }] },
  ],
};

/** A client whose library lists `listed` and whose card is of `kept`, or of nothing. */
function client(listed: Picture[] = [PORCH], kept: Picture | null = PORCH): Client {
  const row = (one: Picture) => ({ id: one.id, name: one.name, runs: 0, imported: true });
  return {
    openRuns: vi.fn(() => Promise.resolve([])),
    request: vi.fn((_method: string, path: string) => {
      if (path === "/api/examples") return Promise.resolve({ examples: [] });
      if (path.startsWith("/api/library?")) return Promise.resolve({ pictures: listed.map(row), more: false, counts: { all: listed.length, named: listed.length, inputs: 0, imported: listed.length } });
      if (path.startsWith("/api/library/"))
        return kept ? Promise.resolve({ ...row(kept), width: kept.width, height: kept.height, bytes: 2048, runs_list: [] }) : Promise.reject(new ApiError(404, "the store holds no picture of that identity"));
      return Promise.reject(new ApiError(404, "no such route"));
    }),
    blobUrl: vi.fn(() => new Promise(() => undefined)),
  } as unknown as Client;
}

async function settle() {
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 0));
  });
}

beforeEach(() => localStorage.clear());

describe("a picture picked on Home", () => {
  it("lists every task that takes a picture, with the field it goes in", () => {
    const starters = startersOf(HELLO);
    expect(starters.map((one) => [one.task.name, one.field.name])).toEqual([
      ["remove_prompt", "image"],
      ["brief-compliance", "candidate"],
    ]);
    expect(draftWith(starters[0]!, PORCH, 5)).toEqual({ kit: "rascal", task: "remove_prompt", values: { image: PORCH.id }, pictures: { [PORCH.id]: PORCH }, at: 5 });
  });

  it("opens from a tile, and a task picked opens its draft with the picture given", async () => {
    const go = vi.fn();
    const view = render(<Home hello={HELLO} client={client()} go={go} />);
    await settle();
    click(view.get(".home__pictures .picture-tile"));
    expect(go).toHaveBeenLastCalledWith(`#/?picture=${PORCH.id}`);
    view.rerender(<Home hello={HELLO} client={client()} go={go} picture={PORCH.id} />);
    await settle();
    const panel = view.get("[role=region]");
    expect(panel.getAttribute("aria-label")).toBe("Start a task with porch.png");
    expect(view.all(".picked__task").map((one) => one.textContent)).toEqual(["Remove what you nameRascal · as its image", "Check against a briefSquint · as its candidate"]);
    expect(view.get(".home__pictures .picture-tile--selected")).toBeTruthy();
    click(view.all(".picked__task")[1]!);
    expect(go).toHaveBeenLastCalledWith("#/k/squint/brief-compliance");
    expect(draftOf(localStore(), "brief-compliance")?.values).toEqual({ candidate: PORCH.id });
    view.unmount();
  });

  it("replaces a draft not yet run only once the person says so", async () => {
    saveDraft(localStore(), { kit: "rascal", task: "remove_prompt", values: { prompt: "the hose" }, pictures: {}, at: 1 });
    const go = vi.fn();
    const view = render(<Home hello={HELLO} client={client()} go={go} picture={PORCH.id} />);
    await settle();
    click(view.all(".picked__task")[0]!);
    expect(go).not.toHaveBeenCalled();
    expect(view.get(".picked__replacing").textContent).toContain("This replaces the draft of Remove what you name");
    expect(draftOf(localStore(), "remove_prompt")?.values).toEqual({ prompt: "the hose" });
    click(view.get(".picked__replacing .button--secondary"));
    expect(go).toHaveBeenLastCalledWith("#/k/rascal/remove_prompt");
    expect(draftOf(localStore(), "remove_prompt")?.values).toEqual({ image: PORCH.id });
    expect(localStorage.getItem(DRAFTS_KEY)).toContain(PORCH.id);
    view.unmount();
  });

  it("brings the panel into view once for a picture, not on every render", async () => {
    const scrolled = vi.fn();
    const before = Element.prototype.scrollIntoView;
    Element.prototype.scrollIntoView = scrolled;
    try {
      const made = client();
      const view = render(<Home hello={HELLO} client={made} picture={PORCH.id} />);
      await settle();
      view.rerender(<Home hello={HELLO} client={made} picture={PORCH.id} />);
      await settle();
      view.rerender(<Home hello={HELLO} client={made} picture={PORCH.id} />);
      await settle();
      expect(scrolled).toHaveBeenCalledTimes(1);
      view.unmount();
    } finally {
      Element.prototype.scrollIntoView = before;
    }
  });

  it("looks up a picture past the library's first page by its identity", async () => {
    const made = client([], PORCH);
    const view = render(<Home hello={HELLO} client={made} picture={PORCH.id} />);
    await settle();
    expect(made.request).toHaveBeenCalledWith("GET", `/api/library/${PORCH.id}`);
    expect(view.get("[role=region]").getAttribute("aria-label")).toBe("Start a task with porch.png");
    view.unmount();
  });

  it("says a picture the store does not hold is missing, and offers no task", async () => {
    const view = render(<Home hello={HELLO} client={client([], null)} picture={PORCH.id} />);
    await settle();
    expect(view.get("[role=region] [role=alert]").textContent).toContain("holds no picture");
    expect(view.all(".picked__task")).toHaveLength(0);
    view.unmount();
  });

  it("keeps each chip's answer to its chip, and says a list that failed rather than none", async () => {
    const answers: Record<string, (value: unknown) => void> = {};
    const failures: Record<string, (error: unknown) => void> = {};
    const made = {
      openRuns: vi.fn(() => Promise.resolve([])),
      request: vi.fn((_method: string, path: string) => {
        if (path === "/api/examples") return Promise.resolve({ examples: [] });
        return new Promise((resolve, reject) => {
          answers[path] = resolve;
          failures[path] = reject;
        });
      }),
      blobUrl: vi.fn(() => new Promise(() => undefined)),
    } as unknown as Client;
    const view = render(<Home hello={HELLO} client={made} />);
    await settle();
    const all = "/api/library?show=all&limit=24";
    expect(answers[all]).toBeTruthy();
    click(view.all<HTMLElement>(".home__chip").find((one) => one.textContent?.startsWith("Named"))!);
    await settle();
    const named = "/api/library?show=named&limit=24";
    // The answer for All comes after Named was chosen: it is dropped.
    const row = (id: string) => ({ id, name: id, runs: 0, imported: true });
    const counts = { all: 1, named: 1, inputs: 0, imported: 1 };
    answers[all]!({ pictures: [row(PORCH.id)], more: true, counts });
    await settle();
    expect(view.all(".home__tile")).toHaveLength(0);
    expect(view.text()).not.toContain("No pictures yet");
    failures[named]!(new ApiError(500, "the store could not answer"));
    await settle();
    expect(view.get(".home__pictures [role=alert]").textContent).toContain("the store could not answer");
    expect(view.text()).not.toContain("No pictures yet");
    view.unmount();
  });
});
