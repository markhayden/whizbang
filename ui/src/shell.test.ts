// The shell's pieces that are not components: the token, the routes, the keys, the client.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ApiError, createClient, Limiter, newRequestId, POLL_MAX_MS, POLL_MS, retryPause, sentenceFor, START_TRIES, UPLOADS_AT_ONCE } from "./api";
import { actionOf } from "./keyboard";
import { hrefOf, parseRoute } from "./routes";
import { takeToken, TOKEN_KEY } from "./token";

const TOKEN = "ab".repeat(32);

function keeper(initial: Record<string, string> = {}) {
  const items = new Map(Object.entries(initial));
  return { getItem: (k: string) => items.get(k) ?? null, setItem: (k: string, v: string) => void items.set(k, v), items };
}

describe("the token", () => {
  const at = (hash: string) => ({ hash, pathname: "/", search: "" });
  it("is read from the address, kept, and taken out of the address", () => {
    const kept = keeper();
    const replace = vi.fn();
    expect(takeToken(at(`#k=${TOKEN}`), kept, replace)).toEqual({ token: TOKEN, offered: true, fresh: true });
    expect(kept.items.get(TOKEN_KEY)).toBe(TOKEN);
    expect(replace).toHaveBeenCalledWith("/#/");
  });
  it("reads the address before it replaces it, as the live location changes under it", () => {
    const live = { hash: `#k=${TOKEN}`, pathname: "/", search: "" };
    const taken = takeToken(live, keeper(), (url) => {
      live.hash = url.slice(url.indexOf("#"));
    });
    expect(taken).toEqual({ token: TOKEN, offered: true, fresh: true });
    expect(live.hash).toBe("#/");
  });
  it("takes a token in capitals, and keeps it as the host writes it", () => {
    const kept = keeper();
    expect(takeToken(at(`#k=${TOKEN.toUpperCase()}`), kept, () => undefined)).toEqual({ token: TOKEN, offered: true, fresh: true });
    expect(kept.items.get(TOKEN_KEY)).toBe(TOKEN);
  });
  it("is the one kept before when the address holds none, and nothing is replaced", () => {
    const replace = vi.fn();
    expect(takeToken(at("#/history"), keeper({ [TOKEN_KEY]: TOKEN }), replace)).toEqual({ token: TOKEN, offered: false, fresh: false });
    expect(replace).not.toHaveBeenCalled();
  });
  it("takes out of the bar any hash that begins with #k=, valid or not, and keeps nothing of it", () => {
    for (const hash of ["#k=short", `#k=${TOKEN}&x=1`, `#K=${TOKEN}`, "#k=", `#k=${TOKEN}#/run/r-1`]) {
      const kept = keeper({ [TOKEN_KEY]: "kept" });
      const replace = vi.fn();
      const taken = takeToken(at(hash), kept, replace);
      expect(replace, hash).toHaveBeenCalledWith("/#/");
      expect(kept.items.get(TOKEN_KEY), hash).toBe("kept");
      expect(taken, hash).toEqual({ token: "kept", offered: true, fresh: false });
    }
  });
  it("is null when there is none", () => {
    expect(takeToken(at(""), keeper(), () => undefined).token).toBeNull();
  });
  it("lives for the page when storage refuses it", () => {
    const refusing = {
      getItem: () => {
        throw new Error("denied");
      },
      setItem: () => {
        throw new Error("denied");
      },
    };
    expect(takeToken(at(`#k=${TOKEN}`), refusing, () => undefined).token).toBe(TOKEN);
    expect(takeToken(at(""), refusing, () => undefined).token).toBeNull();
  });
});

describe("the routes", () => {
  it("reads each route of the hash", () => {
    expect(parseRoute("")).toEqual({ name: "home" });
    expect(parseRoute("#/")).toEqual({ name: "home" });
    expect(parseRoute("#/k/rascal")).toEqual({ name: "kit", kit: "rascal" });
    expect(parseRoute("#/k/rascal/remove_checked")).toEqual({ name: "kit", kit: "rascal", task: "remove_checked" });
    expect(parseRoute("#/run/r-8c21_x")).toEqual({ name: "run", id: "r-8c21_x" });
    expect(parseRoute("#/models")).toEqual({ name: "models" });
    const history = parseRoute("#/history?kit=rascal&result=pass");
    expect(history.name === "history" && history.query.get("result")).toBe("pass");
  });
  it("refuses what is not a route", () => {
    for (const hash of ["#/k/Rascal", "#/k/rascal/a/b", "#/run/a%20b", "#/run", "#/nowhere", "#/k/<img>", "#/k/%E0%A4%A", "#/run/%"]) {
      expect(parseRoute(hash).name, hash).toBe("unknown");
    }
  });
  it("writes each route back as it reads", () => {
    for (const hash of ["#/", "#/k/rascal", "#/k/rascal/remove_checked", "#/run/r-1", "#/history", "#/history?kit=squint", "#/models"]) {
      expect(hrefOf(parseRoute(hash))).toBe(hash);
    }
  });
});

