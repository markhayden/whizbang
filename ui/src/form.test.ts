// The form, the plan and the step list, from what the host hands the page (plan m5, task W1).
import { describe, expect, it } from "vitest";
import type { HelloBackend, JournalLine, Prepared, Task } from "./api";
import { argumentsOf, choicesFor, defaultChoice, displayed, endTag, hear, PATH_POINTS, planNeeds, seeded, segmentQuery, shapeOf, stepsOf, thinned, unfilled, whyNotRun } from "./form";

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

  it("says checking, not not ready, for a backend hello is not sure of before any preparation answers", () => {
    const field = TASK.fields![2]!;
    const unsure: HelloBackend[] = [...BACKENDS, { capability: "fill", name: "third-fill", ready: false, missing: null }];
    expect(choicesFor(field, null, unsure).map((c) => [c.value, c.readiness])).toEqual([
      ["cloud-fill", "ready"],
      ["other-fill", "no_key"],
      ["third-fill", "checking"],
    ]);
  });

  it("says why Run cannot be pressed, and nothing once a ready preparation has a handle", () => {
    const values = { image: "sha256:a", prompt: "hose" };
    expect(whyNotRun(TASK, {}, null, false)).toBe("Give Image, Prompt.");
    expect(whyNotRun(TASK, values, null, true)).toBe("Preparing…");
    expect(whyNotRun(TASK, values, { ready: false, missing: [{ capability: "judge", kind: "other", why: "the judge is out" }] }, false)).toBe("judge: the judge is out");
    // A key that is not set: the host's plain sentence, which names the variable to set.
    expect(whyNotRun(TASK, values, { ready: false, missing: [{ capability: "fill", kind: "no_key", why: "the filler needs FAL_KEY in your env file" }] }, false)).toBe("The filler needs FAL_KEY in your env file.");
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

describe("review fixes of W1", () => {
  it("thins a long stroke to the host's limit, keeping its ends", () => {
    const points: Array<[number, number]> = Array.from({ length: 2000 }, (_, at) => [at / 4, 50 + 40 * Math.sin(at / 30)]);
    const shape = shapeOf({ kind: "stroke", points, width: 12 }, ["stroke"]);
    expect(shape?.kind).toBe("stroke");
    const kept = shape && "points" in shape ? shape.points : [];
    expect(kept.length).toBeGreaterThan(2);
    expect(kept.length).toBeLessThanOrEqual(PATH_POINTS);
    expect(kept[0]).toEqual(points[0]);
    expect(kept[kept.length - 1]).toEqual(points[points.length - 1]);
    // A straight line of 2,000 points is two; a cap below what simplification keeps holds.
    const straight: Array<[number, number]> = Array.from({ length: 2000 }, (_, at) => [at, at]);
    expect(thinned(straight, PATH_POINTS)).toEqual([[0, 0], [1999, 1999]]);
    expect(thinned(points, 10)).toHaveLength(10);
  });

  it("seeds a choice, a yes-or-no, a small whole number and a backend with what each shows", () => {
    const task: Task = {
      name: "t",
      fields: [
        { name: "mode", kind: "choice", choices: ["a", "b"] },
        { name: "strict", kind: "boolean" },
        { name: "attempts", kind: "integer", min: 1, max: 3, default: 3 },
        { name: "filler", kind: "backend", capability: "fill" },
        { name: "words", kind: "text" },
        { name: "size", kind: "number" },
      ],
    };
    const values = seeded(task, { mode: "b" }, BACKENDS);
    expect(values).toEqual({ mode: "b", strict: false, attempts: 3, filler: "cloud-fill" });
    for (const field of task.fields ?? []) expect(displayed(field, values[field.name], BACKENDS)).toEqual(values[field.name]);
    expect(argumentsOf(task, values)).toEqual(values);
    // Nothing new to seed: the same object back.
    expect(seeded(task, values, BACKENDS)).toBe(values);
  });

  it("names the backend the host resolved, and the default only when it names none", () => {
    const prepared: Prepared = { ready: true, needs: [{ capability: "fill", backend: "other-fill", model: "m", requests: 1, each: 0.04, bound: 0.04 }] };
    expect(planNeeds(prepared, BACKENDS)[0]?.backend).toBe("other-fill");
    const unnamed: Prepared = { ready: true, needs: [{ capability: "fill", backend: "", model: "m", requests: 1, each: 0.04, bound: 0.04 }] };
    expect(planNeeds(unnamed, BACKENDS)[0]?.backend).toBe("cloud-fill");
  });
});
