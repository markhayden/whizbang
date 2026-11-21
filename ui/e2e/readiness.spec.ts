// Readiness per choice and the bound of a paid run, before Run (plan m5, task W1, items 4
// and 5; row 1 of the acceptance table).
import { expect, test } from "./fixtures";
import { file, give, openAt, runWhenReady, script } from "./play";

test.afterEach(async ({ request, host }) => {
  await script(request, host, "reset");
});

test("each choice shows ready, no key or get with its size, and a choice that is not ready can be picked and Run says what is missing", async ({ page, host }) => {
  // A segmenter of the catalogue that is not installed: GET with its size.
  await openAt(page, host, "#/k/rascal/remove_prompt");
  await give(page, "image", file("ready.png", 90));
  await page.locator("#field-prompt").fill("the hose");
  const segmenter = page.locator('.choice-list:has(input[name="field-backend"])');
  await expect(segmenter.locator(".choice-list__row", { hasText: "local-seg" })).toContainText("READY");
  await expect(segmenter.locator(".choice-list__row", { hasText: "sam3" })).toContainText(/GET · \d/);
  await expect(page.locator(".workspace__run .button--primary")).toBeEnabled();
  await segmenter.locator(".choice-list__row", { hasText: "sam3" }).click();
  await expect(page.locator(".workspace__run .button--primary")).toBeDisabled();
  await expect(page.locator(".workspace__run .button__why")).toContainText("segment");
  // A judge whose key is not set: NO KEY, and Run says so once it is picked.
  await page.goto(`${host.address}#/k/squint/reality-checks`);
  await give(page, "candidate", file("judged.png", 92));
  const judge = page.locator('.choice-list:has(input[name="field-judge"])');
  await expect(judge.locator(".choice-list__row", { hasText: "cloud-judge" })).toContainText("READY");
  await expect(judge.locator(".choice-list__row", { hasText: "keyless-judge" })).toContainText("NO KEY");
  await expect(page.locator(".workspace__run .button--primary")).toBeEnabled();
  await judge.locator(".choice-list__row", { hasText: "keyless-judge" }).click();
  await expect(page.locator(".workspace__run .button--primary")).toBeDisabled();
  await expect(page.locator(".workspace__run .button__why")).toContainText("judge");
});

test("a paid run shows each need's bound and the total before Run, and Run is refused once the preparation is stale", async ({ page, host }) => {
  await openAt(page, host, "#/k/rascal/remove_prompt");
  await give(page, "image", file("paid.png", 91));
  await page.locator("#field-prompt").fill("the hose");
  const plan = page.getByRole("region", { name: "Plan" });
  await expect(plan).toContainText("fill");
  await expect(plan).toContainText("$0.040");
  await expect(plan).toContainText("up to $");
  await expect(page.locator(".workspace__bound")).toContainText("up to $");
  // The preparation the page holds goes stale before Run: the start carries a handle the
  // host no longer holds, and the host refuses it before anything is admitted.
  await page.route("**/api/runs", async (route) => {
    const body = JSON.parse(route.request().postData() ?? "{}") as Record<string, unknown>;
    await route.continue({ postData: JSON.stringify({ ...body, handle: "f".repeat(32) }) });
  });
  const refused = page.waitForResponse((response) => response.url().endsWith("/api/runs") && response.request().method() === "POST");
  await page.locator(".workspace__run .button--primary").click();
  expect((await refused).status()).toBe(409);
  await expect(page.getByRole("alert")).toContainText("prepared again");
  await expect(page).not.toHaveURL(/#\/run\//);
  await page.unroute("**/api/runs");
  await runWhenReady(page);
  await expect(page.locator(".tag", { hasText: "DONE" })).toBeVisible();
});
