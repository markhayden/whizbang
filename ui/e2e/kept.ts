// What the cases of history, reports and records share (plan m5, task W3): a fixture host
// of a case's own over a store that outlives it, so the host can be stopped and started
// again over the same runs; and a task run from the page to its end.
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { FrameLocator, Page } from "@playwright/test";
import { expect, startHost, stopHost, type Host } from "./fixtures";
import { file, give, openAt, runOf, runWhenReady } from "./play";

export interface Kept {
  host: Host;
  /** Stops the host and starts another over the same store: its address and token are new. */
  restart(): Promise<Host>;
  /** The same, doing `between` to the store's folder while no host holds it. */
  restartAfter(between: (store: string) => void): Promise<Host>;
}

/** A host over a store of the case's own, stopped and the store removed when `use` is done. */
export async function withKeptHost<T>(use: (kept: Kept) => Promise<T>): Promise<T> {
  const store = mkdtempSync(join(tmpdir(), "whizbang-e2e-kept-"));
  let started = await startHost(store);
  const kept: Kept = {
    host: started.host,
    restart: () => kept.restartAfter(() => undefined),
    restartAfter: async (between) => {
      await stopHost(started.child);
      between(store);
      started = await startHost(store);
      kept.host = started.host;
      return started.host;
    },
  };
  try {
    return await use(kept);
  } finally {
    await stopHost(started.child);
    rmSync(store, { recursive: true, force: true });
  }
}

/** Runs a rascal removal from the page, to its end; its run's id. */
export async function removal(page: Page, host: Host, name: string, seed: number, words = "the hose"): Promise<string> {
  await openAt(page, host, "#/k/rascal/remove_prompt");
  await give(page, "image", file(name, seed));
  await page.locator("#field-prompt").fill(words);
  await runWhenReady(page);
  await expect(page.locator(".workspace__title-row .tag", { hasText: "DONE" })).toBeVisible();
  return runOf(page);
}

/** Runs squint's reality check from the page, to its verdict; its run's id. */
export async function realityCheck(page: Page, host: Host, name: string, seed: number, verdict: "PASS" | "FAIL"): Promise<string> {
  await openAt(page, host, "#/k/squint/reality-checks");
  await give(page, "candidate", file(name, seed));
  await runWhenReady(page);
  await expect(page.locator(".workspace__title-row .tag", { hasText: verdict })).toBeVisible();
  return runOf(page);
}

/** The report of a run, opened in the page; the frame it is shown in. */
export async function openReport(page: Page, host: Host, run: string): Promise<FrameLocator> {
  await openAt(page, host, `#/run/${run}/report`);
  const frame = page.frameLocator("iframe.isolated-frame");
  await expect(frame.locator("h1")).toBeVisible();
  return frame;
}

/** Every picture of a frame is decoded: it has pixels. */
export async function picturesDecoded(frame: FrameLocator): Promise<number> {
  const images = frame.locator("img");
  const count = await images.count();
  for (let at = 0; at < count; at += 1) {
    await expect.poll(() => images.nth(at).evaluate((img: HTMLImageElement) => img.complete && img.naturalWidth > 0)).toBe(true);
  }
  return count;
}
