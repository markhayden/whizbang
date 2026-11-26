// The four negative cases of `SPEC.md` section 10, as a hostile page would try them in
// Chrome (plan m5, task U1, item 7; the Rust negative tests of the same are H1's).
import { request as http } from "node:http";
import type { Page } from "@playwright/test";
import { expect, test } from "./fixtures";
import { bundledVia, pictureVia, runVia } from "./bundled";
import { openAt, READY } from "./play";
import { largePng } from "./png";

/** What a page's fetch came to: its status, or that the browser refused to hand it over. */
type Outcome = { status: number; allowOrigin: string | null } | { failed: string };

/**
 * What the host answered the page, as Chrome saw it on the wire. A response the browser
 * keeps from the page (no header allows the other origin) raises no response event in
 * Playwright, so the answers are read through Chrome's own protocol: each request's
 * address, and its status and headers as they arrived.
 */
async function answersFrom(page: Page, origin: string): Promise<Array<{ url: string; status: number; headers: Record<string, string> }>> {
  const cdp = await page.context().newCDPSession(page);
  await cdp.send("Network.enable");
  const urls = new Map<string, string>();
  // Chrome may say what arrived before it says what was sent (a preflight's answer, from a
  // fast host): such an answer waits here for its request's address.
  const early = new Map<string, Array<{ status: number; headers: Record<string, string> }>>();
  const seen: Array<{ url: string; status: number; headers: Record<string, string> }> = [];
  const keep = (url: string, answer: { status: number; headers: Record<string, string> }) => {
    if (url.startsWith(origin)) seen.push({ url, ...answer });
  };
  cdp.on("Network.requestWillBeSent", (event) => {
    urls.set(event.requestId, event.request.url);
    for (const answer of early.get(event.requestId) ?? []) keep(event.request.url, answer);
    early.delete(event.requestId);
  });
  cdp.on("Network.responseReceivedExtraInfo", (event) => {
    const headers = Object.fromEntries(Object.entries(event.headers).map(([k, v]) => [k.toLowerCase(), String(v)]));
    const answer = { status: event.statusCode, headers };
    const url = urls.get(event.requestId);
    if (url === undefined) early.set(event.requestId, [...(early.get(event.requestId) ?? []), answer]);
    else keep(url, answer);
  });
  return seen;
}

/** A request to the host from Node, outside any browser, with the headers given. */
function direct(url: string, headers: Record<string, string>, method = "GET"): Promise<{ status: number; allowOrigin: string | undefined }> {
  return new Promise((resolve, reject) => {
    const sent = http(url, { method, headers }, (response) => {
      response.resume();
      resolve({ status: response.statusCode ?? 0, allowOrigin: response.headers["access-control-allow-origin"] as string | undefined });
    });
    sent.on("error", reject);
    sent.end();
  });
}

test("a fetch of the api without the token from another origin is refused", async ({ page, host, otherOrigin }) => {
  await page.goto(otherOrigin.address);
  const answers = await answersFrom(page, host.origin);
  const outcome = await page.evaluate(async (url): Promise<Outcome> => {
    try {
      const response = await fetch(url);
      return { status: response.status, allowOrigin: response.headers.get("access-control-allow-origin") };
    } catch (error) {
      return { failed: String(error) };
    }
  }, `${host.address}api/hello`);
  expect(outcome).toHaveProperty("failed");
  await expect.poll(() => answers.length).toBeGreaterThan(0);
  expect(answers.map((answer) => answer.status)).toEqual([401]);
  expect(answers[0]!.headers["access-control-allow-origin"]).toBeUndefined();
});

