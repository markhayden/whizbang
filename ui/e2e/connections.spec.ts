// Of Chrome's six connections to the host, the page's thumbnails and uploads take at most
// two each, so polls, Stop and Quit always find one (plan m5, "Who may call"; the review of
// task W1).
import { expect, test } from "./fixtures";
import { file, give, openAt, picture, runWhenReady, script } from "./play";

test.afterEach(async ({ request, host }) => {
  await script(request, host, "reset");
});

test("thumbnails and uploads leave at least four of Chrome's six connections for polls, stop and quit", async ({ page, request, host }) => {
  // A library of pictures, sent as the page sends them.
  for (let seed = 200; seed < 212; seed += 1) {
    const sent = await request.post(`${host.address}api/pictures?name=lib-${seed}.png`, {
      headers: { Authorization: `Bearer ${host.token}`, Origin: host.origin, "Content-Type": "image/png" },
      data: picture(seed),
    });
    expect(sent.status()).toBe(201);
  }
  // A run held at its first request, followed in one tab.
  await script(request, host, "hold");
  await openAt(page, host, "#/k/rascal/remove_prompt");
  await give(page, "image", file("held.png", 199));
  await page.locator("#field-prompt").fill("the hose");
  await runWhenReady(page);
  await expect(page.locator('.step-list [data-state="running"]').first()).toBeVisible();

  // Another tab of the same browser loads every thumbnail and sends pictures at once.
  const other = await page.context().newPage();
  let artifacts = 0;
  let mostArtifacts = 0;
  let uploads = 0;
  let mostUploads = 0;
  const kind = (url: string, method: string) => (url.includes("/api/artifacts/") ? "artifact" : method === "POST" && url.includes("/api/pictures") ? "upload" : null);
  other.on("request", (sent) => {
    const which = kind(sent.url(), sent.method());
    if (which === "artifact") mostArtifacts = Math.max(mostArtifacts, (artifacts += 1));
    if (which === "upload") mostUploads = Math.max(mostUploads, (uploads += 1));
  });
  const done = (sent: { url(): string; method(): string }) => {
    const which = kind(sent.url(), sent.method());
    if (which === "artifact") artifacts -= 1;
    if (which === "upload") uploads -= 1;
  };
  other.on("requestfinished", done);
  other.on("requestfailed", done);
  await openAt(other, host, "#/");
  await other.locator(".home__pictures input[type=file]").setInputFiles(Array.from({ length: 6 }, (_, at) => file(`drop-${at}.png`, 220 + at)));

  // Meanwhile the first tab still polls, and Stop is answered.
  await page.getByRole("button", { name: "Stop run" }).click();
  await expect(page.locator(".tag", { hasText: "CANCELLED" })).toBeVisible();
  await expect(other.locator(".home__pictures img[src^=\"blob:\"]")).toHaveCount(13 + 6, { timeout: 15_000 });
  expect(mostArtifacts).toBeLessThanOrEqual(2);
  expect(mostUploads).toBeLessThanOrEqual(2);
  await other.close();
});
