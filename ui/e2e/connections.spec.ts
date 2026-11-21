// Of Chrome's six connections to one host, the page's pictures take at most two a tab, so
// even with two tabs' pictures held open by the host, polls, Stop and Quit find a
// connection (plan m5, "Who may call"; the reviews of task W1). The fixture holds every
// `/api/artifacts/…` request open while its script says `hold-pictures`.
import type { Page } from "@playwright/test";
import { expect, test, withHost } from "./fixtures";
import { file, give, openAt, picture, runWhenReady, script } from "./play";

test("pictures held open on the host take two connections a tab and leave the two that polls, stop and quit need", async ({ page, request }) => {
  // A host of its own: the case ends with Quit.
  await withHost([], async (host) => {
    const sent = { Authorization: `Bearer ${host.token}`, Origin: host.origin };
    for (let seed = 300; seed < 306; seed += 1) {
      const answer = await request.post(`${host.address}api/pictures?name=held-${seed}.png`, {
        headers: { ...sent, "Content-Type": "image/png" },
        data: picture(seed),
      });
      expect(answer.status()).toBe(201);
    }

    // The run is held open from now on.
    await script(request, host, "hold");
    // A request is open from when it is sent until its answer's head arrives (which the
    // page sees before it can send a picture again, a 429's retry among them), or it fails.
    const inFlight = new Map<Page, Set<object>>();
    const open = new Map<Page, number>();
    let most = 0;
    const counting = (tab: Page) => {
      const artifact = (asked: { url(): string }) => asked.url().includes("/api/artifacts/");
      const held = new Set<object>();
      inFlight.set(tab, held);
      tab.on("request", (asked) => {
        if (!artifact(asked)) return;
        held.add(asked);
        open.set(tab, held.size);
        most = Math.max(most, held.size);
      });
      const done = (asked: object) => {
        held.delete(asked);
        open.set(tab, held.size);
      };
      tab.on("response", (answer) => done(answer.request()));
      tab.on("requestfinished", done);
      tab.on("requestfailed", done);
    };
    counting(page);
    await openAt(page, host, "#/k/rascal/remove_prompt");
    await give(page, "image", file("held.png", 299));
    await page.locator("#field-prompt").fill("the hose");
    await runWhenReady(page);
    await expect(page.locator('.step-list [data-state="running"]').first()).toBeVisible();
    await expect(page.locator(".canvas__picture").first()).toBeVisible();
    // And every picture any tab asks for from now on is held open by the host.
    await script(request, host, "hold-pictures");

    // A second tab's library: its thumbnails are asked for and held, two at a time; the
    // rest wait in the page.
    const other = await page.context().newPage();
    counting(other);
    await openAt(other, host, "#/");
    await expect.poll(() => open.get(other) ?? 0).toBe(2);

    // The run's tab still polls, and Stop is answered.
    await page.getByRole("button", { name: "Stop run" }).click();
    await expect(page.locator(".tag", { hasText: "CANCELLED" })).toBeVisible();

    // Quit is answered too, and the host goes.
    const quit = await request.post(`${host.address}api/quit`, { headers: sent });
    expect(quit.status()).toBe(202);
    await expect
      .poll(async () => {
        try {
          await request.get(`${host.address}api/hello`, { headers: sent, timeout: 1_000 });
          return "answering";
        } catch {
          return "gone";
        }
      })
      .toBe("gone");
    // No tab ever held more than two of its connections with pictures.
    expect(most).toBeLessThanOrEqual(2);
    await other.close();
  });
});
