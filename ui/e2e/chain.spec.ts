// Guided chain replay (plan m7, task U2, item 2; the acceptance row "Guided chain replay in
// the playground"): a chain picked in History and bundled, taken in on the Bundles page,
// and replayed one ordinary run a step, each step reviewed as a draft and run only when
// Run is pressed, its linked input bound to the new run of the step before.
import { readFileSync } from "node:fs";
import { expect, test } from "./fixtures";
import { api, file, give, openAt, runOf, runWhenReady } from "./play";
import { bundledVia, chainVia } from "./bundled";

interface Recorded {
  inputs: Array<{ port: string; id: string }>;
  outputs: Array<{ port: string; type: string; id: string }>;
  replayed: { run: string; bundle: { digest: string; step: number } | null; mode: string } | null;
}

test("a chain of a removal and its check replays one ordinary run a step, binds the new output and shows its changed identity", async ({ page, request, host }) => {
  const chain = await chainVia(request, host, 71, "the hose");
  // Picked in History in the order they ran: the host names the link between them.
  await openAt(page, host, "#/history");
  await page.getByRole("button", { name: "Make a chain" }).click();
  await page.locator(`[data-run="${chain.removal}"] .history__pin-button`, { hasText: "Pick" }).click();
  await page.locator(`[data-run="${chain.check}"] .history__pin-button`, { hasText: "Pick" }).click();
  await expect(page.locator(".history__chain-title")).toHaveText("Making a chain · 2 picked · 1 link");
  await expect(page.locator(".history__chain")).toContainText("1's image → 2's");
  const downloading = page.waitForEvent("download");
  await page.getByRole("button", { name: "Bundle chain" }).click();
  const saved = await (await downloading).path();
  // Taken in on the Bundles page: checked whole, its steps and their link shown.
  await page.goto(`${host.address}#/bundles`);
  await page.locator(".bundles__drop input[type=file]").setInputFiles({ name: "chain.whizbang-bundle", mimeType: "application/octet-stream", buffer: readFileSync(saved) });
  await expect(page).toHaveURL(/#\/bundles\/sha256:[0-9a-f]{64}$/);
  const card = page.locator(".bundle-card");
  await expect(card).toContainText("A chain of 2 steps");
  await expect(card).toContainText("step 1 replays offline");
  await card.getByRole("button", { name: "Start the chain" }).click();
  await expect(page).toHaveURL(/#\/chain\//);
  await expect(page.locator(".chain-step--review")).toContainText("1 ·");
  // Step 1 opens as a draft and runs nothing; the person changes its picture and runs it.
  await page.getByRole("button", { name: "Review step 1" }).click();
  await expect(page).toHaveURL(/#\/k\/rascal\/remove_prompt$/);
  await expect(page.locator(".workspace__chain")).toContainText("STEP 1 OF 2");
  expect((await api<{ runs: unknown[] }>(request, host, "/api/runs?open=1")).runs).toHaveLength(0);
  await page.locator(".field:has(#field-image)").getByRole("button", { name: "Clear" }).click();
  await give(page, "image", file("chain-other.png", 72));
  await runWhenReady(page);
  const first = runOf(page);
  // Continue: step 2 opens with its candidate bound to step 1's new output, which differs
  // from the one recorded.
  const next = page.locator(".chain-card").getByRole("button", { name: "Continue to step 2" });
  await expect(next).toBeVisible();
  await next.click();
  await expect(page).toHaveURL(/#\/k\/squint\/source-fidelity$/);
  await expect(page.locator(".workspace__chain")).toContainText("STEP 2 OF 2");
  await expect(page.locator(".chain-binding--differs")).toContainText("differs from recorded");
  await runWhenReady(page);
  const second = runOf(page);
  await expect(page.locator(".chain-card")).toContainText("Chain done");
  await expect(page.locator(".chain-card").getByRole("button", { name: "Compare last step" })).toBeVisible();
  // Each step's run is an ordinary run that names the bundle and its step.
  const made = await api<Recorded>(request, host, `/api/runs/${first}/record`);
  const checked = await api<Recorded>(request, host, `/api/runs/${second}/record`);
  expect(made.replayed?.bundle?.step).toBe(0);
  expect(checked.replayed?.bundle?.step).toBe(1);
  expect(checked.replayed?.mode).toBe("live");
  const output = made.outputs.find((one) => one.port === "image")!.id;
  expect(checked.inputs.map((one) => one.id)).toContain(output);
});

test("stop leaves the chain and a reopened tab finds its runs", async ({ page, request, host }) => {
  const chain = await chainVia(request, host, 73, "the cup");
  const digest = await bundledVia(request, host, [chain.removal, chain.check]);
  await openAt(page, host, `#/chain/${digest}`);
  await page.getByRole("button", { name: "Review step 1" }).click();
  await runWhenReady(page);
  const first = runOf(page);
  await page.locator(".chain-card").getByRole("button", { name: "Stop" }).click();
  await expect(page.locator(".chain-card")).toContainText("You stopped this chain here");
  // Another tab of the same page: the chain is read again from its runs and its bundle.
  const other = await page.context().newPage();
  await openAt(other, host, `#/chain/${digest}`);
  await expect(other.locator(".chain-step--ended")).toHaveCount(1);
  await expect(other.locator(".chain-step--ended")).toContainText(first.slice(0, 6));
  await expect(other.locator(".chain-step--stopped")).toHaveCount(1);
  await expect(other.getByRole("button", { name: "Go on with the chain" })).toBeVisible();
  // Nothing ran on its own: one run of the chain, and nothing going.
  expect((await api<{ runs: unknown[] }>(request, host, "/api/runs?open=1")).runs).toHaveLength(0);
  expect((await api<{ runs: Array<{ run: string }> }>(request, host, "/api/runs?replayed=only")).runs.map((one) => one.run)).toContain(first);
  await other.close();
});
