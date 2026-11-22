// Quit and the terminal's interrupt (plan m5, "Stopping"; ADR 0041; SPEC.md section 7,
// "Quit and terminal shutdown cancel and account for running work and release the store",
// row 10 of the plan's acceptance table). Each case has a host of its own over a store
// that outlives it: the host is quit, then the store is read and opened again.
import { execFileSync, type ChildProcess } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import type { APIRequestContext, Page } from "@playwright/test";
import { BINARY, expect, startHost, stopHost, test, type Host } from "./fixtures";
import { api, file, give, openAt, runOf, runWhenReady, script } from "./play";

/** A removal started from the page and held at its segmenter: its run's id. */
async function going(page: Page, request: APIRequestContext, host: Host, seed: number): Promise<string> {
  await script(request, host, "hold");
  await openAt(page, host, "#/k/rascal/remove_prompt");
  await give(page, "image", file(`quit-${seed}.png`, seed));
  await page.locator("#field-prompt").fill("the hose");
  await runWhenReady(page);
  await expect(page.locator('.step-list [data-state="running"]')).toBeVisible();
  return runOf(page);
}

/**
 * Once the host has exited: the ledger holds no open reservation for the run, the store
 * takes a writer at once (another host opens over it), and the run's record says cancelled.
 */
async function accounted(request: APIRequestContext, store: string, run: string): Promise<void> {
  const books = execFileSync(resolve(BINARY), ["cli", "spend", "status", "--store", store, "--run", run], { encoding: "utf8" });
  expect(books).toContain("reservation: released");
  expect(books).not.toMatch(/reservation: (open|interrupted|frozen)/);
  const asked = Date.now();
  const again = await startHost(store);
  try {
    expect(Date.now() - asked, "a writer is taken at once").toBeLessThan(5_000);
    const record = await api<{ outcome: string; outputs: unknown[] }>(request, again.host, `/api/runs/${run}/record`);
    expect(record.outcome).toBe("cancelled");
    expect(record.outputs).toEqual([]);
  } finally {
    await stopHost(again.child);
  }
}

/** A host of the case's own over a new store, stopped and the store removed when done. */
async function withOwnHost(use: (host: Host, child: ChildProcess, store: string) => Promise<void>): Promise<void> {
  const store = mkdtempSync(join(tmpdir(), "whizbang-e2e-quit-"));
  const { host, child } = await startHost(store);
  try {
    await use(host, child, store);
  } finally {
    await stopHost(child);
    rmSync(store, { recursive: true, force: true });
  }
}

test("quit with a run going cancels it, its record says cancelled, the ledger has no open reservation, and the store takes a writer at once", async ({ page, request }) => {
  await withOwnHost(async (host, _child, store) => {
    const run = await going(page, request, host, 61);
    await page.locator(".top-bar__quit").click();
    const dialog = page.getByRole("alertdialog", { name: "Quit the playground?" });
    await expect(dialog).toContainText("The run still going is cancelled.");
    await expect(dialog).toContainText("accounted for in the ledger");
    await dialog.getByRole("button", { name: "Quit" }).click();
    await expect(page.getByRole("heading", { name: "The playground has quit" })).toBeVisible();
    await expect(page.locator("main")).toContainText("It cancelled the run still going");
    await host.exited;
    await accounted(request, store, run);
  });
});

test("an interrupt from the terminal does the same", async ({ page, request }) => {
  await withOwnHost(async (host, child, store) => {
    const run = await going(page, request, host, 62);
    child.kill("SIGINT");
    await host.exited;
    expect(child.exitCode).toBe(0);
    await accounted(request, store, run);
  });
});
