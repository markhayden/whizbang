import { dollars, percent } from "../format";

export interface AllowanceBarProps {
  /** The ledger's allowance, in US dollars. */
  allowance: number;
  /** Spent and reserved since the reset that opened it. */
  spent: number;
  /** When that reset was, already in words: "since 2026-11-02". */
  since: string;
}

/** "$2.48 since 2026-11-02 · $7.52 left of $10.00": the allowance since the last reset. */
export function AllowanceBar(props: AllowanceBarProps) {
  const left = Math.max(0, props.allowance - props.spent);
  const fraction = props.allowance > 0 ? props.spent / props.allowance : 1;
  return (
    <div class="meter">
      <div class="meter__head">
        <span class="meter__figures meter__figures--muted mono">
          {dollars(props.spent)} {props.since}
        </span>
        <span class="meter__figures mono">
          {dollars(left)} left of {dollars(props.allowance)}
        </span>
      </div>
      <div class="meter__track meter__track--6" role="meter" aria-label="Allowance used" aria-valuemin={0} aria-valuemax={100} aria-valuenow={percent(fraction)}>
        <div class="meter__fill" style={{ width: `${percent(fraction)}%` }} />
      </div>
    </div>
  );
}
