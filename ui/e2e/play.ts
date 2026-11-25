// What the cases that run a task share (plan m5, task W1): pictures to send, the fixture's
// script, and the page opened at a route with its token.
import { deflateSync } from "node:zlib";
import type { APIRequestContext, Page } from "@playwright/test";
import { expect, type Host } from "./fixtures";
import { crcChunk } from "./png";

/** A 64 by 64 picture of its own colours, so two seeds are two pictures. */
export function picture(seed: number, width = 64, height = 64): Buffer {
  const header = Buffer.alloc(13);
  header.writeUInt32BE(width, 0);
  header.writeUInt32BE(height, 4);
  header[8] = 8; // bits a channel
  header[9] = 2; // red, green, blue
  const rows: Buffer[] = [];
  for (let y = 0; y < height; y += 1) {
    const row = Buffer.alloc(1 + width * 3);
    for (let x = 0; x < width; x += 1) {
      row[1 + x * 3] = (x * 4 + seed * 37) & 255;
      row[2 + x * 3] = (y * 4 + seed * 11) & 255;
      row[3 + x * 3] = (seed * 53) & 255;
    }
    rows.push(row);
  }
  const signature = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
  return Buffer.concat([signature, crcChunk("IHDR", header), crcChunk("IDAT", deflateSync(Buffer.concat(rows))), crcChunk("IEND", Buffer.alloc(0))]);
}

/** A file for an input of the page: a picture of this seed under this name. */
export function file(name: string, seed: number) {
  return { name, mimeType: "image/png", buffer: picture(seed) };
}

/** Sets the fixture's script: hold, release, pass, fail, reset (in order, with commas). */
export async function script(request: APIRequestContext, host: Host, words: string): Promise<void> {
  const answer = await request.get(`${host.address}api/hello`, { headers: { Authorization: `Bearer ${host.token}`, "x-fixture-script": words } });
  expect(answer.status(), `the fixture took the script ${words}`).toBe(200);
}

/** A call of the API with the token, as a case checks what the host holds. */
export async function api<T>(request: APIRequestContext, host: Host, path: string): Promise<T> {
  const answer = await request.get(`${host.address}${path.replace(/^\//, "")}`, { headers: { Authorization: `Bearer ${host.token}` } });
  expect(answer.status(), path).toBe(200);
  return (await answer.json()) as T;
}

/** The bytes of an artifact, as the host serves them. */
export async function bytesOf(request: APIRequestContext, host: Host, id: string): Promise<Buffer> {
  const answer = await request.get(`${host.address}api/artifacts/${id}`, { headers: { Authorization: `Bearer ${host.token}` } });
  expect(answer.status(), id).toBe(200);
  return answer.body();
}

/**
 * The status bar once the host has answered: "ready", and not the "not ready" it says
 * before then, which also holds the word.
 */
export const READY = /(?<!not )ready/;

/** Opens the page with its token, then at a route of the same tab. */
export async function openAt(page: Page, host: Host, route: string): Promise<void> {
  await page.goto(host.opening);
  await expect(page.getByRole("status")).toContainText(READY);
  await page.goto(`${host.address}${route}`);
}

/** Sends a picture to a picture field of the workspace, by the field's name. */
export async function give(page: Page, field: string, files: ReturnType<typeof file> | Array<ReturnType<typeof file>>): Promise<void> {
  await page.locator(`.field:has(#field-${field}) input[type=file]`).setInputFiles(files);
}

/**
 * The Run button, once a ready preparation has enabled it; then what the run's screen
 * shows, the running one or its result once it has ended: the line that names the run.
 * A start the host refused leaves the workspace saying why, which is named here rather
 * than waited past. The address is checked after.
 */
export async function runWhenReady(page: Page): Promise<void> {
  const run = page.locator(".workspace__run .button--primary");
  await expect(run).toBeEnabled();
  await run.click();
  const named = page.locator(".workspace__head .workspace__purpose.mono", { hasText: /^run \S/ });
  const refused = page.locator(".workspace__foot:has(.workspace__run) .field__error[role=alert]");
  await expect(named.or(refused).first()).toBeVisible();
  // A problem said before the click may linger a moment after it: the run's screen is
  // still waited for, and a failure names what the workspace said.
  if (!(await named.isVisible())) await expect(named, `the run's screen, after: ${(await refused.allTextContents()).join(" ") || "nothing"}`).toBeVisible();
  await expect(page).toHaveURL(/#\/run\//);
}

/** The run's id, from the address of the running run's screen. */
export function runOf(page: Page): string {
  const found = /#\/run\/([^/?]+)/.exec(page.url());
  if (!found?.[1]) throw new Error(`not a run's address: ${page.url()}`);
  return decodeURIComponent(found[1]);
}
