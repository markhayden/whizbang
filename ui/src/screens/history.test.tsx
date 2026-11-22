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
