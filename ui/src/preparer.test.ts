// Preparing on every change: once a pause, and only the newest answer shown (plan m5, task W1, item 5).
import { describe, expect, it } from "vitest";
import type { Prepared } from "./api";
import { createPreparer } from "./preparer";

function timers() {
  const pending: Array<{ run: () => void; id: number }> = [];
  let next = 0;
  return {
    pending,
    set: (run: () => void) => {
      next += 1;
      pending.push({ run, id: next });
      return next;
    },
    clear: (id: unknown) => {
      const at = pending.findIndex((one) => one.id === id);
      if (at >= 0) pending.splice(at, 1);
    },
    fire: () => pending.splice(0).forEach((one) => one.run()),
  };
}

describe("the preparer", () => {
  it("prepares once after a run of changes, with the last arguments", () => {
    const clock = timers();
    const sent: unknown[] = [];
    const preparer = createPreparer({ prepare: (_, args) => (sent.push(args), new Promise<Prepared>(() => undefined)), onAnswer: () => undefined, onError: () => undefined, onBusy: () => undefined, timers: clock });
    preparer.change("t", { a: 1 });
    preparer.change("t", { a: 2 });
    preparer.change("t", { a: 3 });
    expect(sent).toEqual([]);
    clock.fire();
    expect(sent).toEqual([{ a: 3 }]);
  });

  it("drops the answer of an older preparation than the newest sent", async () => {
    const clock = timers();
    const answers: Prepared[] = [];
    const resolvers: Array<(answer: Prepared) => void> = [];
    const preparer = createPreparer({
      prepare: () => new Promise<Prepared>((resolve) => resolvers.push(resolve)),
      onAnswer: (answer) => answers.push(answer),
      onError: () => undefined,
      onBusy: () => undefined,
      timers: clock,
    });
    preparer.now("t", { a: 1 });
    preparer.now("t", { a: 2 });
    resolvers[1]!({ ready: true, handle: "new" });
    resolvers[0]!({ ready: true, handle: "old" });
    await Promise.resolve();
    await Promise.resolve();
    expect(answers.map((a) => a.handle)).toEqual(["new"]);
    preparer.stop();
    preparer.change("t", {});
    expect(clock.pending).toHaveLength(0);
  });
});
