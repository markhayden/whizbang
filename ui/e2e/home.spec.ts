// A kit the interface has never heard of shows wherever kits do (SPEC.md section 1: a new
// kit needs no change to a door; plan m5, task W1, item 2).
import { expect, test, withHost } from "./fixtures";

test("a third kit of fake actions shows on home, in the switcher and in search", async ({ page }) => {
  await withHost(["--third-kit"], async (host) => {
    await page.goto(host.opening);
    await expect(page.getByRole("status")).toContainText("ready");
    const panel = page.locator(".kit-panel", { has: page.getByRole("heading", { name: /tinker/i }) });
    await expect(panel).toContainText("Echo a picture");
    await panel.getByRole("link", { name: /Echo a picture/ }).click();
    await expect(page.getByRole("heading", { name: "Echo a picture" })).toBeVisible();
    await page.locator(".breadcrumb__kit").click();
    await expect(page.getByRole("menu", { name: "Kits" }).getByRole("menuitem", { name: /tinker/i })).toBeVisible();
    await page.goto(`${host.address}#/`);
    await page.keyboard.press("ControlOrMeta+k");
    await page.getByLabel("Search tasks, tools and pictures").fill("Echo");
    await expect(page.getByRole("option", { name: /Echo a picture/ })).toBeVisible();
  });
});
