// The result (plan m5, task W2): what the page makes of a run's record, and the screen.
import { describe, expect, it, vi } from "vitest";
import { act } from "preact/test-utils";
import type { Client, Hello } from "../../api";
import { DRAFTS_KEY, readDrafts } from "../../drafts";
import type { Heard } from "../../form";
import { HOSTILE, click, expectShownAsText, render } from "../../test/render";
import { withJob } from "../workspace/Workspace";
import { checksEnded, costLines, draftFrom, flagsOf, nothingFound, targetsOf, verdictTitle, type RunRecordView } from "./record";
import { Result, unusedWhy } from "./Result";

const A = `sha256:${"a".repeat(64)}`;
const B = `sha256:${"b".repeat(64)}`;
const M = `sha256:${"c".repeat(64)}`;

const HELLO: Hello = {
  version: "0.1.0",
  kits: [
    { id: "rascal", name: "Rascal", tasks: [{ name: "remove_checked", title: "Remove, checked", kind: "loop", uses: "squint", fields: [{ name: "image", kind: "picture" }, { name: "prompt", kind: "text", label: "What to remove" }] }] },
    { id: "squint", name: "Squint", tasks: [{ name: "source-fidelity", title: "Check an edit", kind: "recipe", fields: [{ name: "candidate", kind: "picture" }, { name: "source", kind: "picture" }, { name: "brief", kind: "text" }] }] },
  ],
};

const RECORD: RunRecordView = {
  run: "1795-1",
  action: "remove_checked",
  title: "Remove, checked",
  outcome: "completed",
  start_ms: 1_000,
  end_ms: 73_000,
  arguments: [{ name: "prompt", value: HOSTILE }],
  inputs: [{ port: "image", type: "image", id: A }],
  outputs: [
    { port: "image", type: "image", id: B },
    { port: "mask", type: "mask", id: M },
  ],
  verdict: { word: "pass", why: HOSTILE, reasons: [HOSTILE], places: [] },
  attempts: {
    list: [
      { attempt: 1, verdict: "fail", unchecked: null, changed: null, cost: 0.042, picture: M, diagnostic: true },
      { attempt: 2, verdict: "pass", unchecked: null, changed: "wider mask, 8 px", cost: 0.042, picture: B, diagnostic: true },
    ],
    kept: 2,
    ended: "attempt 2 passed",
  },
  cost: {
    lines: [
      { capability: "segment", backend: "local-seg", model: "sam3", requests: 1, cost: 0, free: true },
      { capability: "fill", backend: "cloud-fill", model: "fill", requests: 2, cost: 0.08, free: false },
    ],
    total: 0.08,
  },
  reserved: 0.14,
  send_to: [{ task: "source-fidelity", kit: "squint", title: "Check an edit", values: { source: A, candidate: B, brief: "the brief" }, from: ["source", "candidate", "brief"], line: "the source, the edit and the words", job_of: "1795-1" }],
  again: { task: "remove_checked", kit: "rascal", title: "Remove, checked", values: { image: A, prompt: "the hose", filler: "cloud-fill" }, from: [] },
};

function client(): Client {
  return {
    request: vi.fn((_: string, path: string) =>
      Promise.resolve(path.endsWith("/command") ? { command: "whizbang rascal remove-checked --prompt 'the hose'", words: [], config_changed: true } : RECORD),
    ),
    blobUrl: vi.fn(() => new Promise(() => undefined)),
  } as unknown as Client;
}

