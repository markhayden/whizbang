// The form, the plan and the step list, from what the host hands the page (plan m5, task W1).
import { describe, expect, it } from "vitest";
import type { HelloBackend, JournalLine, Prepared, Task } from "./api";
import { argumentsOf, choicesFor, defaultChoice, endTag, hear, planNeeds, segmentQuery, shapeOf, stepsOf, unfilled, whyNotRun } from "./form";

const TASK: Task = {
  name: "remove_prompt",
  kit: "rascal",
  kind: "recipe",
  tools: ["segment", "select", "fill"],
  fields: [
    { name: "image", kind: "picture", required: true, artifact: "image" },
    { name: "prompt", kind: "text", required: true },
    { name: "filler", kind: "backend", capability: "fill" },
    { name: "images", kind: "pictures", artifact: "image", count: { min: 1, max: 3 } },
  ],
};

const BACKENDS: HelloBackend[] = [
  { capability: "fill", name: "cloud-fill", ready: true, default: true },
  { capability: "fill", name: "other-fill", ready: false, missing: "no_key", why: "not ready: no key" },
  { capability: "judge", name: "cloud-judge", ready: true },
];

function line(seq: number, at: number, event: JournalLine["event"]): JournalLine {
  return { seq, at_ms: at, message: "said", event };
}

describe("the form", () => {
  it("sends each field that says something, trimmed, and nothing blank", () => {
    expect(argumentsOf(TASK, { image: "sha256:a", prompt: "  the hose ", filler: "", images: [], other: "x" })).toEqual({ image: "sha256:a", prompt: "the hose" });
    expect(unfilled(TASK, { prompt: " " })).toEqual(["Image", "Prompt"]);
  });

  it("offers a backend field's choices from its own preparations, else from hello", () => {
    const field = TASK.fields![2]!;
    expect(choicesFor(field, null, BACKENDS).map((c) => [c.value, c.readiness])).toEqual([
      ["cloud-fill", "ready"],
      ["other-fill", "no_key"],
    ]);
    const prepared: Prepared = { ready: true, choices: { filler: [{ value: "sam3", ready: false, kind: "not_installed", size: 3_400_000_000 }] } };
    expect(choicesFor(field, prepared, BACKENDS)).toEqual([{ value: "sam3", label: "sam3", readiness: "not_installed", size: 3_400_000_000, why: undefined }]);
    expect(defaultChoice(field, BACKENDS)).toBe("cloud-fill");
  });

  it("says why Run cannot be pressed, and nothing once a ready preparation has a handle", () => {
    const values = { image: "sha256:a", prompt: "hose" };
    expect(whyNotRun(TASK, {}, null, false)).toBe("Give Image, Prompt.");
    expect(whyNotRun(TASK, values, null, true)).toBe("Preparing…");
    expect(whyNotRun(TASK, values, { ready: false, missing: [{ capability: "judge", kind: "no_key", why: "the key is not set" }] }, false)).toBe("judge: the key is not set");
    expect(whyNotRun(TASK, values, { ready: false, refused: true, why: "no such picture" }, false)).toBe("no such picture");
    expect(whyNotRun(TASK, values, { ready: true, handle: "h" }, false)).toBeNull();
  });

  it("lays out each need's bound for the plan block", () => {
    expect(planNeeds({ ready: true, needs: [{ capability: "fill", backend: "cloud-fill", model: "fill", requests: 2, each: 0.04, bound: 0.08 }] })).toEqual([
      { capability: "fill", backend: "cloud-fill", model: "fill", requests: 2, each: 0.04, total: 0.08, local: undefined },
    ]);
  });

  it("turns what was drawn into the shape a location takes, and segment's query", () => {
    expect(shapeOf({ kind: "box", x: 2, y: 3, w: 10, h: 5 }, ["box", "polygon"])).toEqual({ kind: "box", left: 2, top: 3, right: 12, bottom: 8 });
    expect(shapeOf({ kind: "box", x: 0, y: 0, w: 1, h: 1 }, ["polygon"])?.kind).toBe("polygon");
    expect(shapeOf({ kind: "stroke", points: [[0, 0], [4, 4]], width: 12 }, ["stroke"])).toEqual({ kind: "stroke", points: [[0, 0], [4, 4]], radius: 6 });
    expect(shapeOf({ kind: "stroke", points: [[0, 0], [4, 4]], width: 12 }, ["box"])).toBeNull();
    expect(segmentQuery({ kind: "point", x: 5, y: 6 })).toEqual({ kind: "point", query: "5 6" });
    expect(segmentQuery({ kind: "box", x: 1, y: 2, w: 3, h: 4 })).toEqual({ kind: "box", query: "1 2 4 6" });
  });

  it("tags an ended run by its verdict, else its outcome", () => {
    expect(endTag("completed", "fail")).toBe("fail");
    expect(endTag("completed", null)).toBe("done");
    expect(endTag("cancelled", undefined)).toBe("cancelled");
    expect(endTag("invalid", undefined)).toBe("error");
  });
});

