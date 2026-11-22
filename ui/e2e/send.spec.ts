// Send to, Back to the run and Run again (plan m5, task W2; SPEC.md section 7, row 4 of the
// plan's acceptance table): a result handed to another task, or to its own again, as a new
// draft that changes nothing of the run it came from.
import type { Page } from "@playwright/test";
import { expect, test } from "./fixtures";
import { api, file, give, openAt, runOf, runWhenReady, script } from "./play";

test.afterEach(async ({ request, host }) => {
  await script(request, host, "reset");
});

interface Recorded {
  action: string;
  arguments: Array<{ name: string; value: unknown }>;
  inputs: Array<{ port: string; type: string; id: string }>;
  outputs: Array<{ port: string; type: string; id: string }>;
}

/** A removal of `words` from a new picture, run to its end: its run. */
async function removed(page: Page, task: string, seed: number, words: string): Promise<string> {
  await page.goto(`${page.url().split("#")[0]}#/k/rascal/${task}`);
  await give(page, "image", file(`send-${seed}.png`, seed));
  await page.locator("#field-prompt").fill(words);
  await runWhenReady(page);
  await expect(page.locator(".workspace__title-row .tag", { hasText: /DONE|PASS|FAIL/ })).toBeVisible();
  return runOf(page);
}

async function sendTo(page: Page, title: string): Promise<void> {
  await page.locator(".result__send .button").click();
  await page.locator(".send-to .menu__item", { hasText: title }).click();
}

test("send to squint opens check an edit with the source, the edit and the words tagged from rascal", async ({ page, request, host }) => {
  await openAt(page, host, "#/");
  const run = await removed(page, "remove_prompt", 41, "the hose");
  const record = await api<Recorded>(request, host, `/api/runs/${run}/record`);
  await sendTo(page, "Squint · Check an edit");
  await expect(page).toHaveURL(/#\/k\/squint\/source-fidelity$/);
  await expect(page.locator(".sent-from")).toContainText("Sent from Rascal");
  await expect(page.locator(".sent-from")).toContainText(run);
  for (const field of ["source", "candidate", "brief"]) {
    await expect(page.locator(`.field:has(#field-${field}) .picture-field__from, .field:has(#field-${field}) .field__from`).first()).toHaveText("FROM RASCAL");
  }
  await expect(page.locator("#field-brief")).toHaveValue(/the hose/);
  // The source is the removal's picture and the candidate its result.
  const image = record.outputs.find((output) => output.port === "image")!.id;
  await expect(page.locator(".field:has(#field-candidate) .picture-field__name")).toContainText("result");
  expect(image).toMatch(/^sha256:/);
});

test("back to the run returns to the sending result", async ({ page, host }) => {
  await openAt(page, host, "#/");
  const run = await removed(page, "remove_prompt", 42, "the cup");
  await sendTo(page, "Squint · Check an edit");
  await expect(page.locator(".sent-from")).toBeVisible();
  await page.locator(".sent-from").getByRole("button", { name: /Back to the run/ }).click();
  await expect(page).toHaveURL(new RegExp(`#/run/${run}$`));
  await expect(page.locator(".workspace__title-row .tag", { hasText: "DONE" })).toBeVisible();
  await expect(page.getByRole("tab", { name: "Result" })).toBeVisible();
});

test("run again opens a draft whose changes do not touch the result it came from", async ({ page, request, host }) => {
  await openAt(page, host, "#/");
  const run = await removed(page, "remove_prompt", 43, "the bucket");
  await page.getByRole("button", { name: "Run again" }).click();
  await expect(page).toHaveURL(/#\/k\/rascal\/remove_prompt$/);
  await expect(page.locator("#field-prompt")).toHaveValue("the bucket");
  await page.locator("#field-prompt").fill("the ladder");
  // The run it came from is as it was, in the store and on its screen.
  const record = await api<Recorded>(request, host, `/api/runs/${run}/record`);
  expect(record.arguments.find((argument) => argument.name === "prompt")?.value).toBe("the bucket");
  await page.goto(`${host.address}#/run/${run}`);
  await expect(page.locator(".run__inputs")).toContainText("the bucket");
  await expect(page.locator(".run__inputs")).not.toContainText("the ladder");
});

test("a result's mask sent to another rascal task binds as its mask", async ({ page, request, host }) => {
  await openAt(page, host, "#/");
  const run = await removed(page, "remove_prompt", 44, "the rake");
  const record = await api<Recorded>(request, host, `/api/runs/${run}/record`);
  const mask = record.outputs.find((output) => output.type === "mask")!.id;
  await sendTo(page, "Rascal · Protect a first edit");
  await expect(page).toHaveURL(/#\/k\/rascal\/protect$/);
  await expect(page.locator(".field:has(.binding-picker) .field__from")).toHaveText("FROM RASCAL");
  await expect(page.locator('.binding-picker [aria-selected="true"]')).toHaveCount(1);
  await runWhenReady(page);
  await expect(page.locator(".workspace__title-row .tag", { hasText: "DONE" })).toBeVisible();
  const protectedRun = await api<Recorded>(request, host, `/api/runs/${runOf(page)}/record`);
  expect(protectedRun.inputs.find((input) => input.port === "mask")?.id).toBe(mask);
});

test("a removal sent to check an edit carries the job the loop writes", async ({ page, request, host }) => {
  await openAt(page, host, "#/");
  const run = await removed(page, "remove_checked", 45, "the garden hose");
  await sendTo(page, "Squint · Check an edit");
  await expect(page.locator(".sent-from")).toBeVisible();
  await runWhenReady(page);
  await expect(page.locator(".workspace__title-row .tag", { hasText: /PASS|FAIL|UNSURE/ })).toBeVisible();
  const checked = await api<Recorded>(request, host, `/api/runs/${runOf(page)}/record`);
  const job = JSON.parse(String(checked.arguments.find((argument) => argument.name === "job")?.value)) as { requirements: Array<{ id: string; text: string }>; brief: string };
  // The loop's own job: its three requirements, the phrase in the first.
  expect(job.requirements.map((requirement) => requirement.id)).toEqual(["RQ1", "RQ2", "RQ3"]);
  expect(job.requirements[0]!.text).toContain("the garden hose");
  expect(job.brief).toContain("the garden hose");
  expect(run).not.toBe(runOf(page));
});
