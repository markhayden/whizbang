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
    } finally {
      await stopHost(child);
    }
  } finally {
    rmSync(store, { recursive: true, force: true });
    rmSync(files, { recursive: true, force: true });
  }
});
