// Before and after (plan m5, task W2; SPEC.md section 7, row 3 of the plan's acceptance
// table): a result's source and its picture side by side, swiped and as their difference.
import { expect, test } from "./fixtures";
import { file, give, openAt, picture, runWhenReady, script } from "./play";

test.afterEach(async ({ request, host }) => {
  await script(request, host, "reset");
});

test("side by side, swipe and difference show the source and the result at one zoom", async ({ page, host }) => {
  await openAt(page, host, "#/k/rascal/remove_prompt");
  await give(page, "image", file("compare.png", 31));
  await page.locator("#field-prompt").fill("the hose");
  await runWhenReady(page);
  await expect(page.locator(".workspace__title-row .tag", { hasText: "DONE" })).toBeVisible();
  await page.getByRole("tab", { name: "Compare" }).click();
  // Side by side: the two pictures, each at the same size, so one zoom for both.
  const figures = page.locator(".compare__side .compare__figure");
  await expect(figures).toHaveCount(2);
  await expect(figures.nth(0).locator("img")).toHaveAttribute("alt", "Before");
  await expect(figures.nth(1).locator("img")).toHaveAttribute("alt", "After");
  const sizes = await figures.evaluateAll((all) => all.map((one) => `${(one as HTMLElement).style.width}|${(one as HTMLElement).style.height}`));
  expect(sizes[0]).toBe(sizes[1]);
  // Swipe: the result under the source, cut where the handle is.
  await page.getByRole("tab", { name: "Swipe" }).click();
  const swipe = page.locator(".compare__swipe");
  await expect(swipe.locator("img")).toHaveCount(2);
  await swipe.getByRole("slider").fill("20");
  await expect(swipe.locator(".compare__top")).toHaveAttribute("style", /inset\(0px 80% 0px 0px\)|inset\(0 80% 0 0\)/);
  // Difference: drawn from the two pictures, at the same size, and not blank (the fill
  // changed the picture).
  await page.getByRole("tab", { name: "Difference" }).click();
  const difference = page.locator(".compare__difference");
  await expect(difference).toBeVisible();
  await expect
    .poll(() =>
      difference.evaluate((canvas) => {
        const drawn = (canvas as HTMLCanvasElement).getContext("2d")!.getImageData(0, 0, (canvas as HTMLCanvasElement).width, (canvas as HTMLCanvasElement).height).data;
        let lit = 0;
        for (let i = 0; i < drawn.length; i += 4) if (drawn[i]! + drawn[i + 1]! + drawn[i + 2]! > 0) lit += 1;
        return lit;
      }),
    )
    .toBeGreaterThan(0);
  const box = await difference.boundingBox();
  expect(`${box?.width}px`).toBe(sizes[0]!.split("|")[0]);
});

test("two pictures wider than half the canvas both fit side by side, at one zoom", async ({ page, host }) => {
  await openAt(page, host, "#/k/rascal/remove_prompt");
  await page.locator(".field:has(#field-image) input[type=file]").setInputFiles({ name: "large.png", mimeType: "image/png", buffer: picture(32, 640, 480) });
  await page.locator("#field-prompt").fill("the hose");
  await runWhenReady(page);
  await expect(page.locator(".workspace__title-row .tag", { hasText: "DONE" })).toBeVisible();
  await page.getByRole("tab", { name: "Compare" }).click();
  const figures = page.locator(".compare__side .compare__figure");
  await expect(figures).toHaveCount(2);
  // The zoom follows the stage as it is measured, so the stage and the two pictures are
  // read in one look, in the page, and looked at again until both pictures are decoded and
  // laid out at one zoom inside the stage. Read one box at a time, the first was once taken
  // at a zoom the next layout changed (397 and 479 pixels wide, run 45027669412). The look
  // says which part does not hold yet, and a failure names it. Polled, never timed.
  await expect
    .poll(() =>
      page.locator(".compare__stage").evaluate((stage) => {
        const figures = [...stage.querySelectorAll(".compare__side .compare__figure")];
        const images = figures.map((figure) => figure.querySelector("img"));
        if (figures.length !== 2) return ["two pictures"];
        const room = stage.getBoundingClientRect();
        const [before, after] = figures.map((figure) => figure.getBoundingClientRect()) as [DOMRect, DOMRect];
        const unmet: string[] = [];
        if (!images.every((img) => img !== null && img.complete && img.naturalWidth > 0)) unmet.push("both decoded");
        if (before.width !== after.width) unmet.push(`one width (${before.width} and ${after.width})`);
        if (before.width <= 100) unmet.push(`wider than 100 (${before.width})`);
        if (after.right > room.right + 0.5) unmet.push(`inside the stage's width (${after.right} past ${room.right})`);
        if (before.bottom > room.bottom + 0.5) unmet.push(`inside the stage's height (${before.bottom} past ${room.bottom})`);
        return unmet;
      }),
    )
    .toEqual([]);
});
