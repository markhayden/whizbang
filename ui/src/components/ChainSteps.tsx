import type { BundleStep, StepState } from "../bundles";
import { middle } from "../format";
import "./bundles.css";

export interface ChainStepsProps {
  steps: BundleStep[];
  states: StepState[];
  /** The new run of each step, or none. */
  runs: Array<string | null>;
  onOpen?: (run: string) => void;
}

const WORDS: Record<StepState, string> = {
  ended: "ended",
  review: "to review",
  waiting: "waits",
  stopped: "stopped",
  blocked: "blocked",
};

/** A chain's steps as a list, each with its state (the design's component "Chain step"). */
export function ChainSteps(props: ChainStepsProps) {
  return (
    <ol class="chain-steps" aria-label="The chain's steps">
      {props.steps.map((step, at) => {
        const state = props.states[at] ?? "waiting";
        const run = props.runs[at];
        return (
          <li key={at} class={`chain-step chain-step--${state}`} data-state={state}>
            <span class="chain-step__mark mono" aria-hidden="true">
              {state === "ended" ? "✓" : String(at + 1)}
            </span>
            <span class="chain-step__text">
              <span class="chain-step__name">{`${at + 1} · ${step.title ?? step.action ?? "a step"}`}</span>
              <span class="chain-step__line mono">
                {WORDS[state]}
                {run ? ` · ${middle(run, 14)} · was ${middle(step.run, 14)}` : ` · was ${middle(step.run, 14)}`}
              </span>
            </span>
            {run && props.onOpen ? (
              <button type="button" class="chain-step__open" onClick={() => props.onOpen?.(run)}>
                Open
              </button>
            ) : null}
          </li>
        );
      })}
    </ol>
  );
}
