// The replay of a run (plan m7, task U2, item 3): Replay… on a result, offline with nothing
// spent, and live with its cost reviewed first; the report at the top of the replay's
// result: reproduced, or where it differs.
import { expect, test } from "./fixtures";
import { realityCheck, removal } from "./kept";
import { runOf, script } from "./play";

test.afterEach(async ({ request, host }) => {
  await script(request, host, "reset");
});

test("replay of a result shows reproduced, and a changed one shows where it differs", async ({ page, request, host }) => {
  await removal(page, host, "replay.png", 81, "the rake");
  const run = runOf(page);
  await page.getByRole("button", { name: /^More/ }).click();
  await page.getByRole("menuitem", { name: "Replay…" }).click();
  const dialog = page.getByRole("dialog", { name: `Replay run ${run}` });
  await dialog.getByRole("button", { name: "Replay offline" }).click();
  await expect(page).not.toHaveURL(new RegExp(`#/run/${run}$`));
  const report = page.getByRole("region", { name: "Replay report" });
  await expect(report).toContainText("Reproduced");
  await expect(report).toContainText("$0.000 spent");
  // A check whose judge now answers otherwise, replayed live with its cost reviewed: the
  // report says where it differs.
  await script(request, host, "pass");
  await realityCheck(page, host, "replay-check.png", 82, "PASS");
  const checked = runOf(page);
  await script(request, host, "fail");
  await page.getByRole("button", { name: /^More/ }).click();
  await page.getByRole("menuitem", { name: "Replay…" }).click();
  const live = page.getByRole("dialog", { name: `Replay run ${checked}` });
  await live.getByRole("radio").nth(1).check();
  await expect(live).toContainText("Live, with the cost reviewed");
  await live.getByRole("button", { name: /^Replay live · up to \$/ }).click();
  const differs = page.getByRole("region", { name: "Replay report" });
  await expect(differs).toContainText(/Differs in \d+ place/);
  await expect(differs).toContainText("live");
});
