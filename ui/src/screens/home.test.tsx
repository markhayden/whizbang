// Home: a picture picked (from the library or from search) starts a task with it.
import { beforeEach, describe, expect, it, vi } from "vitest";
import { act } from "preact/test-utils";
import type { Client, Hello, Picture } from "../api";
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

function client(): Client {
  return {
    openRuns: vi.fn(() => Promise.resolve([])),
    listPictures: vi.fn(() => Promise.resolve([PORCH])),
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
});