test("a fetch with the token from another origin is refused and the page cannot read the answer", async ({ page, host, otherOrigin }) => {
  await page.goto(otherOrigin.address);
  const answers = await answersFrom(page, host.origin);
  const outcomes = await page.evaluate(
    async ({ url, token }) => {
      const tries: Array<() => Promise<Response>> = [
        () => fetch(`${url}api/hello`, { headers: { Authorization: `Bearer ${token}` } }),
        () => fetch(`${url}api/quit`, { method: "POST", headers: { Authorization: `Bearer ${token}` } }),
        () => fetch(`${url}api/quit`, { method: "POST", mode: "no-cors" }),
      ];
      const said: string[] = [];
      for (const attempt of tries) {
        try {
          const response = await attempt();
          said.push(`read ${response.type} ${response.status}`);
        } catch {
          said.push("refused");
        }
      }
      return said;
    },
    { url: host.address, token: host.token },
  );
  // The read with the token, its preflight refused; the change, the same; the change sent
  // without a preflight (no-cors, so with no token), its answer kept from the page.
  expect(outcomes).toEqual(["refused", "refused", "refused"]);
  await expect.poll(() => answers.length).toBe(3);
  for (const answer of answers) {
    expect([401, 403], `${answer.url}: ${answer.status}`).toContain(answer.status);
    expect(answer.headers["access-control-allow-origin"]).toBeUndefined();
  }
  // The guard itself, outside the browser: the token with that origin is refused 403.
  const guarded = await direct(`${host.address}api/hello`, { Host: `127.0.0.1:${host.port}`, Authorization: `Bearer ${host.token}`, Origin: otherOrigin.address.replace(/\/$/, "") });
  expect(guarded).toEqual({ status: 403, allowOrigin: undefined });
  // And the host is still there: the quit from the other origin did nothing.
  expect((await direct(`${host.address}api/hello`, { Authorization: `Bearer ${host.token}` })).status).toBe(200);
});

test("a request to the host under another name that resolves to loopback is refused", async ({ page, host }) => {
  const response = await page.goto(`http://rebind.test:${host.port}/`);
  expect(response?.status()).toBe(421);
  const outcome = await page.evaluate(async (token) => {
    const response = await fetch("/api/hello", { headers: { Authorization: `Bearer ${token}` } });
    return response.status;
  }, host.token);
  expect(outcome).toBe(421);
});

test("an upload of a picture over the pixel limit is refused, naming the limit", async ({ page, host }) => {
  await page.goto(host.opening);
  await expect(page.getByRole("status")).toContainText(READY);
  const picture = largePng().toString("base64");
  const outcome = await page.evaluate(
    async ({ picture, token }) => {
      const bytes = Uint8Array.from(atob(picture), (c) => c.charCodeAt(0));
      const response = await fetch("/api/pictures?name=large.png", {
        method: "POST",
        headers: { Authorization: `Bearer ${token}`, "Content-Type": "image/png" },
        body: bytes,
      });
      return { status: response.status, body: (await response.json()) as { error?: string } };
    },
    { picture, token: host.token },
  );
  expect(outcome.status).toBe(413);
  expect(outcome.body.error).toContain("50000000");
});

test("a bundle holding markup in its words shows them as text", async ({ page, request, host }) => {
  const hostile = `<img src=x onerror="document.title='ran'">`;
  // A removal whose words are markup, then `protect` on what it made. Not a check: the
  // loop's job refuses a phrase that holds `<` or `>` (plan mE, task L2).
  const image = await pictureVia(request, host, 91);
  const removal = await runVia(request, host, "remove_prompt", { image, prompt: hostile });
  const record = await (await request.get(`${host.address}api/runs/${removal}/record`, { headers: { Authorization: `Bearer ${host.token}` } })).json();
  const target = (record.send_to as Array<{ task: string; values: Record<string, unknown> }>).find((one) => one.task === "protect")!;
  const second = await runVia(request, host, "protect", { ...target.values });
  const digest = await bundledVia(request, host, [removal, second]);
  await openAt(page, host, `#/bundles/${digest}`);
  await expect(page.locator(".bundle-card")).toContainText("A chain of 2 steps");
  await page.goto(`${host.address}#/chain/${digest}`);
  await page.getByRole("button", { name: "Review step 1" }).click();
  // The bundle's words are the draft's, as text in its field: nothing of them is markup.
  await expect(page.locator("#field-prompt")).toHaveValue(hostile);
  expect(await page.locator("img[onerror]").count()).toBe(0);
  expect(await page.title()).not.toBe("ran");
});
