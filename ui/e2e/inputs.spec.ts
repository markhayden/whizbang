// Typed inputs: lists, artifact bindings, locations drawn on the canvas, and a snapshot of
// what was sent (plan m5, task W1, items 3 and 4; rows 1 and 9 of the acceptance table).
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { Page } from "@playwright/test";
import { expect, test, withHost } from "./fixtures";
import { api, bytesOf, file, give, openAt, picture, runOf, runWhenReady, script } from "./play";

interface HelloForm {
  kits: Array<{ tasks: Array<{ name: string; fields: Array<{ name: string; count?: { min: number; max: number | null } | null }> }> }>;
}

test.afterEach(async ({ request, host }) => {
  await script(request, host, "reset");
});

/** The arguments of the next preparation the page sends whose arguments hold this; asked before what sends it. */
async function nextPrepared(page: Page, holds: (args: Record<string, unknown>) => boolean): Promise<Record<string, unknown>> {
  const argsOf = (body: string | null) => (JSON.parse(body ?? "{}") as { arguments?: Record<string, unknown> }).arguments ?? {};
  const sent = await page.waitForRequest((request) => request.url().endsWith("/api/prepare") && holds(argsOf(request.postData())));
  return argsOf(sent.postData());
}

const IDENTITY = /^sha256:[0-9a-f]{64}$/;

/** Drags across the canvas through these fractions of its width and height. */
async function drag(page: Page, path: Array<[number, number]>): Promise<void> {
  const box = await page.locator(".canvas__marks").boundingBox();
  if (!box) throw new Error("no canvas");
  const at = ([x, y]: [number, number]) => [box.x + box.width * x, box.y + box.height * y] as const;
  await page.mouse.move(...at(path[0]!));
  await page.mouse.down();
  for (const point of path.slice(1)) await page.mouse.move(...at(point), { steps: 4 });
  await page.mouse.up();
}

test("a variadic port takes, orders and removes pictures within its count", async ({ page, request, host }) => {
  const hello = await api<HelloForm>(request, host, "/api/hello");
  const field = hello.kits.flatMap((kit) => kit.tasks).find((task) => task.name === "reference-consistency")?.fields.find((one) => one.name === "references");
  const most = field?.count?.max ?? 8;
  await openAt(page, host, "#/k/squint/reference-consistency");
  await give(page, "references", [file("a.png", 40), file("b.png", 41), file("c.png", 42)]);
  const items = page.locator("#field-references .list-field__label");
  await expect(items).toHaveText(["a.png", "b.png", "c.png"]);
  await page.getByRole("button", { name: "Move c.png up" }).click();
  await expect(items).toHaveText(["a.png", "c.png", "b.png"]);
  await page.getByRole("button", { name: "Remove a.png" }).click();
  await expect(items).toHaveText(["c.png", "b.png"]);
  const more = Array.from({ length: most }, (_, at) => file(`more-${at}.png`, 50 + at));
  const sent = nextPrepared(page, (args) => Array.isArray(args.references) && args.references.length === most);
  await give(page, "references", more);
  await expect(items).toHaveCount(most);
  await expect(page.locator(".field:has(#field-references) input[type=file]")).toHaveCount(0);
  expect(((await sent).references as string[]).every((id) => IDENTITY.test(id))).toBe(true);
});

test("an artifact port binds a picture from the library and from a drop", async ({ page, host }) => {
  await openAt(page, host, "#/k/rascal/remove_prompt");
  const droppedSent = nextPrepared(page, (args) => typeof args.image === "string");
  await give(page, "image", file("dropped.png", 60));
  await expect(page.locator("#field-image .picture-field__name")).toHaveText("dropped.png");
  const dropped = await droppedSent;
  expect(String(dropped.image)).toMatch(IDENTITY);
  await page.locator("#field-image").getByRole("button", { name: "Clear" }).click();
  const pickedSent = nextPrepared(page, (args) => args.image === dropped.image);
  await page.locator("#field-image").getByRole("button", { name: "From the library" }).click();
  await page.getByRole("group", { name: /^Pictures for / }).getByRole("button", { name: "dropped.png" }).first().click();
  await expect(page.locator("#field-image .picture-field__name")).toHaveText("dropped.png");
  expect((await pickedSent).image).toBe(dropped.image);
});