describe("the keys", () => {
  const key = (k: string, extra: Partial<{ metaKey: boolean; ctrlKey: boolean; altKey: boolean; shiftKey: boolean }> = {}) => ({
    key: k,
    metaKey: true,
    ctrlKey: false,
    altKey: false,
    shiftKey: false,
    ...extra,
  });
  it("maps ⌘K, ⌘↵, ⌘S, ⌘0 and ⌘1 to ⌘9, and Control the same", () => {
    expect(actionOf(key("k"))).toEqual({ kind: "search" });
    expect(actionOf(key("K"))).toEqual({ kind: "search" });
    expect(actionOf(key("Enter"))).toEqual({ kind: "run" });
    expect(actionOf(key("s"))).toEqual({ kind: "save" });
    expect(actionOf(key("0"))).toEqual({ kind: "home" });
    expect(actionOf(key("1"))).toEqual({ kind: "kit", index: 0 });
    expect(actionOf(key("9"))).toEqual({ kind: "kit", index: 8 });
    expect(actionOf(key("k", { metaKey: false, ctrlKey: true }))).toEqual({ kind: "search" });
  });
  it("leaves other keys to the page", () => {
    expect(actionOf(key("k", { metaKey: false }))).toBeNull();
    expect(actionOf(key("k", { altKey: true }))).toBeNull();
    expect(actionOf(key("k", { shiftKey: true }))).toBeNull();
    expect(actionOf(key("x"))).toBeNull();
  });
});

function answer(status: number, body?: unknown): Response {
  return new Response(body === undefined ? null : JSON.stringify(body), { status, headers: { "Content-Type": "application/json" } });
}

describe("the client", () => {
  it("sends the token as a bearer header, never a cookie", async () => {
    const fetch = vi.fn(async () => answer(200, { version: "0.1.0" }));
    const client = createClient({ token: TOKEN, fetch: fetch as unknown as typeof globalThis.fetch });
    expect(await client.hello()).toEqual({ version: "0.1.0" });
    const [path, init] = fetch.mock.calls[0] as unknown as [string, RequestInit];
    expect(path).toBe("/api/hello");
    expect((init.headers as Record<string, string>).Authorization).toBe(`Bearer ${TOKEN}`);
    expect(init.credentials).toBe("omit");
  });
  it("uses a token it is given later", async () => {
    const fetch = vi.fn(async () => answer(200, { version: "0.1.0" }));
    const client = createClient({ token: null, fetch: fetch as unknown as typeof globalThis.fetch });
    await client.hello();
    client.setToken(TOKEN);
    await client.hello();
    const auth = fetch.mock.calls.map((c) => ((c as unknown as [string, RequestInit])[1].headers as Record<string, string>).Authorization);
    expect(auth).toEqual([undefined, `Bearer ${TOKEN}`]);
  });

  it("turns a refusal into its sentence, the host's own when it gives one", async () => {
    const fetch = vi.fn(async () => answer(413, { error: "The picture is over the limit of 50000000 pixels." }));
    const client = createClient({ token: TOKEN, fetch: fetch as unknown as typeof globalThis.fetch });
    await expect(client.request("POST", "/api/pictures", {})).rejects.toEqual(new ApiError(413, "The picture is over the limit of 50000000 pixels."));
  });
  it("says to run whizbang play again on a 401, and tells the page", async () => {
    const onUnauthorized = vi.fn();
    const client = createClient({ token: "old", fetch: (async () => answer(401, { error: "unauthorized" })) as unknown as typeof globalThis.fetch, onUnauthorized });
    await expect(client.hello()).rejects.toThrow("Run whizbang play again");
    expect(onUnauthorized).toHaveBeenCalledTimes(1);
  });
  it("says the host is not answering when the request fails", async () => {
    const client = createClient({ token: TOKEN, fetch: (async () => Promise.reject(new TypeError("failed"))) as unknown as typeof globalThis.fetch });
    await expect(client.hello()).rejects.toEqual(new ApiError(0, sentenceFor(0)));
  });
  it("has a sentence for each refusal", () => {
    for (const status of [0, 401, 403, 409, 413, 415, 429, 503, 500]) expect(sentenceFor(status).length).toBeGreaterThan(10);
  });
});

