// Running a task from the workspace and following it (plan m5, task W1; SPEC.md section 7,
// "Run both tools", row 1 of the plan's acceptance table).
import { expect, test } from "./fixtures";
import { api, file, give, openAt, runOf, runWhenReady, script } from "./play";

/** The five use cases of squint, each a task of its own. */
const USE_CASES = ["source-fidelity", "reference-consistency", "reality-checks", "brief-compliance", "product-fidelity"];

test.afterEach(async ({ request, host }) => {
  await script(request, host, "reset");
});

test("a rascal task runs from a picture and words to a result", async ({ page, host }) => {
  await openAt(page, host, "#/k/rascal/remove_prompt");
  await give(page, "image", file("porch.png", 1));
  await page.locator("#field-prompt").fill("the hose");
  await runWhenReady(page);
  await expect(page.locator(".tag", { hasText: "DONE" })).toBeVisible();
  await expect(page.getByRole("tab", { name: "Result" })).toHaveAttribute("aria-selected", "true");
  await expect(page.locator(".canvas__picture")).toBeVisible();
});

test("a squint task runs from two pictures and a brief to a verdict", async ({ page, host }) => {
  await openAt(page, host, "#/k/squint/source-fidelity");
  await give(page, "candidate", file("edited.png", 2));
  await give(page, "source", file("source.png", 3));
  await page.locator("#field-brief").fill("Keep the fence as it was.");
  await runWhenReady(page);
  await expect(page.locator(".tag", { hasText: "PASS" })).toBeVisible();
});

test("progress shows each step as it starts and ends", async ({ page, request, host }) => {
  await script(request, host, "hold");
  await openAt(page, host, "#/k/rascal/remove_prompt");
  await give(page, "image", file("steps.png", 4));
  await page.locator("#field-prompt").fill("the hose");
  await runWhenReady(page);
  // The segmenter is held: its step is running, and the fill is still to come.
  await expect(page.locator('.step-list [data-state="running"]')).toContainText("segment");
  await expect(page.locator('.step-list [data-state="waiting"]', { hasText: "fill" })).toBeVisible();
  await script(request, host, "release");
  await expect(page.locator(".tag", { hasText: "DONE" })).toBeVisible();
  await expect(page.locator('.step-list [data-state="done"]', { hasText: "segment" })).toBeVisible();
  await expect(page.locator('.step-list [data-state="done"]', { hasText: "fill" })).toBeVisible();
  await expect(page.locator('.step-list [data-state="running"], .step-list [data-state="waiting"]')).toHaveCount(0);
});

test("the running step list shows a step running and an attempt started before the run ends", async ({ page, request, host }) => {
  // The judge fails each check, and every request is held: each release lets the requests
  // waiting then go on and holds the next, until the loop has started its second attempt.
  await script(request, host, "fail,hold");
  await openAt(page, host, "#/k/rascal/remove_checked");
  await give(page, "image", file("loop.png", 5));
  await page.locator("#field-prompt").fill("the hose");
  await runWhenReady(page);
  await expect(page.locator('.step-list [data-state="running"]').first()).toBeVisible();
  const divider = page.locator(".step-list__attempt", { hasText: "ATTEMPT 2" });
  for (let tries = 0; tries < 16 && !(await divider.isVisible()); tries += 1) {
    await script(request, host, "release,hold");
    await divider.waitFor({ state: "visible", timeout: 1_000 }).catch(() => undefined);
  }
  await expect(divider).toBeVisible();
  await expect(page.locator(".workspace__title-row .tag", { hasText: "RUNNING" })).toBeVisible();
  await expect(page.locator('.step-list [data-state="running"]').last()).toBeVisible();
  await script(request, host, "reset");
  await expect(page.locator(".workspace__title-row .tag", { hasText: /PASS|FAIL|DONE/ })).toBeVisible();
});

test("each of the five squint use cases runs from its form", async ({ page, context, host }) => {
  // Two tabs, each running its share of the use cases one after another: each tab waits
  // for one picture's upload before it gives the next, so the two stay within the host's
  // two uploads at once, and at most two runs go at once, within its four.
  const other = await context.newPage();
  // A picture given, and its upload answered, before the next is given.
  const sent = async (tab: typeof page, field: string, given: ReturnType<typeof file>) => {
    const answered = tab.waitForResponse((response) => response.url().includes("/api/pictures?name=") && response.request().method() === "POST");
    await give(tab, field, given);
    expect((await answered).status(), field).toBe(201);
  };
  const lane = async (tab: typeof page, cases: string[], seed: number) => {
    await openAt(tab, host, "#/");
    for (const useCase of cases) {
      await tab.goto(`${host.address}#/k/squint/${useCase}`);
      await sent(tab, "candidate", file(`${useCase}.png`, (seed += 1)));
      if (await tab.locator("#field-source").count()) await sent(tab, "source", file("source.png", (seed += 1)));
      if (await tab.locator("#field-references").count()) await sent(tab, "references", file("reference.png", (seed += 1)));
      if (await tab.locator("#field-brief").count()) await tab.locator("#field-brief").fill("A kitchen with a window, and no person in it.");
      await runWhenReady(tab);
      await expect(tab.locator(".tag", { hasText: "PASS" }), useCase).toBeVisible();
    }
  };
  try {
    await Promise.all([lane(page, USE_CASES.filter((_, at) => at % 2 === 0), 10), lane(other, USE_CASES.filter((_, at) => at % 2 === 1), 30)]);
  } finally {
    await other.close();
  }
});

test("stop ends the run as cancelled and nothing is committed", async ({ page, request, host }) => {
  await script(request, host, "hold");
  await openAt(page, host, "#/k/rascal/remove_prompt");
  await give(page, "image", file("stop.png", 20));
  await page.locator("#field-prompt").fill("the hose");
  await runWhenReady(page);
  await expect(page.locator('.step-list [data-state="running"]')).toBeVisible();
  await page.getByRole("button", { name: "Stop run" }).click();
  await expect(page.locator(".tag", { hasText: "CANCELLED" })).toBeVisible();
  const ended = await api<{ outcome: string; outputs?: unknown[] }>(request, host, `/api/runs/${runOf(page)}`);
  expect(ended.outcome).toBe("cancelled");
  expect(ended.outputs ?? []).toEqual([]);
  await expect(page.getByRole("tab", { name: "Result" })).toHaveCount(0);
});
