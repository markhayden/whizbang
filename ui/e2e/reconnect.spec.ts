// Runs belong to the host, not to the page (plan m5, "Reconnecting"; SPEC.md section 7,
// rows 1 and 2 of the plan's acceptance table).
import { expect, test } from "./fixtures";
import { file, give, openAt, runOf, runWhenReady, script } from "./play";

test.afterEach(async ({ request, host }) => {
  await script(request, host, "reset");
});

test("a reloaded page shows the running run and follows it to its end", async ({ page, request, host }) => {
  await script(request, host, "hold");
  await openAt(page, host, "#/k/rascal/remove_prompt");
  await give(page, "image", file("reload.png", 30));
  await page.locator("#field-prompt").fill("the hose");
  await runWhenReady(page);
  await expect(page.locator('.step-list [data-state="running"]')).toBeVisible();
  await page.reload();
  await expect(page.locator(".workspace__title-row .tag", { hasText: "RUNNING" })).toBeVisible();
  await expect(page.locator('.step-list [data-state="running"]')).toContainText("segment");
  await script(request, host, "release");
  await expect(page.locator(".tag", { hasText: "DONE" })).toBeVisible();
});

test("a run started in a tab that is closed ends completed, and a new tab finds it in Continue and opens its result", async ({ page, request, host, context }) => {
  await script(request, host, "hold");
  await openAt(page, host, "#/k/rascal/remove_prompt");
  await give(page, "image", file("closed.png", 31));
  await page.locator("#field-prompt").fill("the hose");
  await runWhenReady(page);
  const run = runOf(page);
  await expect(page.locator('.step-list [data-state="running"]')).toBeVisible();
  await page.close();
  const again = await context.newPage();
  await again.goto(host.opening);
  const row = again.locator(".continue-row", { hasText: run });
  await expect(row).toContainText("RUNNING");
  await row.click();
  await expect(again).toHaveURL(new RegExp(`#/run/${run}`));
  await script(request, host, "release");
  await expect(again.locator(".tag", { hasText: "DONE" })).toBeVisible();
  await expect(again.getByRole("tab", { name: "Result" })).toBeVisible();
  await again.close();
});
