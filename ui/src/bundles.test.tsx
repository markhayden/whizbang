// Bundles, guided chain replay and the replay of a run (plan m7, task U2): the pure helpers,
// the routes, a chain's place in the browser's storage, the replay dialog saying the
// engine's refusal, and words from a bundle shown as text.
import { beforeEach, describe, expect, it, vi } from "vitest";
import { act } from "preact/test-utils";
import { ApiError, type Client } from "./api";
import { chainOf, chainOfRun, draftOfStep, linksOf, readChains, reportTitle, saveChain, sizeWords, stepStates, titlesOf, withRun, type OpenedStep, type ReplayReport as Report } from "./bundles";
import { ChainCard } from "./components/ChainCard";
import { ChainSteps } from "./components/ChainSteps";
import { ReplayDialog } from "./components/ReplayDialog";
import { ReplayReport } from "./components/ReplayReport";
import { localStore } from "./drafts";
import { hrefOf, parseRoute } from "./routes";
import { HOSTILE, click, expectShownAsText, render } from "./test/render";

const DIGEST = `sha256:${"ab".repeat(32)}`;
const PICTURE = `sha256:${"cd".repeat(32)}`;
const OTHER = `sha256:${"ef".repeat(32)}`;

beforeEach(() => localStorage.clear());

async function settle() {
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 0));
  });
}

function report(over: Partial<Report>): Report {
  return { replay: "1795-9", of: "1795-1", bundle: null, mode: "offline", report: "reproduced", differences: [], more: 0, stopped: null, settings: [], spent_micros: 0, ...over };
}