describe("what the page makes of a record", () => {
  it("says a judge no key let run was not used, and nothing of one that ran", () => {
    const unchecked: RunRecordView = {
      ...RECORD,
      verdict: null,
      cost: { lines: [{ capability: "fill", backend: "cloud-fill", model: "fill", requests: 1, cost: 0.04, free: false }], total: 0.04 },
      attempts: { list: [{ attempt: 1, verdict: null, unchecked: "no judge is ready: the judge needs GEMINI_KEY in your env file", changed: null, cost: 0.04, picture: B, diagnostic: true }], kept: 1, ended: "one unchecked pass: no judge is ready: the judge needs GEMINI_KEY in your env file" },
    };
    expect(unusedWhy(unchecked, "judge")).toBe("no key");
    expect(unusedWhy(unchecked, "fill")).toBeNull();
    expect(unusedWhy(RECORD, "judge")).toBeNull();
  });

  it("says plainly that nothing matched a prompt, and nothing for any other end", () => {
    const invalid = { invalid: null, error: { kind: "invalid", message: "the step segment (segment) found nothing, so the step select (select) had no mask to take" }, arguments: [{ name: "prompt", value: "the thing" }] };
    expect(nothingFound(invalid)).toBe('Nothing in the picture matched "the thing". Name what to remove more exactly.');
    expect(nothingFound({ ...invalid, arguments: [] })).toContain("Nothing was found in the picture");
    expect(nothingFound({ invalid: null, error: { kind: "failed", message: "the filler timed out" }, arguments: [] })).toBeNull();
    expect(nothingFound({ invalid: null, error: null, arguments: [] })).toBeNull();
  });

  it("says the cost by capability and backend, free local work as free", () => {
    expect(costLines(RECORD.cost)).toEqual([
      { what: "segment", backend: "local-seg local", cost: 0 },
      { what: "fill × 2", backend: "cloud-fill", cost: 0.08 },
    ]);
    expect(costLines(undefined)).toEqual([]);
    expect(verdictTitle("pass")).toBe("Check passed");
    expect(verdictTitle("unsure")).toBe("Not sure");
  });

  it("draws a verdict's places on their own picture, in its pixels", () => {
    const places = [
      { image: A, box: [0.1, 0.2, 0.5, 0.6] as [number, number, number, number] },
      { image: B, box: [0, 0, 1, 1] as [number, number, number, number] },
      { image: A, polygon: [[0.5, 0.5], [0.75, 0.5], [0.75, 1]] as Array<[number, number]> },
    ];
    expect(flagsOf(places, A, 100, 200)).toEqual([
      { kind: "flag", x: 10, y: 40, w: 40, h: 80, words: "place 1" },
      { kind: "flag", x: 50, y: 100, w: 25, h: 100, words: "place 3" },
    ]);
    expect(flagsOf(places, null, 100, 100)).toEqual([]);
  });

  it("finds each check a loop made once it has ended, by its run", () => {
    const heard: Heard = [
      { seq: 0, at_ms: 0, message: "", event: { kind: "step_started", step: "find", action: "remove_prompt", attempt: 1 } },
      { seq: 1, at_ms: 0, message: "", event: { kind: "step_ended", step: "find", outcome: "completed", run: "r-find", attempt: 1 } },
      { seq: 2, at_ms: 0, message: "", event: { kind: "step_started", step: "check", action: "source-fidelity", attempt: 1 } },
      { skipped: 3, seq: 3 },
      { seq: 6, at_ms: 0, message: "", event: { kind: "step_ended", step: "check", outcome: "completed", run: "r-check", attempt: 1 } },
      { seq: 7, at_ms: 0, message: "", event: { kind: "step_started", step: "check", action: "source-fidelity", attempt: 2 } },
    ];
    expect(checksEnded(heard, new Set(["source-fidelity"]))).toEqual(["r-check"]);
  });

  it("opens a sent draft with where it came from, and a draft run again as a copy", () => {
    const sent = draftFrom(RECORD.send_to![0]!, RECORD, HELLO.kits!, true, 9);
    expect(sent).toMatchObject({ kit: "squint", task: "source-fidelity", values: { source: A, candidate: B }, at: 9 });
    expect(sent.sent).toEqual({ run: "1795-1", kit: "Rascal", title: "Remove, checked", at: 73_000, fields: ["source", "candidate", "brief"], jobOf: "1795-1" });
    expect(sent.pictures[B]?.name).toContain("result");
    const again = draftFrom(RECORD.again!, RECORD, HELLO.kits!, false, 9);
    expect(again.sent).toBeUndefined();
    again.values.prompt = "something else";
    expect(RECORD.again!.values.prompt).toBe("the hose");
    expect(targetsOf(RECORD.send_to!, HELLO.kits!)).toEqual([{ id: "source-fidelity", title: "Squint · Check an edit", line: "the source, the edit and the words", tone: "volt" }]);
  });

  it("sends a check the run's job and no brief beside it", () => {
    expect(withJob({ candidate: B, brief: "x" }, "1795-1")).toEqual({ candidate: B, job_of: "1795-1" });
    expect(withJob({ candidate: B, brief: "x" }, null)).toEqual({ candidate: B, brief: "x" });
  });
});

