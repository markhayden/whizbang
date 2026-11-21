import { bytes } from "../format";
import { cx } from "./cx";

/**
 * Why a choice is or is not ready, as the host hands it out: `ready`, or one of the kinds
 * a preparation says beside its words (plan m5, task K1, item 1).
 */
export type ReadinessKind =
  | "ready"
  | "no_key"
  | "not_installed"
  | "out_of_date"
  | "damaged"
  | "not_in_catalogue"
  | "not_for_this_platform"
  | "other";

export interface Choice {
  value: string;
  label: string;
  readiness: ReadinessKind;
  /** The size of what Get would fetch, in bytes, for a choice that is not installed. */
  size?: number;
  /** The host's words for why it is not ready, shown as the row's title. */
  why?: string;
}

/** The mark a readiness kind shows at the row's end, and its tone. */
export function readinessMark(kind: ReadinessKind, size?: number): { text: string; tone: "pass" | "lacquer" | "volt" | "muted" } {
  switch (kind) {
    case "ready":
      return { text: "READY", tone: "pass" };
    case "no_key":
      return { text: "NO KEY", tone: "lacquer" };
    case "not_installed":
      return { text: size === undefined ? "GET" : `GET · ${bytes(size)}`, tone: "volt" };
    case "out_of_date":
      return { text: size === undefined ? "UPDATE" : `UPDATE · ${bytes(size)}`, tone: "volt" };
    case "damaged":
      return { text: "DAMAGED", tone: "lacquer" };
    case "not_in_catalogue":
    case "not_for_this_platform":
    case "other":
      return { text: "NOT READY", tone: "muted" };
  }
}

export interface ChoiceListProps {
  /** Names the group for assistive technology. */
  label: string;
  name: string;
  choices: Choice[];
  value: string | null;
  disabled?: boolean;
  onChange?: (value: string) => void;
}

/**
 * The choices of a capability, each with its readiness. A choice that is not ready can
 * still be picked: Run then says what is missing (plan m5, task W1, item 4).
 */
export function ChoiceList(props: ChoiceListProps) {
  return (
    <div class="choice-list" role="radiogroup" aria-label={props.label}>
      {props.choices.map((choice) => {
        const mark = readinessMark(choice.readiness, choice.size);
        const selected = choice.value === props.value;
        return (
          <label
            key={choice.value}
            class={cx("choice-list__row", selected && "choice-list__row--selected", choice.readiness !== "ready" && "choice-list__row--unready")}
            title={choice.why}
          >
            <input
              class="visually-hidden"
              type="radio"
              name={props.name}
              value={choice.value}
              checked={selected}
              disabled={props.disabled}
              onChange={() => props.onChange?.(choice.value)}
            />
            <span class="choice-list__dot" aria-hidden="true" />
            <span class="choice-list__label">{choice.label}</span>
            <span class={`choice-list__mark mono tone--${mark.tone}`}>{mark.text}</span>
          </label>
        );
      })}
    </div>
  );
}