describe("bundles and chains", () => {
  it("are addressed by a bundle's digest", () => {
    expect(parseRoute("#/bundles")).toEqual({ name: "bundles" });
    expect(parseRoute(`#/bundles/${DIGEST}`)).toEqual({ name: "bundles", digest: DIGEST });
    expect(parseRoute(`#/chain/${DIGEST}`)).toEqual({ name: "chain", digest: DIGEST });
    expect(parseRoute("#/bundles/..%2Fetc").name).toBe("unknown");
    expect(parseRoute("#/chain/sha256:12").name).toBe("unknown");
    expect(hrefOf({ name: "chain", digest: DIGEST })).toBe(`#/chain/${DIGEST}`);
  });

  it("say a size about before it is written and exactly after", () => {
    expect(sizeWords(3_100_000, true)).toBe("about 3.1 MB");
    expect(sizeWords(512)).toBe("512 bytes");
    expect(sizeWords(2_048)).toBe("2.0 KB");
  });

  it("name each link of a chain", () => {
    const steps = [
      { step: 0, run: "1-1", action: "remove_prompt", version: "1", runs: 3, replayable: true, why: null, inputs: [{ port: "image", place: 0, artifact: PICTURE, from: null }] },
      { step: 1, run: "1-2", action: "source-fidelity", version: "1", runs: 1, replayable: true, why: null, inputs: [{ port: "candidate", place: 0, artifact: null, from: { step: 0, port: "image", place: 0 } }] },
    ];
    expect(linksOf({ steps })).toEqual(["1's image → 2's candidate"]);
  });

  it("keep a chain's place, its runs by step, and find the chain of a run", () => {
    const store = localStore();
    const place = { digest: DIGEST, steps: 3, runs: [], stopped: false, at: 1 };
    saveChain(store, withRun(place, 0, "1795-4"));
    expect(chainOf(store, DIGEST)?.runs).toEqual(["1795-4"]);
    saveChain(store, withRun(chainOf(store, DIGEST)!, 1, "1795-5"));
    expect(chainOfRun(store, "1795-5")?.step).toBe(1);
    // A step run again forgets the later steps' runs.
    expect(withRun(chainOf(store, DIGEST)!, 0, "1795-6").runs).toEqual(["1795-6"]);
    expect(stepStates({ ...chainOf(store, DIGEST)!, stopped: false }, null)).toEqual(["ended", "ended", "review"]);
    expect(stepStates({ ...place, runs: ["1795-4"], stopped: true }, null)).toEqual(["ended", "stopped", "stopped"]);
    expect(stepStates({ ...place, runs: ["1795-4"] }, 1)).toEqual(["ended", "blocked", "waiting"]);
    localStorage.setItem("whizbang.chains", "not json");
    expect(readChains(store)).toEqual([]);
  });

  it("open a step as a draft that names the step and its bound links", () => {
    const opened: OpenedStep = {
      task: "source-fidelity",
      kit: "squint",
      values: { source: PICTURE, candidate: OTHER },
      step: { index: 1, of: 2, run: "1-2", action: "source-fidelity" },
      bindings: [{ port: "candidate", place: 0, from: { step: 0, port: "image", place: 0 }, run: "1795-4", now: OTHER, recorded: PICTURE, differs: true }],
      stopped: null,
    };
    const draft = draftOfStep(opened, DIGEST, 5);
    expect(draft.chain).toEqual({ digest: DIGEST, step: 1, of: 2, bindings: [{ port: "candidate", from: { step: 0, port: "image" }, run: "1795-4", now: OTHER, recorded: PICTURE, differs: true }] });
    expect(draft.pictures[OTHER]?.name).toContain("step 1's image");
    expect(draft.pictures[PICTURE]?.name).toContain("from the bundle");
    // Nothing beyond the fields when the host names nothing more.
    expect(draft.jobOf).toBeUndefined();
    expect(draft.kept).toBeUndefined();
  });

  it("open a step whose call holds more than its form shows with that kept for the call", () => {
    // N1's pass, finding 4: the step's check takes its job from the bundle's step, and a
    // recipe's `until` that no field shows is sent as it was.
    const opened: OpenedStep = {
      task: "source-fidelity",
      kit: "squint",
      values: { source: PICTURE, candidate: OTHER, brief: "the words" },
      step: { index: 1, of: 2, run: "1-2", action: "source-fidelity" },
      bindings: [],
      stopped: null,
      job_of: { bundle: DIGEST, step: 1 },
      kept: { until: "select" },
    };
    const draft = draftOfStep(opened, DIGEST, 5);
    expect(draft.jobOf).toEqual({ bundle: DIGEST, step: 1 });
    expect(draft.kept).toEqual({ until: "select" });
  });

  it("name each step on a step's card by its task's title, while the bundle is still being read", async () => {
    // N1's pass, finding 3: "Step 1 · step 1 ended", "Its outputs go to step 2, step 2."
    const reading = { request: vi.fn(() => new Promise(() => undefined)) } as unknown as Client;
    const titled = { digest: DIGEST, steps: 2, runs: ["1795-4"], stopped: false, at: 1, titles: ["Remove what you name", "Check an edit"] };
    let view = render(<ChainCard client={reading} place={titled} step={0} run="1795-4" outcome="completed" title="Remove what you name" go={vi.fn()} />);
    await settle();
    expect(view.get(".chain-card__title").textContent).toBe("Step 1 · Remove what you name ended");
    expect(view.get(".chain-card__line").textContent).toContain("Its outputs go to step 2, Check an edit.");
    view.unmount();
    // A place kept before titles were: this step's title from its result, the next by number once.
    const { titles: _, ...untitled } = titled;
    view = render(<ChainCard client={reading} place={untitled} step={0} run="1795-4" outcome="completed" title="Remove what you name" go={vi.fn()} />);
    await settle();
    expect(view.get(".chain-card__title").textContent).toBe("Step 1 · Remove what you name ended");
    expect(view.get(".chain-card__line").textContent).toContain("Its outputs go to step 2.");
    view.unmount();
    // Chain done lists each step by its title.
    view = render(<ChainCard client={reading} place={{ ...titled, runs: ["1795-4", "1795-5"] }} step={1} run="1795-5" outcome="completed" title="Check an edit" go={vi.fn()} />);
    await settle();
    expect(view.all(".chain-step__name").map((one) => one.textContent)).toEqual(["1 · Remove what you name", "2 · Check an edit"]);
    view.unmount();
  });

  it("take the titles of a chain's steps into the draft of a step, for its place", () => {
    const opened: OpenedStep = { task: "remove_prompt", kit: "rascal", values: {}, step: { index: 0, of: 2, run: "1-1", action: "remove_prompt" }, bindings: [], stopped: null };
    const titles = titlesOf({ steps: [{ step: 0, run: "1-1", action: "remove_prompt", title: "Remove what you name", version: null, runs: 1, replayable: true, why: null, inputs: [] }, { step: 1, run: "1-2", action: "source-fidelity", title: null, version: null, runs: 1, replayable: true, why: null, inputs: [] }] });
    expect(titles).toEqual(["Remove what you name", "source-fidelity"]);
    expect(draftOfStep(opened, DIGEST, 5, titles).chain.titles).toEqual(titles);
    // A place's titles are read back only as the page writes them: words and nulls.
    const place = { digest: DIGEST, steps: 2, runs: [], stopped: false, at: 1 };
    localStorage.setItem("whizbang.chains", JSON.stringify([{ ...place, titles: ["Remove what you name", null] }, { ...place, digest: OTHER, titles: [3, { x: 1 }] }, { ...place, digest: PICTURE, titles: "Remove" }]));
    const read = readChains(localStore());
    expect(read[0]?.titles).toEqual(["Remove what you name", null]);
    expect(read[1]).toEqual({ ...place, digest: OTHER });
    expect(read[2]).toEqual({ ...place, digest: PICTURE });
  });
});