test("a box and a stroke drawn on the canvas become the location the task is given, and a click and a drag become segment's point and box", async ({ page, request, host }) => {
  // A box on `remove`, run: the run is given the location drawn.
  await openAt(page, host, "#/k/rascal/remove");
  await give(page, "image", file("boxed.png", 70));
  await expect(page.locator(".canvas__marks")).toBeVisible();
  const boxedSent = nextPrepared(page, (args) => typeof args.location === "string");
  await drag(page, [
    [0.2, 0.2],
    [0.6, 0.7],
  ]);
  const boxed = await boxedSent;
  await runWhenReady(page);
  await expect(page.locator(".tag", { hasText: "DONE" })).toBeVisible();
  const ran = await api<{ arguments: Record<string, unknown> }>(request, host, `/api/runs/${runOf(page)}`);
  expect(ran.arguments.location).toBe(boxed.location);
  // A stroke on `repair`.
  await page.goto(`${host.address}#/k/rascal/repair`);
  await give(page, "image", file("scratched.png", 71));
  await expect(page.locator(".canvas__marks")).toBeVisible();
  const strokedSent = nextPrepared(page, (args) => typeof args.stroke === "string");
  await drag(page, [
    [0.1, 0.5],
    [0.4, 0.45],
    [0.8, 0.55],
  ]);
  expect(String((await strokedSent).stroke)).toMatch(IDENTITY);
  // Segment's query: a click is a point, a drag a box.
  await page.goto(`${host.address}#/k/rascal/segment`);
  await give(page, "image", file("segmented.png", 72));
  await expect(page.locator(".canvas__marks")).toBeVisible();
  await page.getByRole("tab", { name: "Point", exact: true }).click();
  await drag(page, [
    [0.5, 0.5],
    [0.5, 0.5],
  ]);
  await expect(page.locator("#field-query")).toHaveValue(/^\d+ \d+$/);
  await expect(page.getByRole("tab", { name: "point", exact: true })).toHaveAttribute("aria-selected", "true");
  await page.getByRole("tab", { name: "Box", exact: true }).click();
  await drag(page, [
    [0.2, 0.2],
    [0.7, 0.8],
  ]);
  await expect(page.locator("#field-query")).toHaveValue(/^\d+ \d+ \d+ \d+$/);
  await expect(page.getByRole("tab", { name: "box", exact: true })).toHaveAttribute("aria-selected", "true");
});

test("a picture changed on disk after it was sent leaves the run's input and result as they were", async ({ page, request, host }) => {
  const folder = mkdtempSync(join(tmpdir(), "whizbang-e2e-disk-"));
  try {
    const path = join(folder, "garden.png");
    writeFileSync(path, picture(80));
    await openAt(page, host, "#/k/rascal/remove_prompt");
    await page.locator("#field-image input[type=file]").setInputFiles(path);
    await page.locator("#field-prompt").fill("the hose");
    await runWhenReady(page);
    await expect(page.locator(".tag", { hasText: "DONE" })).toBeVisible();
    const run = runOf(page);
    const ended = await api<{ arguments: { image: string }; outputs: Array<{ type: string; id: string }> }>(request, host, `/api/runs/${run}`);
    const input = await bytesOf(request, host, ended.arguments.image);
    const result = ended.outputs.find((output) => output.type === "image")!;
    const made = await bytesOf(request, host, result.id);
    // The file is changed, then gone: the run's input and result are the store's own.
    writeFileSync(path, picture(81));
    rmSync(path);
    await page.reload();
    await expect(page.locator(".tag", { hasText: "DONE" })).toBeVisible();
    expect((await bytesOf(request, host, ended.arguments.image)).equals(input)).toBe(true);
    expect((await bytesOf(request, host, result.id)).equals(made)).toBe(true);
  } finally {
    rmSync(folder, { recursive: true, force: true });
  }
});

test("a picture picked in search starts a task, whose mask port lists the store's masks after a reload, and a task shows its earlier runs with their mask and result", async ({ page }) => {
  await withHost([], async (host) => {
    await openAt(page, host, "#/k/rascal/remove_prompt");
    await give(page, "image", file("picked.png", 71));
    await page.locator("#field-prompt").fill("the rake");
    await runWhenReady(page);
    await expect(page.locator(".workspace__title-row .tag", { hasText: "DONE" })).toBeVisible();
    const removed = runOf(page);
    // Picked in search, the picture starts a task with it given.
    await page.goto(`${host.address}#/`);
    await page.keyboard.press("ControlOrMeta+k");
    await page.getByLabel("Search tasks, tools and pictures").fill("picked.png");
    await page.getByRole("option", { name: /picked\.png/ }).click();
    await expect(page).toHaveURL(/#\/\?picture=sha256:[0-9a-f]{64}$/);
    const panel = page.getByRole("region", { name: "Start a task with picked.png" });
    await panel.getByRole("button", { name: /Protect a first edit/ }).click();
    await expect(page).toHaveURL(/#\/k\/rascal\/protect$/);
    await expect(page.locator(".field:has(#field-image) .picture-field__name")).toContainText("picked.png");
    // The mask port offers the removal's mask from the store, after a reload too.
    await page.reload();
    const option = page.locator(".binding-picker [role=option]");
    await expect(option).toHaveCount(1);
    await expect(option).toContainText(removed);
    await option.locator("button").click();
    await expect(page.getByRole("tab", { name: "Mask" })).toBeVisible();
    // The removal's own task lists it among its runs, and its mask and result as tabs.
    await page.goto(`${host.address}#/k/rascal/remove_prompt`);
    await expect(page.locator(".workspace__runs .continue-row", { hasText: removed })).toContainText("DONE");
    await expect(page.getByRole("tab", { name: "Mask" })).toBeVisible();
    await page.getByRole("tab", { name: "Result" }).click();
    await expect(page.locator(".workspace__note")).toContainText(`The result of the last run, ${removed}.`);
    await expect(page.locator(".canvas__picture")).toBeVisible();
  });
});
