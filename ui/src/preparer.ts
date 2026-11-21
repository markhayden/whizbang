// Preparing on every change (plan m5, task W1, item 5): a change waits 300 ms for the next,
// so typing prepares once; and an answer to an older preparation than the newest sent is
// dropped, so what the page shows is always what it last asked.
import type { Prepared } from "./api";

/** How long a change waits for the next before the call is prepared. */
export const PREPARE_DEBOUNCE_MS = 300;

export interface Preparer {
  /** The call changed: prepare it once no further change comes for the debounce. */
  change(action: string, args: Record<string, unknown>): void;
  /** Prepare now, without waiting: after a stale start. */
  now(action: string, args: Record<string, unknown>): void;
  stop(): void;
}

export interface PreparerOptions {
  prepare: (action: string, args: Record<string, unknown>) => Promise<Prepared>;
  /** The answer to the newest preparation sent, and only that one. */
  onAnswer: (answer: Prepared) => void;
  onError: (error: unknown) => void;
  /** Whether a preparation is waiting or out: the plan block is shown as stale meanwhile. */
  onBusy: (busy: boolean) => void;
  delay?: number;
  timers?: { set: (run: () => void, ms: number) => unknown; clear: (timer: unknown) => void };
}

export function createPreparer(options: PreparerOptions): Preparer {
  const delay = options.delay ?? PREPARE_DEBOUNCE_MS;
  const timers = options.timers ?? {
    set: (run: () => void, ms: number) => setTimeout(run, ms),
    clear: (timer: unknown) => clearTimeout(timer as ReturnType<typeof setTimeout>),
  };
  let timer: unknown;
  let sent = 0;
  let stopped = false;
  const send = (action: string, args: Record<string, unknown>) => {
    timer = undefined;
    sent += 1;
    const mine = sent;
    options.prepare(action, args).then(
      (answer) => {
        if (stopped || mine !== sent) return;
        options.onBusy(false);
        options.onAnswer(answer);
      },
      (error: unknown) => {
        if (stopped || mine !== sent) return;
        options.onBusy(false);
        options.onError(error);
      },
    );
  };
  return {
    change: (action, args) => {
      if (stopped) return;
      if (timer !== undefined) timers.clear(timer);
      options.onBusy(true);
      timer = timers.set(() => send(action, args), delay);
    },
    now: (action, args) => {
      if (stopped) return;
      if (timer !== undefined) timers.clear(timer);
      options.onBusy(true);
      send(action, args);
    },
    stop: () => {
      stopped = true;
      if (timer !== undefined) timers.clear(timer);
    },
  };
}
