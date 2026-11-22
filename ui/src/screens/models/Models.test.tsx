// The Models screen: the catalogue's models and their one action each, the backends with
// whether a key is set (never a key), and the allowance with its confirmed reset.
import { describe, expect, it, vi } from "vitest";
import { act } from "preact/test-utils";
import type { Client } from "../../api";
import { HOSTILE, click, input, render } from "../../test/render";
import { isAllowance, lineOf, stateOf } from "./catalogue";
import { Models } from "./Models";

const MODELS = {
  models: [
    { name: "pocket", title: "Pocket", bytes: 5596, files: 3, state: "not_installed", why: "not installed", files_done: null, sign_in: "none" },
    { name: "sam3", title: "SAM 3", bytes: 3_400_000_000, files: 9, state: "not_installed", why: "not installed", files_done: null, sign_in: "not_named" },
    { name: "tiny", title: HOSTILE, bytes: 200_000_000, files: 4, state: "installed", why: "installed", files_done: null, sign_in: "none" },
    { name: "depth", title: "Depth", bytes: 1_400_000_000, files: 3, state: "not_installed", why: "not installed", files_done: 1, sign_in: "none", install: "installing" },
  ],
  installing: "depth",
};
const BACKENDS = {
  backends: [
    { capability: "segment", name: "local-seg", local: true, model: "tiny", access: "local" },
    { capability: "fill", name: "cloud-fill", local: false, access: "key_set", price: { kind: "at_most", usd: 0.04 } },
    { capability: "judge", name: "keyless-judge", local: false, access: "no_key", price: { kind: "about", usd: 0.002 } },
    { capability: "judge", name: "cloud-judge", local: false, access: "no_key_needed", price: { kind: "free" } },
  ],
};
const SPEND = {
  epoch: 2,
  allowance: 10,
  since_ms: Date.UTC(2026, 10, 2),
  counted: { exactly: true, sum: { usd: 2.48 } },
  charged: { usd: 2.4 },
  held: { usd: 0.08 },
  left: 7.52,
  call_cap: 1.5,
  config: "whizbang.toml",
};

function hosted() {
  const request = vi.fn((method: string, path: string, _body?: unknown) => {
    if (path === "/api/backends") return Promise.resolve(BACKENDS);
    if (path === "/api/spend") return Promise.resolve(SPEND);
    if (path === "/api/spend/reset") return Promise.resolve({ allowance: 20, epoch: 3 });
    if (method === "POST" || method === "DELETE") return Promise.resolve({});
    return Promise.reject(new Error(`unexpected ${method} ${path}`));
  });
  const poll = vi.fn((_: (after: number) => string, next: (answer: unknown) => unknown, _onError?: unknown, _options?: unknown) => {
    void Promise.resolve().then(() => next(MODELS));
    return () => undefined;
  });
  return { client: { request, poll } as unknown as Client, request, poll };
}

async function settle() {
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 0));
  });
}