describe("a replay's report", () => {
  it("says each outcome as the design does", () => {
    expect(reportTitle(report({}))).toBe("Reproduced");
    expect(reportTitle(report({ report: "differs", differences: [{ place: "the run", what: "outputs", original: "a", replay: "b" }], more: 1 }))).toBe("Differs in 2 places");
    expect(reportTitle(report({ report: "diverged", stopped: { kind: "diverged", capability: "judge", request: 2, why: "x" } }))).toBe("Diverged at judge request 2");
    expect(reportTitle(report({ report: "unanswerable" }))).toBe("Can't be answered offline");
  });

  it("shows a difference holding markup as text", () => {
    const view = render(<ReplayReport report={report({ report: "differs", differences: [{ place: "the run", what: HOSTILE, original: "a", replay: "b" }] })} />);
    expectShownAsText(view);
    view.unmount();
  });

  it("shows a chain's steps and words from a bundle as text", () => {
    const steps = [{ step: 0, run: "1-1", action: HOSTILE, version: "1", runs: 1, replayable: false, why: HOSTILE, inputs: [] }];
    const view = render(<ChainSteps steps={steps} states={["review"]} runs={[null]} />);
    expectShownAsText(view);
    expect(view.get(".chain-step").getAttribute("data-state")).toBe("review");
    view.unmount();
  });
});

describe("the replay dialog", () => {
  it("is always offered, and says the engine's refusal in its words", async () => {
    const request = vi.fn(async (method: string, path: string) => {
      if (method === "POST" && path === "/api/runs/1795-1/replay") throw new ApiError(422, "the run 1795-1 was not replayed: it was interrupted");
      throw new Error(`unexpected ${method} ${path}`);
    });
    const client = { request, prepare: vi.fn() } as unknown as Client;
    const started = vi.fn();
    const view = render(<ReplayDialog client={client} run="1795-1" title="Remove what you name" onStarted={started} onCancel={() => undefined} />);
    const replay = view.all<HTMLElement>(".button--primary").find((one) => one.textContent?.includes("Replay offline"))!;
    click(replay);
    await settle();
    expect(request).toHaveBeenCalledWith("POST", "/api/runs/1795-1/replay", expect.objectContaining({ live: false }));
    expect(view.get("[role=alert]").textContent).toContain("it was interrupted");
    expect(started).not.toHaveBeenCalled();
    view.unmount();
  });

  it("reviews a live replay's cost before it starts", async () => {
    const prepare = vi.fn(async () => ({ ready: true, handle: "h-1", bound: 0.46, needs: [{ capability: "fill", backend: "cloud-fill", model: "fill", requests: 1, each: 0.04, bound: 0.04 }] }));
    const request = vi.fn(async () => ({ run: "1795-9" }));
    const client = { request, prepare } as unknown as Client;
    const started = vi.fn();
    const view = render(<ReplayDialog client={client} run="1795-1" title="Remove" again={{ task: "remove_prompt", values: { prompt: "the hose" } }} onStarted={started} onCancel={() => undefined} />);
    const live = view.all<HTMLInputElement>("input[type=radio]")[1]!;
    act(() => {
      live.checked = true;
      live.dispatchEvent(new Event("change", { bubbles: true }));
    });
    await settle();
    expect(prepare).toHaveBeenCalledWith("remove_prompt", { prompt: "the hose" });
    expect(view.text()).toContain("Live, with the cost reviewed");
    const go = view.all<HTMLElement>(".button--primary").find((one) => one.textContent?.includes("Replay live"))!;
    expect(go.textContent).toContain("$0.460");
    click(go);
    await settle();
    expect(request).toHaveBeenCalledWith("POST", "/api/runs/1795-1/replay", expect.objectContaining({ live: true, handle: "h-1" }));
    expect(started).toHaveBeenCalledWith("1795-9");
    view.unmount();
  });

  it("never starts a live replay whose cost was refused or is not ready", async () => {
    for (const answer of [{ ready: false, refused: true, why: "the call is refused", bound: 0 }, { ready: false, bound: 0.04, needs: [] }]) {
      const prepare = vi.fn(async () => answer);
      const request = vi.fn();
      const client = { request, prepare } as unknown as Client;
      const view = render(<ReplayDialog client={client} run="1795-1" title="Remove" again={{ task: "remove_prompt", values: {} }} onStarted={() => undefined} onCancel={() => undefined} />);
      const live = view.all<HTMLInputElement>("input[type=radio]")[1]!;
      act(() => {
        live.checked = true;
        live.dispatchEvent(new Event("change", { bubbles: true }));
      });
      await settle();
      const go = view.all<HTMLButtonElement>(".button--primary").find((one) => one.textContent?.includes("Replay live"))!;
      expect(go.disabled).toBe(true);
      expect(view.text()).toContain("a live replay cannot start");
      expect(request).not.toHaveBeenCalled();
      view.unmount();
    }
  });
});
