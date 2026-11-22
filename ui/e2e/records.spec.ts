// A tool's or task's record in the playground (SPEC.md section 8: "the playground shows the
// same page"; plan m5, task W3, item 7).
import { expect, test } from "./fixtures";
import { picturesDecoded } from "./kept";
import { openAt } from "./play";

test("a task's record opens from the workspace with its examples and limits, its pictures decoded and its styles applied", async ({ page, host }) => {
  await openAt(page, host, "#/k/rascal/remove_prompt");
  await page.getByRole("link", { name: "Read this task's record" }).click();
  await expect(page).toHaveURL(/#\/record\/remove_prompt$/);
  const frame = page.frameLocator("iframe.isolated-frame");
  await expect(frame.locator("h1")).toHaveText("remove_prompt");
  await expect(frame.locator("h2", { hasText: "Examples" })).toBeVisible();
  await expect(frame.locator("li", { hasText: "Do not use it" })).toBeVisible();
  expect(await picturesDecoded(frame)).toBeGreaterThan(0);
  // Its pictures were made data addresses by the page, and its own style applied.
  const sources = await frame.locator("img").evaluateAll((all) => all.map((img) => (img as HTMLImageElement).src.slice(0, 22)));
  expect(new Set(sources)).toEqual(new Set(["data:image/png;base64,"]));
  expect(await frame.locator("pre").first().evaluate((pre) => getComputedStyle(pre).whiteSpace)).toBe("pre-wrap");
  // A tool the task uses links to its own record.
  await page.goto(`${host.address}#/k/rascal/remove_prompt`);
  await page.locator(".workspace__tools a", { hasText: "segment" }).click();
  await expect(page.frameLocator("iframe.isolated-frame").locator("h1")).toHaveText("segment");
});
