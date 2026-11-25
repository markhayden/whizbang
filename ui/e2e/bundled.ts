// What the cases of bundles, chains and replay share (plan m7, task U2): runs made and
// bundles written and taken in through the host's API, as a page would, so a case spends
// its seconds on the screens it is about.
import type { APIRequestContext } from "@playwright/test";
import { expect, type Host } from "./fixtures";
import { picture } from "./play";

function headers(host: Host, type = "application/json"): Record<string, string> {
  return { Authorization: `Bearer ${host.token}`, Origin: host.origin, "Content-Type": type };
}

/** A POST of JSON with the token, as the page sends one: its answer. */
export async function post<T>(request: APIRequestContext, host: Host, path: string, body: unknown, status = 200): Promise<T> {
  const answer = await request.post(`${host.address}${path.replace(/^\//, "")}`, { headers: headers(host), data: JSON.stringify(body) });
  expect(answer.status(), `${path}: ${await answer.text()}`).toBe(status);
  return (await answer.json()) as T;
}

/** A picture sent as the page sends one: its identity. */
export async function pictureVia(request: APIRequestContext, host: Host, seed: number): Promise<string> {
  const answer = await request.post(`${host.address}api/pictures?name=bundle-${seed}.png`, { headers: headers(host, "image/png"), data: picture(seed) });
  expect(answer.status()).toBe(201);
  return ((await answer.json()) as { id: string }).id;
}

let made = 0;

/** A call prepared and started through the API, to its end: its run. */
export async function runVia(request: APIRequestContext, host: Host, action: string, args: Record<string, unknown>): Promise<string> {
  const prepared = await post<{ ready: boolean; handle: string }>(request, host, "/api/prepare", { action, arguments: args });
  expect(prepared.ready).toBe(true);
  made += 1;
  const started = await post<{ run: string }>(request, host, "/api/runs", { action, arguments: args, handle: prepared.handle, request_id: `e2e-bundle-${Date.now()}-${made}` }, 201);
  await expect
    .poll(async () => {
      const answer = await request.get(`${host.address}api/runs/${started.run}?after=0`, { headers: { Authorization: `Bearer ${host.token}` } });
      return ((await answer.json()) as { state: string }).state;
    })
    .toBe("ended");
  return started.run;
}

/** A removal and the check of its edit, through the API: the chain's two runs. */
export async function chainVia(request: APIRequestContext, host: Host, seed: number, words = "the hose"): Promise<{ removal: string; check: string }> {
  const image = await pictureVia(request, host, seed);
  const removal = await runVia(request, host, "remove_prompt", { image, prompt: words });
  const record = await (await request.get(`${host.address}api/runs/${removal}/record`, { headers: { Authorization: `Bearer ${host.token}` } })).json();
  const target = (record.send_to as Array<{ task: string; values: Record<string, unknown> }>).find((one) => one.task === "source-fidelity")!;
  const values = { ...target.values };
  delete values.brief;
  const check = await runVia(request, host, "source-fidelity", { ...values, job_of: removal });
  return { removal, check };
}

/** A chain's bundle, written by the host and taken in by it: its digest. */
export async function bundledVia(request: APIRequestContext, host: Host, runs: string[]): Promise<string> {
  const written = await request.post(`${host.address}api/bundles/chain`, { headers: headers(host), data: JSON.stringify({ runs }) });
  expect(written.status()).toBe(200);
  const taken = await request.post(`${host.address}api/bundles`, { headers: headers(host, "application/x-tar"), data: await written.body() });
  expect(taken.status(), await taken.text()).toBe(201);
  return ((await taken.json()) as { digest: string }).digest;
}
