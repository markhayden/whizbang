// A running run: its steps from typed events, its spend, Stop, and its end.
import { describe, expect, it, vi } from "vitest";
import { act } from "preact/test-utils";
import type { Client, Hello, Polled } from "../api";
import { HOSTILE, click, render } from "../test/render";
import { Run } from "./run/Run";

const HELLO: Hello = {
  version: "0.1.0",
  kits: [{ id: "rascal", name: "Rascal", tasks: [{ name: "remove_prompt", title: "Remove what you name", kind: "recipe", tools: ["segment", "fill"], fields: [{ name: "prompt", kind: "text", label: "What to remove" }] }] }],
};

/** A run's record as the host answers it, ended as `outcome` with this verdict. */
function recorded(run: string, outcome: string, verdict: string | null) {
  return {
    run,
    action: "remove_prompt",
    outcome,
    arguments: [{ name: "prompt", value: "the hose" }],
    inputs: [],
    outputs: [],
    verdict: verdict ? { word: verdict, why: "", reasons: [], places: [] } : null,
    cost: { lines: [], total: 0 },
    send_to: [],
    again: null,
  };
}

function following(outcome = "cancelled", verdict: string | null = null) {
  let answer: (polled: Polled) => void = () => undefined;
  const made = {
    request: vi.fn((_: string, path: string) => Promise.resolve(recorded(path.split("/")[3] ?? "", outcome, verdict))),
    pollRun: vi.fn((_: string, on: (polled: Polled) => void) => {
      answer = on;
      return () => undefined;
    }),
    cancelRun: vi.fn(() => Promise.resolve({ run: "r-1", state: "stopping" })),
    blobUrl: vi.fn(() => new Promise(() => undefined)),
  } as unknown as Client;
  return { made, say: (polled: Polled) => act(() => answer(polled)) };
}

describe("a run", () => {
  it("shows its steps as they start, its spend, and stops when asked", async () => {
    const { made, say } = following();
    const view = render(<Run id="r-1" hello={HELLO} client={made} />);
    say({
      run: "r-1",
      state: "running",
      events: [{ seq: 0, at_ms: Date.now(), message: HOSTILE, event: { kind: "step_started", step: "segment", action: "segment" } }],
      next: 1,
      skipped: 0,
      action: "remove_prompt",
      arguments: { prompt: HOSTILE },
      spent: 0.01,
      reserved: 0.04,
    });
    expect(view.get("h1").textContent).toBe("Remove what you name");
    expect(view.text()).toContain("RUNNING");
    expect(view.all("[data-state]").map((step) => `${step.querySelector(".step-list__name")?.textContent}:${step.getAttribute("data-state")}`)).toEqual(["segment:running", "fill:waiting"]);
    expect(view.text()).toContain("$0.010");
    expect(view.text()).toContain(HOSTILE);
    expect(view.root.querySelector("img[onerror]")).toBeNull();
    click(view.get(".button--danger"));
    expect(made.cancelRun).toHaveBeenCalledWith("r-1");
    expect(view.text()).toContain("Stopping…");
    say({ run: "r-1", state: "ended", outcome: "cancelled", events: [], next: 1, skipped: 0 });
    // The result opens: its record read from the host.
    await act(async () => undefined);
    expect(made.request).toHaveBeenCalledWith("GET", "/api/runs/r-1/record");
    expect(view.text()).toContain("CANCELLED");
    expect(view.all(".button--danger")).toHaveLength(0);
    expect(view.text()).toContain("Run again");
    view.unmount();
  });

  it("tags a check that ended by its verdict", async () => {
    const { made, say } = following("completed", "fail");
    const view = render(<Run id="r-2" hello={HELLO} client={made} />);
    say({ run: "r-2", state: "ended", outcome: "completed", verdict: "fail", events: [], next: 0, skipped: 0 });
    await act(async () => undefined);
    expect(view.text()).toContain("FAIL");
    expect(view.text()).toContain("The check failed");
    view.unmount();
  });
});