describe("the step list", () => {
  it("marks each step as it starts and ends, and the recipe's steps still to come as waiting", () => {
    const heard = hear([], [line(0, 1000, { kind: "step_started", step: "segment", action: "segment" }), line(1, 1500, { kind: "step_ended", step: "segment", outcome: "completed", run: "r-1" }), line(2, 1600, { kind: "step_started", step: "select", action: "select" })], 0, 0);
    const entries = stepsOf(heard, ["segment", "select", "fill"], 2000, true);
    expect(entries.map((e) => (e.kind === "step" ? [e.name, e.state, e.ms] : [e.kind]))).toEqual([
      ["segment", "done", 500],
      ["select", "running", 400],
      ["fill", "waiting", undefined],
    ]);
    expect(stepsOf(heard, ["segment", "select", "fill"], 2000, false).some((e) => e.kind === "step" && e.state === "waiting")).toBe(false);
  });

  it("puts a divider before each attempt after the first, a gap where events were not kept, and a failed step's outcome", () => {
    let heard = hear([], [line(0, 0, { kind: "attempt_started", number: 1 }), line(1, 0, { kind: "step_started", step: "remove", action: "remove_prompt", attempt: 1 }), line(2, 10, { kind: "step_ended", step: "remove", outcome: "completed", run: "r-2", attempt: 1 }), line(3, 10, { kind: "step_started", step: "check", action: "check", attempt: 1 }), line(4, 20, { kind: "step_ended", step: "check", outcome: "error", run: "r-3", attempt: 1 })], 0, 0);
    heard = hear(heard, [line(9, 30, { kind: "attempt_started", number: 2, changed: "widen the mask" }), line(10, 30, { kind: "step_started", step: "remove", action: "remove_prompt", attempt: 2 })], 4, 5);
    const entries = stepsOf(heard, [], 40, true);
    expect(entries.map((e) => (e.kind === "step" ? `${e.name}:${e.state}` : e.kind === "attempt" ? `attempt ${e.number} ${e.change}` : `skipped ${e.count}`))).toEqual([
      "remove:done",
      "check:failed",
      "skipped 4",
      "attempt 2 widen the mask",
      "remove:running",
      "check:waiting",
    ]);
  });
});

describe("hello as the page reads it", () => {
  it("takes backends that are not a list as none, and lists every task and tool with its kit", async () => {
    const { backendsOf, tasksOf } = await import("./api");
    expect(backendsOf({ version: "1", backends: { refused: "x" } as unknown as [] })).toEqual([]);
    expect(backendsOf(null)).toEqual([]);
    const all = tasksOf({ version: "1", kits: [{ id: "squint", tasks: [{ name: "check" }] }], tools: [{ name: "view", kind: "primitive", kit: null }] });
    expect(all.map((task) => [task.name, task.kit])).toEqual([
      ["check", "squint"],
      ["view", null],
    ]);
  });
});
