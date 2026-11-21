// The shell: the screen of each route, an empty state for each, the refusal said, the keys.
import { describe, expect, it, vi } from "vitest";
import { act } from "preact/test-utils";
import { ApiError, type Client, type Hello } from "./api";
import { App, NO_TOKEN } from "./app";
import { HOSTILE, render } from "./test/render";

function clientAnswering(hello: Promise<Hello>): Client {
  return {
    hello: () => hello,
    request: vi.fn(),
    upload: vi.fn(),
    start: vi.fn(),
    poll: vi.fn(),
  } as unknown as Client;
}

async function settle() {
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 0));
  });
}

function mount(hash: string, hello: Promise<Hello>, hasToken = true) {
  const listeners: Array<(hash: string) => void> = [];
  const go = vi.fn((next: string) => listeners.forEach((listener) => listener(next)));
  const view = render(<App client={clientAnswering(hello)} hasToken={hasToken} initialHash={hash} subscribeHash={(l) => (listeners.push(l), () => undefined)} go={go} />);
  return { view, go };
}

const HELLO: Hello = {
  version: "0.1.0",
  store: "/data/store",
  kits: [
    { id: "rascal", name: "Rascal", line: "edit pictures", tasks: [{ name: "remove_prompt", title: "Remove what you name", shape: "image + words → image" }] },
    { id: "squint", name: "Squint", line: "check pictures", tasks: [{ name: "check", title: HOSTILE }] },
  ],
  backends: [{ name: "SAM3", ready: true }],
  allowance: { allowance: 10, left: 7.61 },
};

describe("the shell", () => {
  it("shows Home with the kits of hello, the status bar and the allowance", async () => {
    const { view } = mount("#/", Promise.resolve(HELLO));
    await settle();
    expect(view.get("h1").textContent).toContain("whizbang");
    expect(view.all(".kit-panel h2").map((h) => h.textContent)).toEqual(["Rascal", "Squint"]);
    expect(view.text()).toContain("store /data/store");
    expect(view.text()).toContain("whizbang 0.1.0");
    expect(view.get(".top-bar__allowance").textContent).toBe("$7.61 / 10.00");
    expect(view.root.querySelector("img[onerror]")).toBeNull();
    expect(view.text()).toContain(HOSTILE);
  });

  it("has an empty state for Home, a kit, a run, History, Models and an unknown address", async () => {
    const empty = mount("#/", Promise.resolve({ version: "0.1.0" }));
    await settle();
    expect(empty.view.text()).toContain("This build lists no kits yet.");
    expect(empty.view.text()).toContain("Nothing is running and there are no drafts.");
    expect(empty.view.text()).toContain("No pictures yet.");
    for (const [hash, words] of [
      ["#/k/rascal/remove_prompt", "cannot be run from the playground in this build yet"],
      ["#/run/r-1", "cannot show a run"],
      ["#/history", "No runs yet."],
      ["#/models", "does not list models"],
      ["#/nowhere", "no page at this address"],
    ]) {
      const { view } = mount(hash!, Promise.resolve({ version: "0.1.0" }));
      await settle();
      expect(view.text(), hash).toContain(words);
      view.unmount();
    }
  });

  it("says to run whizbang play again when the host refuses the token", async () => {
    const { view } = mount("#/", Promise.reject(new ApiError(401, "Run whizbang play again, which opens the page with a new one.")));
    await settle();
    expect(view.get("[role=alert]").textContent).toContain("Run whizbang play again");
    expect(view.text()).toContain("whizbang play again");
  });

  it("says so when the page holds no token at all", async () => {
    const { view } = mount("#/", Promise.reject(new ApiError(401, "x")), false);
    expect(view.get("[role=alert]").textContent).toBe(NO_TOKEN);
  });

  it("follows the address, and the keys open search, Home and a kit", async () => {
    const { view, go } = mount("#/", Promise.resolve(HELLO));
    await settle();
    act(() => go("#/k/rascal/remove_prompt"));
    expect(view.get(".breadcrumb").textContent).toContain("Rascal");
    const press = (key: string) =>
      act(() => {
        window.dispatchEvent(new KeyboardEvent("keydown", { key, metaKey: true, bubbles: true }));
      });
    press("2");
    expect(go).toHaveBeenLastCalledWith("#/k/squint");
    press("0");
    expect(go).toHaveBeenLastCalledWith("#/");
    press("k");
    expect(view.all("[role=dialog]")).toHaveLength(1);
    expect(view.all("[role=option]").length).toBe(2);
  });
});
