import { cx } from "./cx";

export interface Segment {
  value: string;
  label: string;
}

export interface SegmentedProps {
  label: string;
  segments: Segment[];
  value: string;
  /** Numbers use the inset ink style; views use the quiet tab style. */
  look?: "numbers" | "tabs";
  disabled?: boolean;
  onChange?: (value: string) => void;
}

/** One of a few: a number of attempts, or a view (Source, Result, Compare). */
export function Segmented(props: SegmentedProps) {
  const look = props.look ?? "tabs";
  return (
    <div class={cx("segmented", `segmented--${look}`)} role="tablist" aria-label={props.label}>
      {props.segments.map((segment) => {
        const selected = segment.value === props.value;
        return (
          <button
            key={segment.value}
            type="button"
            role="tab"
            aria-selected={selected ? "true" : "false"}
            class={cx("segmented__item", look === "numbers" && "mono", selected && "segmented__item--selected")}
            disabled={props.disabled}
            onClick={() => props.onChange?.(segment.value)}
          >
            {segment.label}
          </button>
        );
      })}
    </div>
  );
}
