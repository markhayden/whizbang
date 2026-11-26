// A kit the interface has never heard of shows wherever kits do (SPEC.md section 1: a new
// kit needs no change to a door; plan m5, task W1, item 2).
import { execFileSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { BINARY, expect, startHost, stopHost, test, withHost } from "./fixtures";
import { picture, READY } from "./play";

test("a third kit of fake actions shows on home, in the switcher and in search", async ({ page }) => {
  await withHost(["--third-kit"], async (host) => {
    await page.goto(host.opening);
    await expect(page.getByRole("status")).toContainText(READY);
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

// Home's library (plan m7, task U1, item 3; the carry-over row "Home's library lists only
// the pictures given through the playground").
test("home lists the pictures of a store filled from the command line", async ({ page }) => {
  const store = mkdtempSync(join(tmpdir(), "whizbang-e2e-door-"));
  const binary = resolve(BINARY);
  const files = mkdtempSync(join(tmpdir(), "whizbang-e2e-files-"));
  try {
    const cli = (...words: string[]) => execFileSync(binary, ["cli", ...words], { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] });
    const identity = (said: string) => /sha256:[0-9a-f]{64}/.exec(said)?.[0] ?? "";
    writeFileSync(join(files, "ran.png"), picture(81));
    writeFileSync(join(files, "kept.png"), picture(82));
    const ran = identity(cli("store", "import", "--picture", join(files, "ran.png"), "--store", store, "--create"));
    const kept = identity(cli("store", "import", "--picture", join(files, "kept.png"), "--store", store));
    expect(ran).not.toBe("");
    expect(kept).not.toBe("");
    cli("spend", "reset", "--allowance", "10", "--store", store);
    const under = ["--config", `${store}.fixture.toml`, "--env-file", `${store}.fixture.env`, "--models-dir", `${store}.fixture.models`];
    cli("rascal", "remove-prompt", "--image", ran, "--prompt", "the hose", "--store", store, ...under);
    const { host, child } = await startHost(store);
    try {
      await page.goto(host.opening);
      await expect(page.getByRole("status")).toContainText(READY);
      const tiles = page.locator(".home__pictures .home__tile");
      await expect(tiles).toHaveCount(2);
      // No name a person gave: each is shown by its identity, with its runs or its import.
      const taken = page.locator(`.home__tile[data-picture="${ran}"]`);
      await expect(taken.locator(".home__tile-line")).toHaveText("1 run");
      await expect(page.locator(`.home__tile[data-picture="${kept}"] .home__tile-line`)).toHaveText("imported · not run yet");
      await expect(page.getByRole("tab", { name: /Inputs of runs/ })).toContainText("1");
      // Its card: no name, first used, and the run that took it.
      await taken.locator(".picture-tile").click();
      const card = page.getByRole("region", { name: /Start a task with sha256:/ });
      await expect(card).toContainText("No name given");
      await expect(card).toContainText("First used");
      await expect(card).toContainText("INPUT OF 1 RUN");
      await expect(card.locator(".picked__run")).toContainText("Remove what you name");
      // A field's library is Home's (N1's pass, finding 1): both pictures, though neither
      // was given through the playground.
      await page.goto(`${host.address}#/k/rascal/remove_prompt`);
      await page.locator(".workspace__picture").getByRole("button", { name: "From the library" }).first().click();
      const library = page.locator(".workspace__picture .library .picture-tile");
      await expect(library).toHaveCount(2);
      await library.first().click();
      await expect(page.locator(".workspace__picture .picture-field__chosen")).toHaveCount(1);
      await expect(library).toHaveCount(0);
    } finally {
      await stopHost(child);
    }
  } finally {
    rmSync(store, { recursive: true, force: true });
    rmSync(files, { recursive: true, force: true });
  }
});

// The shell in Chrome (plan m5, task U1, item 7), moved here from shell.spec.ts so that
// the cases of the Chrome tier's two shards take about one time (plan mE, task P0).

test("the page loads with the token, removes it from the address and shows Home", async ({ page, host }) => {
  const hello = page.waitForResponse((response) => response.url() === `${host.address}api/hello`);
  await page.goto(host.opening);
  expect((await hello).status()).toBe(200);
  await expect(page.getByRole("heading", { name: /Home/ })).toBeVisible();
  await expect(page.getByRole("status")).toContainText(READY);
  expect(page.url()).toBe(`${host.address}#/`);
  expect(page.url()).not.toContain(host.token);
  // The token is kept for this tab: a reload of the same tab, without it in the address,
  // still works; another tab is not given it.
  await page.reload();
  await expect(page.getByRole("status")).toContainText(READY);
  await expect(page.getByRole("alert")).toHaveCount(0);
  const other = await page.context().newPage();
  await other.goto(host.address);
  await expect(other.getByRole("alert")).toContainText("Run whizbang play again");
  await other.close();
});

test("a request from the page without the token is refused and the page says to run whizbang play again", async ({ page, host, otherOrigin }) => {
  const refused = page.waitForResponse((response) => response.url() === `${host.address}api/hello`);
  await page.goto(host.address);
  expect((await refused).status()).toBe(401);
  await expect(page.getByRole("alert")).toContainText("Run whizbang play again");
  // A token of an earlier launch is refused the same way.
  const stale = page.waitForResponse((response) => response.url() === `${host.address}api/hello`);
  await page.goto(`${host.address}#k=${"0".repeat(64)}`);
  await page.reload();
  expect((await stale).status()).toBe(401);
  await expect(page.getByRole("alert")).toContainText("Run whizbang play again");
  // Another page may send Chrome to the page (a top-level navigation is let through to the
  // static files), at an address it makes: the page opens, acts on nothing in it, and asks
  // for nothing but hello.
  await page.goto(otherOrigin.address);
  const changes: string[] = [];
  page.on("request", (request) => {
    if (request.method() !== "GET") changes.push(`${request.method()} ${request.url()}`);
  });
  const arrived = page.waitForResponse((response) => response.url() === `${host.address}api/hello`);
  await page.evaluate((to) => {
    window.location.href = to;
  }, `${host.address}#/k/rascal/remove_checked?start=1`);
  expect((await arrived).status()).toBe(401);
  await expect(page.getByRole("alert")).toContainText("Run whizbang play again");
  expect(changes).toEqual([]);
});

test("the page loads nothing from any origin but its host", async ({ page, host }) => {
  const asked: string[] = [];
  page.on("request", (request) => asked.push(request.url()));
  const response = await page.goto(host.opening);
  expect(response?.headers()["content-security-policy"]).toContain("default-src 'self'");
  await expect(page.getByRole("status")).toContainText(READY);
  await page.evaluate(async () => {
    await document.fonts.ready;
  });
  const fonts = await page.evaluate(() => [...document.fonts].filter((font) => font.status === "loaded").map((font) => font.family));
  expect(fonts.join(" ")).toContain("Geist");
  expect(asked.length).toBeGreaterThan(2);
  const elsewhere = asked.filter((url) => !url.startsWith(`${host.origin}/`) && !url.startsWith("data:") && !url.startsWith("blob:"));
  expect(elsewhere).toEqual([]);
});