describe("the result screen", () => {
  it("says once, plainly, why a run was not checked, and that its judge was not used", async () => {
    const why = "no judge is ready: the judge needs GEMINI_KEY in your env file";
    const unchecked: RunRecordView = {
      ...RECORD,
      verdict: null,
      arguments: [{ name: "prompt", value: "the hose" }, { name: "judge", value: "gemini-4-argon" }],
      cost: { lines: [{ capability: "fill", backend: "cloud-fill", model: "fill", requests: 1, cost: 0.04, free: false }], total: 0.04 },
      attempts: { list: [{ attempt: 1, verdict: null, unchecked: why, changed: null, cost: 0.04, picture: B, diagnostic: true }], kept: 1, ended: `one unchecked pass: ${why}` },
    };
    const hello: Hello = { ...HELLO, kits: [{ ...HELLO.kits![0]!, tasks: [{ ...HELLO.kits![0]!.tasks![0]!, fields: [...HELLO.kits![0]!.tasks![0]!.fields!, { name: "judge", kind: "backend", capability: "judge" }] }] }, HELLO.kits![1]!] };
    const made = { request: vi.fn(() => Promise.resolve(unchecked)), blobUrl: vi.fn(() => new Promise(() => undefined)) } as unknown as Client;
    const view = render(<Result id="1795-1" hello={hello} client={made} go={() => undefined} />);
    await act(async () => undefined);
    expect(view.text().split("needs GEMINI_KEY in your env file")).toHaveLength(2);
    expect(view.text()).not.toContain("key_env");
    expect(view.text()).toContain("gemini-4-argon · not used (no key)");
  });

  it("shows the verdict, how it got there, the cost, and every text of the run as text", async () => {
    const made = client();
    const view = render(<Result id="1795-1" hello={HELLO} client={made} go={() => undefined} />);
    await act(async () => undefined);
    expect(view.get("h1").textContent).toBe("Remove, checked");
    expect(view.get(".workspace__title-row .tag").textContent).toBe("PASS");
    expect(view.get(".verdict").textContent).toContain("Check passed");
    expect(view.all(".result__attempt").map((one) => one.getAttribute("data-verdict"))).toEqual(["fail", "pass"]);
    expect(view.text()).toContain("wider mask, 8 px");
    expect(view.get(".cost-table").textContent).toContain("free");
    expect(view.all("[role=tab]").map((tab) => tab.textContent)).toEqual(["Original", "Compare", "Result", "Mask", "Attempt 1", "Attempt 2"]);
    expect(view.get("[role=tab][aria-selected=true]").textContent).toBe("Result");
    click(view.all<HTMLElement>("[role=tab]")[4]!);
    expect(view.text()).toContain("not its result");
    expectShownAsText(view);
    view.unmount();
  });

  it("sends the result to another task as a new draft, runs it again, and copies its command", async () => {
    localStorage.removeItem(DRAFTS_KEY);
    const made = client();
    const go = vi.fn();
    const view = render(<Result id="1795-1" hello={HELLO} client={made} go={go} />);
    await act(async () => undefined);
    click(view.get(".result__send .button"));
    click(view.get(".send-to .menu__item"));
    expect(go).toHaveBeenCalledWith("#/k/squint/source-fidelity");
    const drafts = readDrafts(localStorage);
    expect(drafts[0]?.sent?.run).toBe("1795-1");
    expect(drafts[0]?.values).toEqual({ source: A, candidate: B, brief: "the brief" });
    const again = view.all<HTMLElement>(".button--quiet").find((one) => one.textContent?.includes("Run again"))!;
    click(again);
    expect(go).toHaveBeenLastCalledWith("#/k/rascal/remove_checked");
    expect(readDrafts(localStorage).find((one) => one.task === "remove_checked")?.sent).toBeUndefined();
    // A draft of the same task the person has not run is replaced only when they say so.
    go.mockClear();
    click(again);
    expect(go).not.toHaveBeenCalled();
    expect(view.get(".result__replacing").textContent).toContain("Remove, checked");
    click(view.all<HTMLElement>(".result__replacing .button").find((one) => one.textContent?.includes("Keep my draft"))!);
    expect(view.all(".result__replacing")).toHaveLength(0);
    click(again);
    click(view.all<HTMLElement>(".result__replacing .button").find((one) => one.textContent?.includes("Replace it"))!);
    expect(go).toHaveBeenCalledWith("#/k/rascal/remove_checked");
    // Copy command is in the More menu (plan m7, task U1), beside the pin and Save as example.
    click(view.all<HTMLElement>(".button--quiet").find((one) => one.textContent?.includes("More"))!);
    expect(view.all("[role=menu][aria-label=More] [role=menuitem]").map((one) => one.textContent)).toEqual(["Pin as baseline", "Save as example…", "Replay…", "Bundle this run…", ">_ Copy command"]);
    const copy = view.all<HTMLElement>("[role=menuitem]").find((one) => one.textContent?.includes("Copy command"))!;
    click(copy);
    await act(async () => undefined);
    expect(made.request).toHaveBeenCalledWith("GET", "/api/runs/1795-1/command");
    expect(view.get(".result__command-text").textContent).toBe("whizbang rascal remove-checked --prompt 'the hose'");
    expect(view.text()).toContain("The config has changed since this run");
    view.unmount();
  });
});
