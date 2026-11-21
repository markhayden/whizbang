import { dollars, percent } from "../format";

export interface SpendMeterProps {
  /** Spent so far by the run and its children, in US dollars. */
  spent: number;
  /** What the run reserved: the most it may spend. */
  reserved: number;
}

/** "Spent so far $0.046 of up to $0.14", with a bar. */
export function SpendMeter(props: SpendMeterProps) {
  const fraction = props.reserved > 0 ? props.spent / props.reserved : 0;
  return (
    <div class="meter">
      <div class="meter__head">
        <span class="meter__title">Spent so far</span>
        <span class="meter__figures mono">
          {dollars(props.spent)} <span class="meter__of">of up to {dollars(props.reserved)}</span>
        </span>
      </div>
      <div class="meter__track meter__track--4" role="meter" aria-label="Spent so far" aria-valuemin={0} aria-valuemax={100} aria-valuenow={percent(fraction)}>
        <div class="meter__fill" style={{ width: `${percent(fraction)}%` }} />
      </div>
    </div>
  );
}
