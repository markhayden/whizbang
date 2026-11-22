// The kit workspace: a form from a task's fields, the plan, and Run only on a ready preparation.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act } from "preact/test-utils";
import { ApiError, type Client, type Hello, type Prepared } from "../api";
import { HOSTILE, click, input, render } from "../test/render";
import { bindingsOf, offered, Workspace } from "./workspace/Workspace";

const HELLO: Hello = {
  version: "0.1.0",
  kits: [
    {
      id: "rascal",
      name: "Rascal",
      tasks: [
        {
          name: "remove_prompt",
          title: "Remove what you name",
          kind: "recipe",
          purpose: HOSTILE,
          tools: ["segment", "fill"],
          fields: [
            { name: "prompt", kind: "text", required: true },
            { name: "attempts", kind: "integer", min: 1, max: 3, default: 3 },
            { name: "filler", kind: "backend", capability: "fill" },
          ],
        },
      ],
    },
  ],
  backends: [
    { capability: "fill", name: "cloud-fill", ready: true, default: true },
    { capability: "fill", name: "keyless-fill", ready: false, missing: "no_key" },
  ],
};

function client(prepare: (args: Record<string, unknown>) => Prepared, start = vi.fn(() => Promise.resolve({ run: "r-1", requestId: "x" }))): Client {
  return {
    prepare: vi.fn((_: string, args: Record<string, unknown>) => Promise.resolve(prepare(args))),
    start,
    openRuns: vi.fn(() => Promise.resolve([])),
    listPictures: vi.fn(() => Promise.resolve([])),
    blobUrl: vi.fn(() => new Promise(() => undefined)),
  } as unknown as Client;
}

async function later(ms: number) {
  await act(async () => {
    vi.advanceTimersByTime(ms);
    await Promise.resolve();
    await Promise.resolve();
  });
}

beforeEach(() => {
  vi.useFakeTimers();
  localStorage.clear();
});
afterEach(() => vi.useRealTimers());

