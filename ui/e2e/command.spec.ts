// Copy command (plan m5, task W2, item 5; SPEC.md section 7, row 8 of the plan's acceptance
// table): a saved run gives the command that makes the same call, run here in a shell
// with `whizbang` replaced by the fixture's own command line (`play_fixture cli`), whose
// fakes are the host's, against the same store, once the host has quit.
import { execFileSync } from "node:child_process";
import { resolve } from "node:path";
import type { Page } from "@playwright/test";
import { BINARY, expect, test, withHost } from "./fixtures";
import { file, give, openAt, runOf, runWhenReady } from "./play";

interface Stored {
  identity: { run: string; parent: string | null };
  asked: { action: string; arguments: Array<{ name: string; value: unknown }>; settings: unknown[]; config: string | null };
  inputs: unknown[];
  models: Array<{ capability: string; backend: string; model: string }>;
  steps: Array<{ run: string }>;
  time: { end_ms: number };
}

/** A word quoted for a POSIX shell. */
function quoted(word: string): string {
  return `'${word.replace(/'/g, `'\\''`)}'`;
}

/** The command the page shows once Copy command is pressed. */
async function copied(page: Page): Promise<string> {
  await page.getByRole("button", { name: /Copy command/ }).click();
  const shown = page.locator(".result__command-text");
  await expect(shown).toContainText("whizbang ");
  return (await shown.textContent()) ?? "";
}

test("copy command gives a command that, run in a shell against the same store, makes the same call", async ({ page, request }) => {
  await withHost([], async (host) => {
    await openAt(page, host, "#/k/rascal/remove_prompt");
    await give(page, "image", file("command.png", 51));
    await page.locator("#field-prompt").fill("the 'hose' and $HOME");
    await runWhenReady(page);
    await expect(page.locator(".workspace__title-row .tag", { hasText: "DONE" })).toBeVisible();
    const first = runOf(page);
    const text = await copied(page);
    // The host quits first: the store has one writer.
    const quit = await request.post(`${host.address}api/quit`, { headers: { Authorization: `Bearer ${host.token}`, Origin: host.origin } });
    expect(quit.status()).toBe(202);
    await host.exited;
    const binary = resolve(BINARY);
    const script = text.replace(/^whizbang /m, `${quoted(binary)} cli `);
    execFileSync("/bin/sh", ["-c", script], { stdio: "pipe" });
    // Every record of the store, as `runs export` writes them.
    const exported = execFileSync(binary, ["cli", "runs", "export", "--store", host.store], { encoding: "utf8" });
    const records: Stored[] = exported
      .split("\n")
      .filter((line) => line.trim())
      .map((line) => JSON.parse(line) as Stored);
    const byRun = new Map(records.map((record) => [record.identity.run, record]));
    const tops = records.filter((record) => record.identity.parent === null && record.asked.action === "remove_prompt").sort((a, b) => a.time.end_ms - b.time.end_ms);
    expect(tops.map((record) => record.identity.run)).toContain(first);
    const a = byRun.get(first)!;
    const b = tops.find((record) => record.identity.run !== first)!;
    const answered = (record: Stored): string[] => {
      const seen = new Set<string>();
      const walk = (one: Stored | undefined) => {
        if (!one) return;
        for (const model of one.models) seen.add(`${model.capability}:${model.backend}:${model.model}`);
        for (const step of one.steps) walk(byRun.get(step.run));
      };
      walk(record);
      return [...seen].sort();
    };
    // The same call: action, arguments once each default is filled in, inputs, resolved
    // settings, the config, and the backend and model of each need.
    expect(b.asked.action).toBe(a.asked.action);
    const filled = [...a.asked.arguments];
    for (const argument of b.asked.arguments) if (!filled.some((given) => given.name === argument.name)) filled.push(argument);
    const named = (list: Array<{ name: string }>) => [...list].sort((x, y) => x.name.localeCompare(y.name));
    expect(named(b.asked.arguments)).toEqual(named(filled));
    expect(b.asked.arguments.find((argument) => argument.name === "prompt")?.value).toEqual({ text: "the 'hose' and $HOME" });
    expect(b.inputs).toEqual(a.inputs);
    expect(b.asked.settings).toEqual(a.asked.settings);
    expect(b.asked.config).toBe(a.asked.config);
    expect(answered(b)).toEqual(answered(a));
  });
});

test("the command of a check sent from a removal carries its whole job", async ({ page, host }) => {
  await openAt(page, host, "#/k/rascal/remove_checked");
  await give(page, "image", file("job.png", 52));
  await page.locator("#field-prompt").fill("the hose reel");
  await runWhenReady(page);
  await expect(page.locator(".workspace__title-row .tag", { hasText: /PASS|FAIL/ })).toBeVisible();
  await page.locator(".result__send .button").click();
  await page.locator(".send-to .menu__item", { hasText: "Check an edit" }).click();
  await runWhenReady(page);
  await expect(page.locator(".workspace__title-row .tag", { hasText: /PASS|FAIL|UNSURE/ })).toBeVisible();
  const text = await copied(page);
  expect(text).toContain("squint check source-fidelity");
  expect(text).toContain("--job-json");
  for (const requirement of ["RQ1", "RQ2", "RQ3", "the hose reel"]) expect(text).toContain(requirement);
  expect(text.match(/--image=sha256:[0-9a-f]{64}/g)).toHaveLength(2);
});
