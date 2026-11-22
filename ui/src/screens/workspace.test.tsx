// The kit workspace: a form from a task's fields, the plan, and Run only on a ready preparation.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act } from "preact/test-utils";
import { ApiError, type Client, type Hello, type Prepared } from "../api";
import { HOSTILE, click, input, render } from "../test/render";
import { Workspace } from "./workspace/Workspace";

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
    expect(view.text()).toContain("fill: the backend's key is not set");
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
});
