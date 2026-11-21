// The client's fetches of artifacts: at most two at once, shared, aborted when nobody waits,
// and their addresses kept in a cache that revokes the oldest (the review of task W1).
import { describe, expect, it } from "vitest";
import { ApiError, ARTIFACTS_AT_ONCE, BLOBS_KEPT, createClient, nameOf } from "./api";

/** A fetch whose answers the test hands out by hand. */
function held() {
  const waiting: Array<{ path: string; signal?: AbortSignal | null; answer: (status?: number) => void }> = [];
  const fetch = (path: string, init?: RequestInit) =>
    new Promise<Response>((resolve, reject) => {
      init?.signal?.addEventListener("abort", () => reject(new DOMException("aborted", "AbortError")));
      waiting.push({ path, signal: init?.signal, answer: (status = 200) => resolve(new Response(new Blob(["x"]), { status })) });
    });
  return { waiting, fetch: fetch as unknown as typeof globalThis.fetch };
}

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

function blobs() {
  let made = 0;
  const revoked: string[] = [];
  return { revoked, make: () => `blob:test-${(made += 1)}`, revoke: (url: string) => void revoked.push(url) };
}

describe("artifact fetches", () => {
  it("sends at most two at once and the third when one is done", async () => {
    const host = held();
    const client = createClient({ token: "t", fetch: host.fetch, blobs: blobs() });
    expect(ARTIFACTS_AT_ONCE).toBe(2);
    const asked = ["/api/artifacts/a", "/api/artifacts/b", "/api/artifacts/c"].map((path) => client.blobUrl(path));
    await settle();
    expect(host.waiting.map((one) => one.path)).toEqual(["/api/artifacts/a", "/api/artifacts/b"]);
    host.waiting[0]!.answer();
    await asked[0];
    await settle();
    expect(host.waiting.map((one) => one.path)).toEqual(["/api/artifacts/a", "/api/artifacts/b", "/api/artifacts/c"]);
    host.waiting[1]!.answer();
    host.waiting[2]!.answer();
    expect(await Promise.all(asked)).toEqual(["blob:test-1", "blob:test-2", "blob:test-3"]);
  });

  it("aborts a fetch nobody waits for any more, and shares one with whoever still does", async () => {
    const host = held();
    const client = createClient({ token: "t", fetch: host.fetch, blobs: blobs() });
    const first = new AbortController();
    const second = new AbortController();
    const one = client.blobUrl("/api/artifacts/a", first.signal);
    const two = client.blobUrl("/api/artifacts/a", second.signal);
    await settle();
    expect(host.waiting).toHaveLength(1);
    first.abort();
    await expect(one).rejects.toBeInstanceOf(ApiError);
    expect(host.waiting[0]!.signal?.aborted).toBe(false);
    second.abort();
    await expect(two).rejects.toBeInstanceOf(ApiError);
    expect(host.waiting[0]!.signal?.aborted).toBe(true);
    // A queued fetch given up before its turn is never sent.
    const busy = [client.blobUrl("/api/artifacts/b"), client.blobUrl("/api/artifacts/c")];
    const late = new AbortController();
    const queued = client.blobUrl("/api/artifacts/d", late.signal);
    late.abort();
    await expect(queued).rejects.toBeInstanceOf(ApiError);
    await settle();
    host.waiting.slice(1).forEach((one) => one.answer());
    await Promise.all(busy);
    await settle();
    expect(host.waiting.map((one) => one.path)).not.toContain("/api/artifacts/d");
  });

  it("keeps the newest addresses and revokes the oldest", async () => {
    const host = held();
    const made = blobs();
    const client = createClient({ token: "t", fetch: host.fetch, blobs: made });
    for (let at = 0; at <= BLOBS_KEPT; at += 1) {
      const asked = client.blobUrl(`/api/artifacts/${at}`);
      await settle();
      host.waiting[host.waiting.length - 1]!.answer();
      await asked;
    }
    expect(made.revoked).toEqual(["blob:test-1"]);
    // One still kept is answered from the cache, with no fetch.
    const sent = host.waiting.length;
    expect(await client.blobUrl("/api/artifacts/5")).toBe("blob:test-6");
    expect(host.waiting).toHaveLength(sent);
  });
});

describe("a file's name as sent", () => {
  it("is cut between code points", () => {
    const name = `${"a".repeat(199)}\u{1F4F7}tail.png`;
    const cut = nameOf({ name });
    expect(Array.from(cut)).toHaveLength(200);
    expect(cut.endsWith("\u{1F4F7}")).toBe(true);
    expect(nameOf({ name: "short.png" })).toBe("short.png");
  });
});