describe("a run's start", () => {
  const call = { action: "remove_checked", arguments: { prompt: "the hose" }, handle: "h-1" };
  const bodies = (fetch: ReturnType<typeof vi.fn>) =>
    fetch.mock.calls.map((args) => JSON.parse(String((args as unknown as [string, RequestInit])[1].body)) as Record<string, unknown>);

  it("sends the call, its handle and a request id the page made", async () => {
    const fetch = vi.fn(async () => answer(201, { run: "r-1" }));
    const client = createClient({ token: TOKEN, fetch: fetch as unknown as typeof globalThis.fetch });
    const started = await client.start(call);
    expect(started.run).toBe("r-1");
    const [body] = bodies(fetch);
    expect(body).toEqual({ ...call, request_id: started.requestId });
    expect(started.requestId).toMatch(/^play-.+/);
    expect((fetch.mock.calls[0] as unknown as [string])[0]).toBe("/api/runs");
  });

  it("makes a new id for each start", () => {
    let n = 0;
    expect(newRequestId(() => String((n += 1)))).toBe("play-1");
    expect(newRequestId()).not.toBe(newRequestId());
  });

  it("sends the same id again when the answer was lost, so the host answers the run it started", async () => {
    let calls = 0;
    const fetch = vi.fn(async () => {
      calls += 1;
      if (calls < 3) throw new TypeError("the connection dropped");
      return answer(201, { run: "r-1" });
    });
    const waits: number[] = [];
    const client = createClient({ token: TOKEN, fetch: fetch as unknown as typeof globalThis.fetch, pause: async (ms) => void waits.push(ms) });
    const started = await client.start(call, "play-given");
    expect(started).toEqual({ run: "r-1", requestId: "play-given" });
    expect(bodies(fetch).map((body) => body.request_id)).toEqual(["play-given", "play-given", "play-given"]);
    expect(waits).toHaveLength(2);
    for (const ms of waits) expect(ms >= 250 && ms <= 500, `${ms}`).toBe(true);
  });

  it("takes a 2xx whose body cannot be read for a lost answer, and sends the same id again", async () => {
    let calls = 0;
    const fetch = vi.fn(async () => {
      calls += 1;
      return calls === 1 ? new Response("{not json", { status: 201 }) : answer(201, { run: "r-1" });
    });
    const client = createClient({ token: TOKEN, fetch: fetch as unknown as typeof globalThis.fetch, pause: async () => undefined });
    const started = await client.start(call, "play-same");
    expect(started.run).toBe("r-1");
    expect(bodies(fetch).map((body) => body.request_id)).toEqual(["play-same", "play-same"]);
  });

  it("waits 250 to 500 ms between tries", () => {
    expect(retryPause(() => 0)).toBe(250);
    expect(retryPause(() => 0.999)).toBe(499);
  });

  it("gives up after its tries, and never retries a refusal", async () => {
    const dropped = vi.fn(async () => Promise.reject(new TypeError("dropped")));
    const client = createClient({ token: TOKEN, fetch: dropped as unknown as typeof globalThis.fetch, pause: async () => undefined });
    await expect(client.start(call)).rejects.toMatchObject({ status: 0 });
    expect(dropped).toHaveBeenCalledTimes(START_TRIES);
    for (const status of [409, 429]) {
      const refused = vi.fn(async () => answer(status, { error: "stale" }));
      await expect(createClient({ token: TOKEN, fetch: refused as unknown as typeof globalThis.fetch }).start(call)).rejects.toMatchObject({ status });
      expect(refused).toHaveBeenCalledTimes(1);
    }
  });
});

