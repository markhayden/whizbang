import { elapsed } from "../format";
import { cx } from "./cx";
import { ProgressBar } from "./ProgressBar";

export type StepState = "done" | "failed" | "running" | "waiting";

export interface Step {
  kind: "step";
  /** Unique within the list. */
  key: string;
  name: string;
  state: StepState;
  /** Elapsed milliseconds: so far for a running step, in all for an ended one. */
  ms?: number;
  /** Where it ran and what it cost, in words: "local", "$0.002". */
  where?: string;
  /** What the step said, as text: "SAM3 found 1 hose". */
  note?: string;
}

export interface AttemptDivider {
  kind: "attempt";
  key: string;
  number: number;
  /** What changed for this attempt: "widen the mask". */
  change?: string;
}

/** A gap where events were not kept (plan m5, task K1, item 4). */
export interface Skipped {
  kind: "skipped";
  key: string;
  count: number;
}

export type StepListEntry = Step | AttemptDivider | Skipped;

export interface StepListProps {
  entries: StepListEntry[];
}

const GLYPH: Record<StepState, string> = { done: "✓", failed: "✕", running: "◐", waiting: "○" };

/**
 * A run's steps: done, failed, running (with its elapsed time and a bar that says it is
 * going, not how far, since a request reports no fraction done), waiting; and a loop's
 * attempt dividers.
 */
export function StepList(props: StepListProps) {
  return (
    <ol class="step-list" aria-label="Steps">
      {props.entries.map((entry) => {
        if (entry.kind === "attempt") {
          return (
            <li key={entry.key} class="step-list__attempt mono" role="separator">
              <span class="step-list__rule" />
              <span>
                ATTEMPT {entry.number}
                {entry.change ? ` · ${entry.change}` : ""}
              </span>
              <span class="step-list__rule" />
            </li>
          );
        }
        if (entry.kind === "skipped") {
          return (
            <li key={entry.key} class="step-list__skipped mono">
              {entry.count} {entry.count === 1 ? "message" : "messages"} not kept
            </li>
          );
        }
        return (
          <li key={entry.key} class={cx("step-list__step", `step-list__step--${entry.state}`)} data-state={entry.state}>
            <span class="step-list__glyph" aria-hidden="true">
              {GLYPH[entry.state]}
            </span>
            <span class="step-list__body">
              <span class="step-list__head">
                <span class="step-list__name">{entry.name}</span>
                <span class="visually-hidden">{entry.state}</span>
                {entry.state === "waiting" ? null : (
                  <span class="step-list__figures mono">
                    {entry.ms !== undefined ? elapsed(entry.ms) : ""}
                    {entry.state === "running" ? " …" : entry.where ? ` · ${entry.where}` : ""}
                  </span>
                )}
              </span>
              {entry.state === "running" ? <ProgressBar label={`${entry.name} running`} /> : null}
              {entry.note && entry.state !== "running" ? <span class="step-list__note">{entry.note}</span> : null}
            </span>
          </li>
        );
      })}
    </ol>
  );
}