describe("the models screen", () => {
  it("shows each model with its one action and each backend with whether its key is set", async () => {
    const { client } = hosted();
    const view = render(<Models client={client} />);
    await settle();
    const rows = view.all(".models__panel").map((panel) => Array.from(panel.querySelectorAll(".model-row")).map((row) => row.textContent));
    expect(rows[0]).toHaveLength(4);
    expect(rows[0]![0]).toContain("Get");
    expect(rows[0]![1]).toContain("NOT SET UP");
    expect(rows[0]![1]).toContain("needs a sign-in the config does not name");
    expect(rows[0]![1]).toContain("How to add");
    expect(rows[0]![2]).toContain("READY");
    expect(rows[0]![2]).toContain("used by segment · local-seg");
    expect(rows[0]![3]).toContain("1 of 3 files");
    expect(rows[0]![3]).toContain("counted in files, not bytes");
    expect(rows[0]![3]).toContain("Cancel");
    // The cloud: only the backends that send somewhere, with whether a key is set.
    expect(rows[1]).toHaveLength(3);
    expect(rows[1]![0]).toContain("KEY SET");
    expect(rows[1]![0]).toContain("at most $0.040 a request");
    expect(rows[1]![1]).toContain("NO KEY");
    expect(rows[1]![2]).toContain("READY");
    expect(view.text()).toContain(HOSTILE);
    expect(view.root.querySelector("img[onerror]")).toBeNull();
    // How to add: the config and the env file, by hand; nothing to type a key into.
    click(view.all<HTMLButtonElement>(".model-row button").find((b) => b.textContent === "How to add")!);
    expect(view.text()).toContain("key_env");
    expect(view.text()).toContain("env file");
    expect(view.all("input")).toHaveLength(0);
    view.unmount();
  });

  it("gets, cancels and removes through the host", async () => {
    const { client, request, poll } = hosted();
    const view = render(<Models client={client} />);
    await settle();
    // An install's progress is asked every 250 ms, with no back-off.
    expect(poll.mock.calls[0]?.[3]).toEqual({ backoff: false });
    const button = (words: string) => view.all<HTMLButtonElement>(".model-row button").find((b) => b.textContent === words)!;
    click(button("Get"));
    click(button("Cancel"));
    click(button("Remove"));
    await settle();
    // Remove asks first, and only the confirmation sends it.
    expect(request.mock.calls.some(([method]) => method === "DELETE")).toBe(false);
    expect(view.get(".models__confirm").textContent).toContain("Remove tiny?");
    click(view.get(".models__confirm .button--danger"));
    await settle();
    const asked = request.mock.calls.map(([method, path]) => `${method} ${path}`);
    expect(asked).toContain("POST /api/models/pocket/get");
    expect(asked).toContain("POST /api/models/depth/cancel");
    expect(asked).toContain("DELETE /api/models/tiny");
    view.unmount();
  });

  it("shows the allowance since the last reset and the cap of one call, read-only", async () => {
    const { client } = hosted();
    const view = render(<Models client={client} />);
    await settle();
    expect(view.text()).toContain("$2.48 spent since 2026-11-02");
    expect(view.text()).toContain("$7.52 left of $10.00");
    expect(view.get(".models__chip").textContent).toBe("Per run$1.50");
    expect(view.get(".models__chip").getAttribute("title")).toContain("whizbang.toml");
    expect(view.text()).not.toContain("this month");
    view.unmount();
  });

  it("resets only once the amount and the word reset are typed, and says the books closed", async () => {
    const { client, request } = hosted();
    const view = render(<Models client={client} />);
    await settle();
    click(view.all<HTMLButtonElement>("button").find((b) => b.textContent === "Reset…")!);
    expect(view.text()).toContain("A reset closes the books");
    expect(view.text()).toContain("Runs already admitted keep what they reserved");
    const reset = () => view.get<HTMLButtonElement>(".models__reset .button--danger");
    expect(reset().disabled).toBe(true);
    input(view.get<HTMLInputElement>("#reset-amount"), "20");
    expect(reset().disabled).toBe(true);
    expect(view.text()).toContain("Type the word reset to confirm.");
    input(view.get<HTMLInputElement>("#reset-word"), "Reset");
    expect(reset().disabled).toBe(true);
    input(view.get<HTMLInputElement>("#reset-word"), "reset");
    input(view.get<HTMLInputElement>("#reset-amount"), "12,50");
    expect(reset().disabled).toBe(true);
    input(view.get<HTMLInputElement>("#reset-amount"), "20");
    expect(reset().disabled).toBe(false);
    expect(request.mock.calls.some(([, path]) => path === "/api/spend/reset")).toBe(false);
    click(reset());
    await settle();
    expect(request).toHaveBeenCalledWith("POST", "/api/spend/reset", { allowance: "20", confirm: "reset" });
    expect(view.text()).toContain("The books are closed: the allowance is $20.00 from now on.");
    expect(view.all(".models__reset")).toHaveLength(0);
    view.unmount();
  });

  it("offers Cancel only for this host's own install, not one another whizbang runs", async () => {
    const { client } = hosted();
    const theirs = { ...MODELS, models: [{ name: "pocket", title: "Pocket", bytes: 5596, files: 3, state: "not_installed", files_done: 1, sign_in: "none" }] };
    vi.mocked(client.poll).mockImplementation((_: unknown, next: (answer: unknown) => unknown) => {
      void Promise.resolve().then(() => next(theirs));
      return () => undefined;
    });
    const view = render(<Models client={client} />);
    await settle();
    const row = view.get(".models__panel .model-row");
    expect(row.textContent).toContain("1 of 3 files");
    expect(row.textContent).toContain("being installed by another whizbang");
    expect(row.querySelectorAll("button")).toHaveLength(0);
    view.unmount();
  });

  it("reads each model's state and line, and what an allowance is", () => {
    expect(stateOf({ name: "a", state: "out_of_date", sign_in: "none" })).toEqual({ kind: "get" });
    expect(stateOf({ name: "a", state: "damaged" })).toEqual({ kind: "damaged" });
    expect(stateOf({ name: "a", state: "not_for_this_platform", why: "it does not run here" })).toEqual({ kind: "not-set-up" });
    expect(lineOf({ name: "a", state: "not_for_this_platform", why: "it does not run here" }, [])).toBe("it does not run here");
    expect(stateOf({ name: "a", state: "not_installed", sign_in: "not_here" })).toEqual({ kind: "not-set-up" });
    expect(lineOf({ name: "a", state: "not_installed", sign_in: "not_here" }, [])).toContain("does not read");
    expect(lineOf({ name: "a", title: "A", state: "not_installed", last: { ended: "cancelled", why: "the install was cancelled" } }, [])).toBe("A · the install was cancelled");
    expect(["20", "12.50", "0.000001"].every(isAllowance)).toBe(true);
    expect(["", "-1", "12,50", "1.0000001", "1e3", " 20"].some(isAllowance)).toBe(false);
  });
});
