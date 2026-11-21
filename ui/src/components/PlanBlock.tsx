import { dollars } from "../format";

/** One need of a preparation: a capability, what serves it, and its bound (K1, item 2). */
export interface PlanNeed {
  capability: string;
  backend: string;
  model?: string;
  /** How many requests the call may send to it. */
  requests: number;
  /** The bound of one request, in US dollars; 0 for free local work. */
  each: number;
  /** The bound of all of them. */
  total: number;
  local?: boolean;
}

export interface PlanBlockProps {
  needs: PlanNeed[];
  /** The total bound of the call: what is held while it runs ("up to $X"). */
  bound: number;
  /** A loop's attempts, as asked and as allowed. */
  attempts?: { asked: number; allowed: number };
  /** The preparation is being made again, after a change. */
  stale?: boolean;
}

/** The words of one need's figures: "17 req · local", "3 × $0.040", "3 × ≤ $1.020". */
export function needFigures(need: PlanNeed): string {
  if (need.local || need.each === 0) return `${need.requests} req · local`;
  return `${need.requests} × ${need.requests > 1 ? "≤ " : ""}${dollars(need.each)}`;
}

/** What the call needs, by capability, and the most it may cost before Run. */
export function PlanBlock(props: PlanBlockProps) {
  return (
    <section class="plan-block" aria-label="Plan" aria-busy={props.stale ? "true" : undefined}>
      {props.needs.map((need) => (
        <div class="plan-block__row" key={`${need.capability}:${need.backend}:${need.model ?? ""}`}>
          <span class="plan-block__name mono">
            {need.capability}
            <span class="plan-block__backend">
              {" · "}
              {need.backend}
              {need.model ? ` ${need.model}` : ""}
            </span>
          </span>
          <span class="plan-block__figures mono">{needFigures(need)}</span>
        </div>
      ))}
      {props.attempts ? (
        <div class="plan-block__row">
          <span class="plan-block__name mono">attempts</span>
          <span class="plan-block__figures mono">
            {props.attempts.asked} of at most {props.attempts.allowed}
          </span>
        </div>
      ) : null}
      <div class="plan-block__row plan-block__row--total">
        <span class="plan-block__name mono">held while running</span>
        <span class="plan-block__total mono">{props.bound === 0 ? "free" : `up to ${dollars(props.bound)}`}</span>
      </div>
    </section>
  );
}
