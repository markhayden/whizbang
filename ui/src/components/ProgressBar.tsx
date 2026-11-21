import { percent } from "../format";

export interface ProgressBarProps {
  label: string;
  /** How far, from 0 to 1; without it the bar says only that work is going. */
  value?: number;
  /** "Depth" and "1.1 of 1.4 GB" above the bar, as in a download. */
  title?: string;
  figures?: string;
}

/** A thin bar: how far, or (with no value) that something is going. */
export function ProgressBar(props: ProgressBarProps) {
  const known = props.value !== undefined;
  const bar = (
    <div
      class={known ? "progress" : "progress progress--indeterminate"}
      role="progressbar"
      aria-label={props.label}
      aria-valuemin={known ? 0 : undefined}
      aria-valuemax={known ? 100 : undefined}
      aria-valuenow={known ? percent(props.value ?? 0) : undefined}
    >
      <div class="progress__fill" style={known ? { width: `${percent(props.value ?? 0)}%` } : undefined} />
    </div>
  );
  if (!props.title && !props.figures) return bar;
  return (
    <div class="progress-card">
      <div class="progress-card__head">
        <span class="progress-card__title">{props.title}</span>
        <span class="progress-card__figures mono">{props.figures}</span>
      </div>
      {bar}
    </div>
  );
}
