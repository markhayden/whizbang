// The shell in Chrome (plan m5, task U1, item 7).
import { expect, test } from "./fixtures";

test("the page loads with the token, removes it from the address and shows Home", async ({ page, host }) => {
  const hello = page.waitForResponse((response) => response.url() === `${host.address}api/hello`);
  await page.goto(host.opening);
  expect((await hello).status()).toBe(200);
  await expect(page.getByRole("heading", { name: /Home/ })).toBeVisible();
  await expect(page.getByRole("status")).toContainText("ready");
  expect(page.url()).toBe(`${host.address}#/`);
  expect(page.url()).not.toContain(host.token);
  // The token is kept for this tab: a reload of the same tab, without it in the address,
  // still works; another tab is not given it.
  await page.reload();
  await expect(page.getByRole("status")).toContainText("ready");
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
  await expect(page.getByRole("status")).toContainText("ready");
  await page.evaluate(async () => {
    await document.fonts.ready;
  });
  const fonts = await page.evaluate(() => [...document.fonts].filter((font) => font.status === "loaded").map((font) => font.family));
  expect(fonts.join(" ")).toContain("Geist");
  expect(asked.length).toBeGreaterThan(2);
  const elsewhere = asked.filter((url) => !url.startsWith(`${host.origin}/`) && !url.startsWith("data:") && !url.startsWith("blob:"));
  expect(elsewhere).toEqual([]);
});