describe("uploads", () => {
  it("run at most two at once, the rest queued in order", async () => {
    expect(UPLOADS_AT_ONCE).toBe(2);
    const releases: Array<() => void> = [];
    const started: number[] = [];
    let live = 0;
    let most = 0;
    const fetch = vi.fn(
      () =>
        new Promise<Response>((resolve) => {
          live += 1;
          most = Math.max(most, live);
          started.push(started.length);
          releases.push(() => {
            live -= 1;
            resolve(answer(200, { id: "p" }));
          });
        }),
    );
    const client = createClient({ token: TOKEN, fetch: fetch as unknown as typeof globalThis.fetch });
    const all = [0, 1, 2, 3, 4].map(() => client.upload("/api/pictures?name=a.png", new Blob(["x"], { type: "image/png" })));
    await Promise.resolve();
    expect(fetch).toHaveBeenCalledTimes(2);
    while (releases.length > 0) {
      releases.shift()!();
      await new Promise((resolve) => setTimeout(resolve, 0));
    }
    await Promise.all(all);
    expect(fetch).toHaveBeenCalledTimes(5);
    expect(most).toBe(2);
    const init = fetch.mock.calls[0] as unknown as [string, RequestInit];
    expect((init[1].headers as Record<string, string>)["Content-Type"]).toBe("image/png");
  });
  it("the limiter counts what runs and what waits", async () => {
    const limiter = new Limiter(1);
    let release!: () => void;
    const first = limiter.run(() => new Promise<void>((resolve) => (release = resolve)));
    const second = limiter.run(async () => "done");
    expect(limiter.active).toBe(1);
    expect(limiter.queued).toBe(1);
    release();
    await first;
    expect(await second).toBe("done");
  });
});

describe("the poll", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("asks every 250 ms with the events after the last it has, until the run is done", async () => {
    const asked: string[] = [];
    const answers = [
      { after: 2, done: false },
      { after: 5, done: false },
      { after: 6, done: true },
    ];
    const fetch = vi.fn(async (path: string) => {
      asked.push(path);
      return answer(200, answers[asked.length - 1]);
    });
    const client = createClient({ token: TOKEN, fetch: fetch as unknown as typeof globalThis.fetch });
    client.poll<{ after: number; done: boolean }>((after) => `/api/runs/r-1?after=${after}`, (a) => a);
    await vi.advanceTimersByTimeAsync(0);
    expect(asked).toEqual(["/api/runs/r-1?after=0"]);
    await vi.advanceTimersByTimeAsync(POLL_MS);
    await vi.advanceTimersByTimeAsync(POLL_MS);
    await vi.advanceTimersByTimeAsync(POLL_MS * 4);
    expect(asked).toEqual(["/api/runs/r-1?after=0", "/api/runs/r-1?after=2", "/api/runs/r-1?after=5"]);
  });
  it("stops on a refusal other than 429, and backs off on 503 and 429 up to its most", async () => {
    for (const status of [400, 403, 404, 409]) {
      const refused = vi.fn(async () => answer(status, { error: "no" }));
      createClient({ token: TOKEN, fetch: refused as unknown as typeof globalThis.fetch }).poll(() => "/api/runs/r-1", () => ({ after: 0, done: false }));
      await vi.advanceTimersByTimeAsync(POLL_MS * 8);
      expect(refused, String(status)).toHaveBeenCalledTimes(1);
    }
    const asked: number[] = [];
    const busy = vi.fn(async () => {
      asked.push(Date.now());
      return answer(503, { error: "stopping" });
    });
    createClient({ token: TOKEN, fetch: busy as unknown as typeof globalThis.fetch }).poll(() => "/api/runs/r-1", () => ({ after: 0, done: false }));
    await vi.advanceTimersByTimeAsync(POLL_MAX_MS * 6);
    const gaps = asked.slice(1).map((t, i) => t - asked[i]!);
    expect(gaps.slice(0, 4)).toEqual([500, 1000, 2000, 4000]);
    expect(Math.max(...gaps)).toBe(POLL_MAX_MS);
  });

  it("stops when told, and on a 401", async () => {
    const fetch = vi.fn(async () => answer(200, { after: 1, done: false }));
    const client = createClient({ token: TOKEN, fetch: fetch as unknown as typeof globalThis.fetch });
    const stop = client.poll<{ after: number; done: boolean }>(() => "/api/runs/r-1", (a) => a);
    await vi.advanceTimersByTimeAsync(0);
    stop();
    await vi.advanceTimersByTimeAsync(POLL_MS * 4);
    expect(fetch).toHaveBeenCalledTimes(1);
    const refused = vi.fn(async () => answer(401));
    const onError = vi.fn();
    createClient({ token: TOKEN, fetch: refused as unknown as typeof globalThis.fetch }).poll(() => "/api/runs/r-1", () => ({ after: 0, done: false }), onError);
    await vi.advanceTimersByTimeAsync(POLL_MS * 4);
    expect(refused).toHaveBeenCalledTimes(1);
    expect(onError).toHaveBeenCalledTimes(1);
  });
});
