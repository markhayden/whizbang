// Reports (plan m5, task W3; SPEC.md section 7, "Reports for both tools shown in place and
// still readable after a restart; squint's exported as a document" and squint's "Report
// viewer and export", rows 7 and 11 of the plan's acceptance table).
import { readFileSync } from "node:fs";
import { expect, test } from "./fixtures";
import { openReport, picturesDecoded, realityCheck, removal, withKeptHost } from "./kept";
import { file, give, openAt, runWhenReady, script } from "./play";

test.afterEach(async ({ request, host }) => {
  await script(request, host, "reset");
});

test("a rascal result's report shows its steps, masks and cost in place, its pictures decoded and its styles applied", async ({ page, host }) => {
  const run = await removal(page, host, "report-porch.png", 41);
  await page.locator(".run__report").click();
  const frame = page.frameLocator("iframe.isolated-frame");
  await expect(frame.locator("h1")).toHaveText("Remove what you name");
  await expect(frame.locator("h2", { hasText: "Masks over the picture" })).toBeVisible();
  await expect(frame.locator("h2", { hasText: "Steps" })).toBeVisible();
  await expect(frame.locator("td", { hasText: /^segment/ }).first()).toBeVisible();
  await expect(frame.locator("td", { hasText: /^fill/ }).first()).toBeVisible();
  await expect(frame.locator("p", { hasText: "Settled in all" })).toContainText("$0.0400");
  expect(await picturesDecoded(frame)).toBeGreaterThanOrEqual(3);
  // Its own style sheet applied: headings in capitals on its dark ground.
  const look = await frame.locator("h2").first().evaluate((h2) => ({ caps: getComputedStyle(h2).textTransform, ground: getComputedStyle(document.body).backgroundColor }));
  expect(look).toEqual({ caps: "uppercase", ground: "rgb(15, 17, 20)" });
  await expect(page.locator(".report__run")).toHaveText(run);
});

test("a squint result's report shows its verdict, findings and the places on the picture in place", async ({ page, request, host }) => {
  await script(request, host, "fail,placed");
  const run = await realityCheck(page, host, "report-attic.png", 42, "FAIL");
  const frame = await openReport(page, host, run);
  await expect(frame.locator(".verdict")).toHaveText("FAIL");
  await expect(frame.locator("h2", { hasText: "Findings" })).toBeVisible();
  await expect(frame.locator("td", { hasText: "violation" }).first()).toBeVisible();
  await expect(frame.locator("figcaption", { hasText: "1 place the judge found" })).toBeVisible();
  await expect(frame.locator("svg rect")).toHaveCount(1);
  await expect(frame.locator(".turn")).toContainText("calls submit_verdict");
  expect(await picturesDecoded(frame)).toBeGreaterThanOrEqual(1);
});

test("both reports read the same after a restart", async ({ page }) => {
  await withKeptHost(async (kept) => {
    const removed = await removal(page, kept.host, "again-porch.png", 43);
    const checked = await realityCheck(page, kept.host, "again-kitchen.png", 44, "PASS");
    const read = async (run: string) => (await openReport(page, kept.host, run)).locator("body").innerText();
    const before = [await read(removed), await read(checked)];
    await kept.restart();
    expect([await read(removed), await read(checked)]).toEqual(before);
  });
});

