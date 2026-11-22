// The Models screen (plan m5, task M1; SPEC.md section 7, "Model installation from the
// interface"): Get with its progress, a second Get attached, Cancel, Remove of a model in
// use, the keys never shown, and the owner's confirmed reset of the allowance. The model
// installed is the fixture's `pocket`, whose files a fake download serves from memory; no
// real model is ever fetched. Each case runs a host of its own, so no case sees another's
// install.
import type { APIRequestContext, Page } from "@playwright/test";
import { expect, test, withHost, type Host } from "./fixtures";
import { openAt, script } from "./play";

/** The fixture's made-up key of `cloud-fill` (crates/whizbang/examples/play_fixture.rs). */
const KEY = ["made", "up", "fixture", "key", "0123456789"].join("-");

const row = (page: Page, name: string) => page.locator(".models__panel .model-row", { has: page.locator(".model-row__name", { hasText: new RegExp(`^${name}$`) }) });

async function post(request: APIRequestContext, host: Host, path: string) {
  return request.post(`${host.address}${path}`, { headers: { Authorization: `Bearer ${host.token}`, Origin: host.origin } });
}

test("get installs a model with its progress, and a second get attaches to the same install", async ({ page, request }) => {
  await withHost([], async (host) => {
    await script(request, host, "hold-install");
    await openAt(page, host, "#/models");
    const pocket = row(page, "pocket");
    await expect(pocket).toContainText("Get");
    await pocket.getByRole("button", { name: "Get" }).click();
    // Held after its first file: the progress counts files, and says so.
    await expect(pocket).toContainText("1 of 3 files");
    await expect(pocket).toContainText("counted in files, not bytes");
    await expect(pocket.getByRole("button", { name: "Cancel" })).toBeVisible();
    const second = await post(request, host, "api/models/pocket/get");
    expect(second.status()).toBe(200);
    expect(await second.json()).toMatchObject({ attached: true });
    await script(request, host, "release-install");
    await expect(pocket).toContainText("READY");
    await expect(pocket.getByRole("button", { name: "Remove" })).toBeVisible();
  });
});

test("cancel leaves the earlier install in place", async ({ page, request }) => {
  await withHost([], async (host) => {
    await script(request, host, "hold-install");
    await openAt(page, host, "#/models");
    const pocket = row(page, "pocket");
    await pocket.getByRole("button", { name: "Get" }).click();
    await expect(pocket).toContainText("1 of 3 files");
    await pocket.getByRole("button", { name: "Cancel" }).click();
    // Before the install there was none: after the cancel there is none, and Get again.
    await expect(pocket).toContainText("the install was cancelled; what was installed before is as it was");
    await expect(pocket.getByRole("button", { name: "Get" })).toBeVisible();
    const listed = await (await request.get(`${host.address}api/models`, { headers: { Authorization: `Bearer ${host.token}` } })).json();
    expect(listed.models.find((model: { name: string }) => model.name === "pocket")).toMatchObject({ state: "not_installed", files_done: null });
    await script(request, host, "release-install");
  });
});

test("remove of a model in use answers busy and the model stays", async ({ page, request }) => {
  await withHost([], async (host) => {
    await openAt(page, host, "#/models");
    const pocket = row(page, "pocket");
    await pocket.getByRole("button", { name: "Get" }).click();
    await expect(pocket).toContainText("READY");
    await script(request, host, "use-model");
    await pocket.getByRole("button", { name: "Remove" }).click();
    await expect(page.locator(".models__problem")).toContainText("not removed");
    await expect(pocket).toContainText("READY");
    await script(request, host, "let-model-go");
    await pocket.getByRole("button", { name: "Remove" }).click();
    await expect(pocket.getByRole("button", { name: "Get" })).toBeVisible();
  });
});

test("no key is shown for a backend without one and no key is ever in the page", async ({ page, request }) => {
  await withHost([], async (host) => {
    const answers: string[] = [];
    page.on("response", async (response) => {
      try {
        answers.push(await response.text());
      } catch {
        // A response with no body to read (a redirect) holds no key.
      }
    });
    await openAt(page, host, "#/models");
    const cloud = page.locator(".models__panel", { has: page.getByRole("heading", { name: "In the cloud" }) });
    await expect(row(page, "cloud-fill")).toContainText("KEY SET");
    await expect(row(page, "keyless-judge")).toContainText("NO KEY");
    await expect(row(page, "cloud-judge")).toContainText("READY");
    // The sign-in SAM 3's download needs is not named in the fixture's config: no Get.
    await expect(row(page, "sam3")).toContainText("NOT SET UP");
    await expect(row(page, "sam3").getByRole("button", { name: "Get" })).toHaveCount(0);
    await row(page, "keyless-judge").getByRole("button", { name: "How to add" }).click();
    await expect(page.locator(".models__help")).toContainText("env file");
    // Nothing on the page takes a key, and nothing it shows or was sent holds one.
    await expect(cloud.locator("input")).toHaveCount(0);
    await expect(page.locator(".models__help input")).toHaveCount(0);
    const shown = await page.content();
    expect(shown).not.toContain(KEY);
    expect(shown).not.toContain("PLAYKIT_");
    for (const path of ["api/hello", "api/models", "api/backends", "api/spend"]) {
      const answer = await request.get(`${host.address}${path}`, { headers: { Authorization: `Bearer ${host.token}` } });
      answers.push(await answer.text());
    }
    for (const text of answers) {
      expect(text).not.toContain(KEY);
      expect(text).not.toContain(host.token);
    }
  });
});

test("a reset needs the amount and the word reset, and the allowance is then what is spent since it", async ({ page }) => {
  await withHost([], async (host) => {
    await openAt(page, host, "#/models");
    const allowance = page.locator(".models__allowance");
    await expect(allowance).toContainText("left of $10.00");
    await expect(allowance.locator(".models__chip")).toContainText("Per run");
    await allowance.getByRole("button", { name: "Reset…" }).click();
    await expect(allowance).toContainText("A reset closes the books");
    const confirm = allowance.locator(".models__reset .button--danger");
    await page.getByLabel("New allowance, in US dollars").fill("20");
    await expect(confirm).toBeDisabled();
    await page.getByLabel("Type reset to confirm").fill("Reset");
    await expect(confirm).toBeDisabled();
    await page.getByLabel("Type reset to confirm").fill("reset");
    await expect(confirm).toBeEnabled();
    await confirm.click();
    await expect(allowance).toContainText("The books are closed: the allowance is $20.00 from now on.");
    const today = new Date().toISOString().slice(0, 10);
    await expect(allowance).toContainText(`$0.000 spent since ${today}`);
    await expect(allowance).toContainText("$20.00 left of $20.00");
  });
});
