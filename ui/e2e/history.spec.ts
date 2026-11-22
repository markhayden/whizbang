// History (plan m5, task W3; SPEC.md section 7, "History that survives a restart; evidence
// that survives a reopen", row 6 of the plan's acceptance table).
import { expect, test } from "./fixtures";
import { realityCheck, removal, withKeptHost } from "./kept";
import { openAt, script } from "./play";

test("runs are listed after the host is stopped and started again, with their result, cost and time", async ({ page }) => {
  await withKeptHost(async (kept) => {
    const removed = await removal(page, kept.host, "porch.png", 31);
    const checked = await realityCheck(page, kept.host, "kitchen.png", 32, "PASS");
    const host = await kept.restart();
    await openAt(page, host, "#/history");
    const rows = page.locator(".history__list [data-run]");
    await expect(rows).toHaveCount(2);
    await expect(rows.nth(0)).toHaveAttribute("data-run", checked);
    await expect(rows.nth(1)).toHaveAttribute("data-run", removed);
    await expect(rows.nth(1)).toContainText("Remove what you name");
    await expect(rows.nth(1)).toContainText("the hose · porch.png");
    await expect(rows.nth(1)).toContainText("DONE");
    await expect(rows.nth(1).locator(".history-row__cost")).toHaveText("$0.040");
    await expect(rows.nth(1).locator(".history-row__time")).toHaveText(/\d+\.\d s/);
    await expect(rows.nth(0)).toContainText("PASS");
    await expect(rows.nth(0).locator(".history-row__cost")).toHaveText("$0.002");
    // Opening a row opens its result.
    await rows.nth(1).locator(".history-row").click();
    await expect(page).toHaveURL(new RegExp(`#/run/${removed}$`));
  });
});

test("filters by kit, result and words narrow the list and the figures", async ({ page, request }) => {
  await withKeptHost(async (kept) => {
    const host = kept.host;
    await removal(page, host, "garden.png", 33, "the garden hose");
    await realityCheck(page, host, "hall.png", 34, "PASS");
    await script(request, host, "fail");
    const failed = await realityCheck(page, host, "attic.png", 35, "FAIL");
    await script(request, host, "reset");
    await openAt(page, host, "#/history");
    const rows = page.locator(".history__list [data-run]");
    const figure = (name: string) => page.locator(".history__figure", { hasText: name }).locator("dd");
    await expect(rows).toHaveCount(3);
    const all = Number(await figure("runs").first().textContent());
    await page.getByRole("tab", { name: "Squint" }).click();
    await expect(page).toHaveURL(/#\/history\?kit=squint$/);
    await expect(rows).toHaveCount(2);
    await expect.poll(async () => Number(await figure("runs").first().textContent())).toBeLessThan(all);
    await page.locator(".history__result select").selectOption("fail");
    await expect(rows).toHaveCount(1);
    await expect(rows.first()).toHaveAttribute("data-run", failed);
    await expect(figure("verdicts")).toHaveText(/pass 0 · fail \d+ · unsure 0/);
    await page.getByRole("tab", { name: "All kits" }).click();
    await page.locator(".history__result select").selectOption("");
    await page.locator("#history-words").fill("GARDEN");
    await expect(rows).toHaveCount(1);
    await expect(rows.first()).toContainText("the garden hose");
    await expect(figure("verdicts")).toHaveText(/^0 · /);
  });
});

test("a finished run's evidence (steps, findings, places, pictures) is the same after a restart", async ({ page, request }) => {
  await withKeptHost(async (kept) => {
    await script(request, kept.host, "fail,placed");
    const run = await realityCheck(page, kept.host, "evidence.png", 36, "FAIL");
    await script(request, kept.host, "reset");
    const read = async () => {
      const frame = page.frameLocator("iframe.isolated-frame");
      await expect(frame.locator("h2", { hasText: "Steps" })).toBeVisible();
      return {
        text: await frame.locator("body").innerText(),
        places: await frame.locator("svg rect").count(),
        pictures: await frame.locator("img").evaluateAll((all) => all.map((img) => (img as HTMLImageElement).src)),
      };
    };
    await page.goto(`${kept.host.address}#/run/${run}/report`);
    const before = await read();
    expect(before.places).toBe(1);
    expect(before.text.toLowerCase()).toContain("findings");
    expect(before.pictures.length).toBeGreaterThan(0);
    const host = await kept.restart();
    await openAt(page, host, `#/run/${run}`);
    await expect(page.locator(".workspace__title-row .tag", { hasText: "FAIL" })).toBeVisible();
    await page.locator(".run__report").click();
    expect(await read()).toEqual(before);
  });
});
