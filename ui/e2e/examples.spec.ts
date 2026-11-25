// Saved examples (plan m7, task U1, item 2; the acceptance row "Saved examples in the
// playground"): saved from a result, listed on Home, opened as a draft that runs nothing,
// after the host is started again and after the run it came from was deleted.
import { execFileSync } from "node:child_process";
import { resolve } from "node:path";
import { BINARY, expect, test } from "./fixtures";
import { removal, withKeptHost } from "./kept";
import { api, openAt } from "./play";

test("an example saved from a result opens as a draft that runs nothing, after a restart and after its run was deleted", async ({ page, request }) => {
  await withKeptHost(async (kept) => {
    const run = await removal(page, kept.host, "example.png", 61, "the garden hose");
    await page.getByRole("button", { name: /^More/ }).click();
    await page.getByRole("menuitem", { name: "Save as example…" }).click();
    const dialog = page.getByRole("dialog", { name: "Save as example" });
    await expect(dialog).toContainText("runs nothing when opened");
    await dialog.locator("input").fill("Porch hose, wider mask");
    await expect(dialog).toContainText("22/80");
    await dialog.getByRole("button", { name: "Save example" }).click();
    await expect(page.getByRole("status").filter({ hasText: "Saved as the example" })).toBeVisible();
    // The host stops; the run is deleted from the command line; the host starts again.
    const host = await kept.restartAfter((store) => {
      execFileSync(resolve(BINARY), ["cli", "store", "delete", "--store", store, run], { stdio: "pipe" });
    });
    expect((await api<{ runs: unknown[] }>(request, host, "/api/runs")).runs).toHaveLength(0);
    await openAt(page, host, "#/");
    const row = page.locator(".examples__row", { hasText: "Porch hose, wider mask" });
    await expect(row).toContainText("Rascal · Remove what you name");
    await row.locator(".examples__name").click();
    // A draft of the task, with the example's words and picture, saying where it came from.
    await expect(page).toHaveURL(/#\/k\/rascal\/remove_prompt$/);
    await expect(page.locator(".workspace__example")).toContainText("Porch hose, wider mask");
    await expect(page.locator("#field-prompt")).toHaveValue("the garden hose");
    await expect(page.locator(".field:has(#field-image) .picture-field__name")).toBeVisible();
    await expect(page.locator(".workspace__run .button--primary")).toBeEnabled();
    // It ran nothing: no run is going, and the store holds none.
    expect((await api<{ runs: unknown[] }>(request, host, "/api/runs?open=1")).runs).toHaveLength(0);
    expect((await api<{ runs: unknown[] }>(request, host, "/api/runs")).runs).toHaveLength(0);
  });
});
