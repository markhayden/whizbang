// History, a report and a record (plan m5, task W3): rows and figures from the API, the
// filter kept in the address, and each document shown in the isolated frame.
import { describe, expect, it, vi } from "vitest";
import type { Client, Hello } from "../api";
import { HOSTILE, render } from "../test/render";
import { History } from "./history/History";
import { Report } from "./report/Report";

const HELLO: Hello = { version: "0.1.0", kits: [{ id: "rascal", name: "Rascal" }, { id: "squint", name: "Squint" }] };

async function settle() {
  for (let at = 0; at < 5; at += 1) await new Promise((resolve) => setTimeout(resolve, 0));
}

function answering(answers: Record<string, unknown>, bytes?: (path: string) => Promise<Blob>): Client {
  return {
    request: vi.fn((_: string, path: string) => Promise.resolve(answers[path.split("?")[0]!])),
    blobUrl: vi.fn(() => new Promise(() => undefined)),
    bytes: vi.fn(bytes ?? (() => Promise.reject(new Error("no bytes")))),
  } as unknown as Client;
}

describe("history", () => {
  it("lists each run with what was asked, as text, and the figures of runs stats", async () => {
    const client = answering({
      "/api/runs": { runs: [{ run: "r-1", action: "remove_prompt", kit: "rascal", title: "Remove what you name", words: [HOSTILE], pictures: ["porch.png"], result: "done", models: ["fill"], time_ms: 1500, cost: 0.04, at_ms: Date.now() }], more: false },
      "/api/stats": { runs: 3, verdicts: { pass: 1, fail: 0, unsure: 0 }, cache_hits: 1, settled_micros: 40000 },
    });
    const go = vi.fn();
    const view = render(<History hello={HELLO} client={client} query={new URLSearchParams("kit=rascal")} go={go} />);
    await settle();
    expect(view.text()).toContain("Remove what you name");
    expect(view.text()).toContain(`${HOSTILE} · porch.png`);
    expect(view.root.querySelector("img[onerror]")).toBeNull();
    expect(view.text()).toContain("DONE");
    expect(view.get(".history__figures").textContent).toContain("cache hits1");
    const asked = (client.request as ReturnType<typeof vi.fn>).mock.calls.map((call) => call[1] as string);
    expect(asked).toContain("/api/runs?kit=rascal&limit=50");
    expect(asked).toContain("/api/stats?kit=rascal");
    view.get<HTMLButtonElement>(".history-row").click();
    expect(go).toHaveBeenCalledWith("#/run/r-1");
    view.unmount();
  });

  it("says what Bundle chain saved, by its file name, once the chain is written", async () => {
    // N1's pass, finding 5: the panel closed with no word of what was saved.
    const row = (run: string) => ({ run, action: "remove_prompt", kit: "rascal", title: "Remove what you name", words: [], pictures: [], result: "done", models: [], time_ms: 1, cost: 0, at_ms: Date.now() });
    const client = answering({
      "/api/runs": { runs: [row("r-1"), row("r-2")], more: false },
      "/api/stats": { runs: 2, verdicts: { pass: 0, fail: 0, unsure: 0 }, cache_hits: 0, settled_micros: 0 },
      "/api/bundles/chain": { digest: "", steps: [], holds: { records: 2, files: 2 }, withheld: [] },
    });
    const postBytes = vi.fn(() => Promise.resolve(new Blob([new Uint8Array(171_520)])));
    Object.assign(client, { postBytes });
    const created = vi.spyOn(URL, "createObjectURL").mockReturnValue("blob:saved");
    const view = render(<History hello={HELLO} client={client} query={new URLSearchParams()} go={vi.fn()} />);
    await settle();
    const button = (words: string) => view.all<HTMLButtonElement>("button").find((one) => one.textContent?.includes(words))!;
    button("Make a chain").click();
    await settle();
    view.get<HTMLButtonElement>('[aria-label="Pick run r-1 for the chain"]').click();
    await settle();
    view.get<HTMLButtonElement>('[aria-label="Pick run r-2 for the chain"]').click();
    await settle();
    button("Bundle chain").click();
    await settle();
    expect(postBytes).toHaveBeenCalledWith("/api/bundles/chain", { runs: ["r-1", "r-2"] });
    expect(view.root.querySelector(".history__chain")).toBeNull();
    expect(view.get('[role="status"].history__saved').textContent).toBe("Saved chain-r-1.whizbang-bundle: a chain of 2 steps, 171.5 KB. Take it into another playground on its Bundles page.");
    created.mockRestore();
    view.unmount();
  });
});

describe("a report", () => {
  it("is shown in the isolated frame, and its words never in the page", async () => {
    const html = `<!doctype html><meta name="task" content="remove_prompt"><p>${HOSTILE}</p>`;
    const client = answering({}, () => Promise.resolve(new Blob([html], { type: "text/html" })));
    const view = render(<Report id="r-1" client={client} />);
    await settle();
    const frame = view.get<HTMLIFrameElement>("iframe.isolated-frame");
    expect(frame.getAttribute("sandbox")).toBe("");
    expect(view.root.querySelector("img[onerror]")).toBeNull();
    expect(client.bytes).toHaveBeenCalledWith("/api/runs/r-1/report");
    view.unmount();
  });
});
