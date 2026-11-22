// The shell: the screen of each route, an empty state for each, the refusal said, the keys.
import { describe, expect, it, vi } from "vitest";
import { act } from "preact/test-utils";
import { ApiError, type Client, type Hello } from "./api";
import { App, NO_TOKEN } from "./app";
import type { Taken } from "./token";
import { HOSTILE, render } from "./test/render";

let lastClient: Client;
function clientAnswering(hello: Promise<Hello>): Client {
  lastClient = {
    hello: vi.fn(() => hello),
    setToken: vi.fn(),
    request: vi.fn(),
    upload: vi.fn(),
    start: vi.fn(),
    poll: vi.fn(),
    prepare: vi.fn(() => new Promise(() => undefined)),
    pollRun: vi.fn(() => () => undefined),
    cancelRun: vi.fn(),
    openRuns: vi.fn(() => Promise.resolve([])),
    uploadPicture: vi.fn(),
    listPictures: vi.fn(() => Promise.resolve([])),
    commitLocation: vi.fn(),
    blobUrl: vi.fn(() => new Promise(() => undefined)),
  } as unknown as Client;
  return lastClient;
}

async function settle() {
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 0));
  });
}

function mount(hash: string, hello: Promise<Hello>, hasToken = true, takeToken: () => Taken = () => ({ token: null, offered: false, fresh: false })) {
  const listeners: Array<(hash: string) => void> = [];
  const go = vi.fn((next: string) => listeners.forEach((listener) => listener(next)));
  const view = render(
    <App client={clientAnswering(hello)} hasToken={hasToken} initialHash={hash} subscribeHash={(l) => (listeners.push(l), () => undefined)} go={go} takeToken={takeToken} />,
  );
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
    expect(view.get("h1 img").getAttribute("alt")).toBe("whizbang");
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
      ["#/k/rascal/remove_prompt", "This build has no task of that name."],
      ["#/k/rascal", "This build has no kit of that name."],
      ["#/run/r-1", "Waiting for the first step."],
      ["#/history", "No runs yet."],
      ["#/models", "Looking at the models directory"],
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

  it("acts on nothing in the address: any route asks hello and changes nothing", async () => {
    // The host lets a top-level navigation (a GET outside /api/ that Chrome marks as a
    // navigation to a document) reach the page without the token (H1, and the fixture
    // host's guard), so another page can open it at any address it makes. Such an address
    // must start, stop or upload nothing: a route only chooses a screen, and only #k= is
    // acted on, by token.ts.
    for (const hash of ["#/", "#/k/rascal/remove_prompt?run=1", "#/run/r-1", "#/history?kit=rascal&result=pass", "#/models", "#/quit", "#/k/rascal/remove_prompt/start"]) {
      const { view } = mount(hash, Promise.resolve(HELLO));
      await settle();
      expect(lastClient.hello, hash).toHaveBeenCalledTimes(1);
      // Screens read (the runs still going, the pictures, a run's state, the models):
      // nothing else, and no request but a read.
      const sent = vi.mocked(lastClient.request).mock.calls.filter(([method]) => method !== "GET");
      expect(sent, hash).toEqual([]);
      expect(lastClient.upload, hash).not.toHaveBeenCalled();
      expect(lastClient.uploadPicture, hash).not.toHaveBeenCalled();
      expect(lastClient.start, hash).not.toHaveBeenCalled();
      expect(lastClient.cancelRun, hash).not.toHaveBeenCalled();
      expect(lastClient.commitLocation, hash).not.toHaveBeenCalled();
      view.unmount();
    }
  });

  it("takes a #k= that a hash change brings, uses its token, and asks hello again", async () => {
    const fresh = "cd".repeat(32);
    let next: Taken = { token: null, offered: false, fresh: false };
    const { view, go } = mount("#/", Promise.reject(new ApiError(401, "Run whizbang play again.")), false, () => next);
    await settle();
    expect(view.get("[role=alert]").textContent).toContain("whizbang play again");
    (lastClient.hello as ReturnType<typeof vi.fn>).mockImplementation(() => Promise.resolve(HELLO));
    next = { token: fresh, offered: true, fresh: true };
    act(() => go(`#k=${fresh}`));
    await settle();
    expect(lastClient.setToken).toHaveBeenCalledWith(fresh);
    expect(lastClient.hello).toHaveBeenCalledTimes(2);
    expect(view.all("[role=alert]")).toHaveLength(0);
    expect(view.text()).toContain("store /data/store");
    // A #k= that is not a token is taken out of the bar and changes nothing else.
    next = { token: fresh, offered: true, fresh: false };
    act(() => go("#k=nonsense&x"));
    await settle();
    expect(lastClient.setToken).toHaveBeenCalledTimes(1);
    expect(view.get("h1 img").getAttribute("alt")).toBe("whizbang");
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
    // Each task, and each task's record (plan m5, task W3, item 7).
    expect(view.all("[role=option]").length).toBe(4);
  });
});