test("export of a squint report downloads one document that opens alone, offline, and holds its pictures", async ({ page, context, request, host }) => {
  await script(request, host, "fail");
  const run = await realityCheck(page, host, "export-attic.png", 45, "FAIL");
  await openReport(page, host, run);
  const [download] = await Promise.all([page.waitForEvent("download"), page.getByRole("button", { name: "Export", exact: true }).click()]);
  expect(download.suggestedFilename()).toBe(`reality-checks-${run}.html`);
  const saved = test.info().outputPath(download.suggestedFilename());
  await download.saveAs(saved);
  const html = readFileSync(saved, "utf8");
  expect(html).not.toMatch(/https?:\/\//);
  expect(html).not.toMatch(/<script/i);
  // Opened alone, with the network off: it asks for nothing, and its pictures are there.
  const alone = await context.newPage();
  const asked: string[] = [];
  alone.on("request", (one) => {
    if (!one.url().startsWith("file:") && !one.url().startsWith("data:")) asked.push(one.url());
  });
  await context.setOffline(true);
  try {
    await alone.goto(`file://${saved}`);
    await expect(alone.locator(".verdict")).toHaveText("FAIL");
    const decoded = await alone.locator("img").evaluateAll((all) => all.map((img) => (img as HTMLImageElement).naturalWidth > 0));
    expect(decoded.length).toBeGreaterThan(0);
    expect(decoded.every(Boolean)).toBe(true);
    expect(asked).toEqual([]);
  } finally {
    await context.setOffline(false);
    await alone.close();
  }
  // The run's archive is a download too.
  const [archive] = await Promise.all([page.waitForEvent("download"), page.getByRole("button", { name: "Export run" }).click()]);
  expect(archive.suggestedFilename()).toBe(`reality-checks-${run}.tar`);
});

test("model and tool text holding markup shows as text and runs nothing", async ({ page, request, host }) => {
  await script(request, host, "markup");
  await openAt(page, host, "#/k/squint/brief-compliance");
  await give(page, "candidate", file("markup.png", 46));
  await page.locator("#field-brief").fill(`<img src=x onerror="document.title='ran'"> a kitchen`);
  await runWhenReady(page);
  await expect(page.locator(".workspace__title-row .tag", { hasText: "PASS" })).toBeVisible();
  await page.locator(".run__report").click();
  const frame = page.frameLocator("iframe.isolated-frame");
  await expect(frame.locator("h2", { hasText: "Reasons" })).toBeVisible();
  await expect(frame.locator("li").first()).toContainText(`<img src=x onerror="document.title='ran'"><script>document.title='ran'</script> held`);
  await expect(frame.locator("body")).toContainText(`<img src=x onerror=`);
  // The markup made no element, in the frame or the page, and ran nothing.
  expect(await frame.locator("img[onerror], script").count()).toBe(0);
  expect(await page.locator("img[onerror], .screen script").count()).toBe(0);
  expect(await page.title()).not.toBe("ran");
});

test("the report frame cannot read the page's storage or call the api", async ({ page, host }) => {
  const run = await removal(page, host, "frame-porch.png", 47);
  await openReport(page, host, run);
  const iframe = page.locator("iframe.isolated-frame");
  // Sandboxed with nothing allowed: no scripts, and no origin of the page's.
  expect(await iframe.getAttribute("sandbox")).toBe("");
  expect(await page.evaluate(() => typeof sessionStorage.getItem("whizbang.token") === "string" || sessionStorage.length > 0)).toBe(true);
  const frame = page.frames().find((one) => one !== page.mainFrame());
  if (!frame) throw new Error("no frame");
  // What its document would reach, tried from inside it.
  const tried = await frame.evaluate(async () => {
    const said: Record<string, string> = { origin: String(self.origin) };
    try {
      said.storage = String(sessionStorage.length);
    } catch {
      said.storage = "refused";
    }
    try {
      said.parent = String(window.parent.sessionStorage.length);
    } catch {
      said.parent = "refused";
    }
    try {
      const answer = await fetch("/api/hello");
      said.api = String(answer.status);
    } catch {
      said.api = "refused";
    }
    return said;
  });
  expect(tried.origin).toBe("null");
  expect(tried.storage).toBe("refused");
  expect(tried.parent).toBe("refused");
  expect(tried.api).not.toBe("200");
  // And nothing it holds asked the host for anything.
  await expect(page.frameLocator("iframe.isolated-frame").locator("h1")).toBeVisible();
});