describe("the workspace", () => {
  it("builds its inputs from the task's fields, its text as text", async () => {
    const view = render(<Workspace kit="rascal" task="remove_prompt" hello={HELLO} client={client(() => ({ ready: false }))} />);
    expect(view.get("h1").textContent).toBe("Remove what you name");
    expect(view.text()).toContain(HOSTILE);
    expect(view.root.querySelector("img[onerror]")).toBeNull();
    expect(view.all("input#field-prompt")).toHaveLength(1);
    expect(view.all(".segmented--numbers [role=tab]").map((tab) => tab.textContent)).toEqual(["1", "2", "3"]);
    expect(view.get(".choice-list").textContent).toContain("NO KEY");
    expect(view.text()).toContain("Give Prompt.");
    expect(view.text()).toContain("segment · fill");
    view.unmount();
  });

  it("prepares once a pause after typing, shows the bound, and runs with the preparation's handle", async () => {
    const start = vi.fn(() => Promise.resolve({ run: "r-1", requestId: "x" }));
    const made = client(() => ({ ready: true, handle: "h-1", bound: 0.042, needs: [{ capability: "fill", backend: "cloud-fill", requests: 1, each: 0.04, bound: 0.04 }] }), start);
    const go = vi.fn();
    const view = render(<Workspace kit="rascal" task="remove_prompt" hello={HELLO} client={made} go={go} />);
    input(view.get<HTMLInputElement>("#field-prompt"), "the hose");
    input(view.get<HTMLInputElement>("#field-prompt"), "the garden hose");
    await later(300);
    expect(made.prepare).toHaveBeenCalledTimes(1);
    // What the attempts and the filler show is sent with what was typed.
    expect(made.prepare).toHaveBeenLastCalledWith("remove_prompt", { attempts: 3, filler: "cloud-fill", prompt: "the garden hose" });
    expect(view.text()).toContain("up to $0.042");
    const run = view.get<HTMLButtonElement>(".button--primary");
    expect(run.disabled).toBe(false);
    click(run);
    await later(0);
    expect(start).toHaveBeenCalledWith({ action: "remove_prompt", arguments: { attempts: 3, filler: "cloud-fill", prompt: "the garden hose" }, handle: "h-1" });
    expect(go).toHaveBeenCalledWith("#/run/r-1");
    view.unmount();
  });

  it("keeps Run off for a choice that is not ready, saying what is missing", async () => {
    const made = client(() => ({ ready: false, missing: [{ capability: "fill", kind: "no_key", why: "the backend's key is not set" }] }));
    const view = render(<Workspace kit="rascal" task="remove_prompt" hello={HELLO} client={made} />);
    input(view.get<HTMLInputElement>("#field-prompt"), "hose");
    await later(300);
    expect(view.get<HTMLButtonElement>(".button--primary").disabled).toBe(true);
    expect(view.text()).toContain("The backend's key is not set.");
    view.unmount();
  });

  it("prepares again at once when a start is refused as stale, and says so", async () => {
    let handle = 0;
    const start = vi.fn(() => Promise.reject(new ApiError(409, "The preparation is stale.", "stale")));
    const made = client(() => ({ ready: true, handle: `h-${(handle += 1)}`, bound: 0 }), start);
    const view = render(<Workspace kit="rascal" task="remove_prompt" hello={HELLO} client={made} />);
    input(view.get<HTMLInputElement>("#field-prompt"), "hose");
    await later(300);
    click(view.get(".button--primary"));
    await later(0);
    expect(view.get("[role=alert]").textContent).toContain("prepared again");
    expect(made.prepare).toHaveBeenCalledTimes(2);
    view.unmount();
  });

  it("lists the task's earlier runs from history, a page at a time, and opens one", async () => {
    const page = (from: number, more: boolean) => ({
      runs: Array.from({ length: 10 }, (_, at) => ({ run: `r-${from + at}`, action: "remove_prompt", result: at === 0 ? "cancelled" : "done", words: [`the hose ${from + at}`], at_ms: Date.now() })),
      more,
    });
    const request = vi.fn((_: string, path: string) => Promise.resolve(path.includes("before=r-9") ? page(10, false) : page(0, true)));
    const made = { ...client(() => ({ ready: false })), request } as unknown as Client;
    const go = vi.fn();
    const view = render(<Workspace kit="rascal" task="remove_prompt" hello={HELLO} client={made} go={go} />);
    await flush();
    expect(request).toHaveBeenCalledWith("GET", "/api/runs?task=remove_prompt&limit=10");
    const rows = () => view.all(".workspace__runs .continue-row");
    expect(rows()).toHaveLength(10);
    expect(rows()[0]!.textContent).toContain("the hose 0");
    expect(rows()[0]!.textContent).toContain("CANCELLED");
    click(view.get(".workspace__runs .link-button"));
    await flush();
    expect(request).toHaveBeenLastCalledWith("GET", "/api/runs?task=remove_prompt&limit=10&before=r-9");
    expect(rows()).toHaveLength(20);
    expect(view.all(".workspace__runs .link-button")).toHaveLength(0);
    click(rows()[12]!);
    expect(go).toHaveBeenLastCalledWith("#/run/r-12");
    view.unmount();
  });

  it("shows Mask and Result once the last run made them, and a mask port offers the store's masks", async () => {
    const mask = `sha256:${"11".repeat(32)}`;
    const result = `sha256:${"22".repeat(32)}`;
    const input = `sha256:${"33".repeat(32)}`;
    const ran = { run: "r-last", action: "protect", title: "Protect a first edit", result: "done", at_ms: Date.now(), thumb: input, outputs: [{ port: "result", type: "image", id: result }, { port: "mask", type: "mask", id: mask }] };
    const request = vi.fn((_: string, path: string) => Promise.resolve({ runs: [ran], more: path.includes("output=mask") }));
    const hello: Hello = {
      version: "0.1.0",
      kits: [{ id: "rascal", name: "Rascal", tasks: [{ name: "protect", title: "Protect a first edit", fields: [{ name: "image", kind: "picture", artifact: "image" }, { name: "mask", kind: "artifact", artifact: "mask" }] }] }],
    };
    const made = { ...client(() => ({ ready: false })), request } as unknown as Client;
    const view = render(<Workspace kit="rascal" task="protect" hello={hello} client={made} />);
    await flush();
    expect(view.all(".workspace__tabs [role=tab]").map((tab) => tab.textContent)).toEqual(["Original", "Mask", "Result"]);
    expect(request).toHaveBeenCalledWith("GET", "/api/runs?output=mask&limit=20");
    const option = view.get(".binding-picker [role=option]");
    expect(option.textContent).toContain("Protect a first edit");
    expect(option.textContent).toContain("r-last");
    // A later page of masks, after the last run shown.
    click(view.get(".binding-picker + .link-button"));
    await flush();
    expect(request).toHaveBeenLastCalledWith("GET", "/api/runs?output=mask&limit=20&before=r-last");
    click(view.get(".workspace__tabs [role=tab]:nth-child(2)"));
    expect(view.text()).toContain("The mask of the last run, r-last, over its picture.");
    click(view.get(".workspace__tabs [role=tab]:nth-child(3)"));
    expect(view.text()).toContain("The result of the last run, r-last.");
    view.unmount();
  });

  it("reads history again once a run going ends, and drops the row given to this draft once the store lists it", async () => {
    const mask = `sha256:${"66".repeat(32)}`;
    let ended = false;
    const going = [{ run: "r-going", action: "remove_prompt", started_ms: 1, state: "running" }];
    const listed = { run: "r-going", action: "remove_prompt", title: "Remove what you name", result: "done", at_ms: Date.now(), outputs: [{ port: "mask", type: "mask", id: mask }] };
    const request = vi.fn((_: string, path: string) => Promise.resolve({ runs: ended && path.includes("output=mask") ? [listed] : [], more: false }));
    const hello: Hello = { version: "0.1.0", kits: [{ id: "rascal", name: "Rascal", tasks: [{ name: "protect", title: "Protect a first edit", fields: [{ name: "mask", kind: "artifact", artifact: "mask" }] }] }] };
    localStorage.setItem("whizbang.drafts", JSON.stringify([{ kit: "rascal", task: "protect", values: { mask }, pictures: {}, at: 1 }]));
    const made = { ...client(() => ({ ready: false })), request, openRuns: vi.fn(() => Promise.resolve(ended ? [] : going)) } as unknown as Client;
    const view = render(<Workspace kit="rascal" task="protect" hello={hello} client={made} />);
    await flush();
    const rows = () => view.all(".binding-picker [role=option]").map((one) => one.textContent ?? "");
    expect(rows()).toHaveLength(1);
    expect(rows()[0]).toContain("given to this draft");
    const asked = request.mock.calls.length;
    ended = true;
    await later(2000);
    await flush();
    expect(request.mock.calls.length).toBeGreaterThan(asked);
    expect(rows()).toHaveLength(1);
    expect(rows()[0]).toContain("Remove what you name");
    expect(rows()[0]).not.toContain("given to this draft");
    view.unmount();
  });

  it("lists a run that ended between the first read of history and the first look at the runs going", async () => {
    // The screen opens right after a run ends: history is read before the store lists the
    // run, and the runs going, asked beside it, are answered after it has ended, so the
    // run is never seen going. The lists are read again once that first look answers.
    const mask = `sha256:${"77".repeat(32)}`;
    let ended = false;
    const listed = { run: "r-just", action: "protect", title: "Protect a first edit", words: ["the hose"], result: "done", at_ms: Date.now(), outputs: [{ port: "mask", type: "mask", id: mask }] };
    const request = vi.fn(() => Promise.resolve({ runs: ended ? [listed] : [], more: false }));
    let answerOpen: (runs: never[]) => void = () => undefined;
    const openRuns = vi.fn(() => new Promise<never[]>((resolve) => (answerOpen = resolve)));
    const hello: Hello = { version: "0.1.0", kits: [{ id: "rascal", name: "Rascal", tasks: [{ name: "protect", title: "Protect a first edit", fields: [{ name: "mask", kind: "artifact", artifact: "mask" }] }] }] };
    const made = { ...client(() => ({ ready: false })), request, openRuns } as unknown as Client;
    const view = render(<Workspace kit="rascal" task="protect" hello={hello} client={made} />);
    await flush();
    expect(view.text()).toContain("Not run yet");
    ended = true;
    await act(async () => answerOpen([]));
    await flush();
    expect(view.text()).not.toContain("Not run yet");
    expect(view.get(".workspace__runs .continue-row").textContent).toContain("the hose");
    expect(view.get(".binding-picker [role=option]").textContent).toContain("the hose");
    view.unmount();
  });

  it("says why history could not be read, in place of the empty lists", async () => {
    const request = vi.fn(() => Promise.reject(new ApiError(501, "this build keeps no history")));
    const hello: Hello = { version: "0.1.0", kits: [{ id: "rascal", name: "Rascal", tasks: [{ name: "protect", title: "Protect a first edit", fields: [{ name: "mask", kind: "artifact", artifact: "mask" }] }] }] };
    const made = { ...client(() => ({ ready: false })), request } as unknown as Client;
    const view = render(<Workspace kit="rascal" task="protect" hello={hello} client={made} />);
    await flush();
    expect(view.get(".workspace__right [role=alert]").textContent).toContain("this build keeps no history");
    expect(view.text()).not.toContain("Not run yet");
    expect(view.get(".field [role=alert]").textContent).toContain("Earlier mask outputs could not be read: this build keeps no history");
    expect(view.text()).not.toContain("No earlier run made a mask yet");
    view.unmount();
  });

  it("offers a port's value that history does not list, as given to this draft", () => {
    const given = `sha256:${"44".repeat(32)}`;
    const listed = [{ id: `sha256:${"55".repeat(32)}`, type: "mask", run: "r-1", task: "Remove", when: "now" }];
    expect(offered(listed, "mask", given, false).map((one) => [one.id, one.task])).toEqual([
      [given, "given to this draft"],
      [listed[0]!.id, "Remove"],
    ]);
    expect(offered(listed, "mask", listed[0]!.id, false)).toEqual(listed);
    expect(offered(listed, "location", given, true)).toEqual(listed);
  });
});

async function flush() {
  for (let at = 0; at < 6; at += 1) await later(0);
}

describe("the outputs a binding picker offers", () => {
  it("say what each run was asked, as its runs list says it", () => {
    const runs = [
      { run: "r-2", action: "remove_checked", title: "Remove, checked", words: ["the rake"], pictures: ["yard.png"], result: "pass" as const, at_ms: 1000, outputs: [{ port: "mask", type: "mask", id: "sha256:2" }] },
      { run: "r-1", action: "remove_checked", title: "Remove, checked", words: [], pictures: [], result: "pass" as const, at_ms: 1000, outputs: [{ port: "mask", type: "mask", id: "sha256:1" }] },
    ];
    expect(bindingsOf(runs, "mask", 1000).map((one) => [one.id, one.asked, one.task])).toEqual([
      ["sha256:2", "the rake · yard.png", "Remove, checked"],
      ["sha256:1", undefined, "Remove, checked"],
    ]);
  });
});
