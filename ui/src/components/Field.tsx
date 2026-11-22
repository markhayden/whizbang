import type { ComponentChildren } from "preact";
import { cx } from "./cx";

export interface FieldProps {
  /** The field's id, which its label points at. */
  id: string;
  label: string;
  optional?: boolean;
  /** A line under the field. */
  help?: string;
  /** A refusal under the field, shown instead of the help. */
  error?: string;
  disabled?: boolean;
  /** A tag beside the label, such as FROM RASCAL: the value came from another run. */
  from?: string;
  children: ComponentChildren;
}

/** A label (and the tag of where its value came from), the control it names, and one line under it: help or an error. */
export function Field(props: FieldProps) {
  const note = props.error ?? props.help;
  return (
    <div class={cx("field", props.error && "field--error", props.disabled && "field--disabled")}>
      <div class="field__head">
        <label class="field__label" for={props.id}>
          {props.label}
          {props.optional ? <span class="field__optional"> · optional</span> : null}
        </label>
        {props.from ? <span class="field__from mono">{props.from}</span> : null}
      </div>
      {props.children}
      {note ? (
        <div class={props.error ? "field__error" : "field__help"} id={`${props.id}-note`} role={props.error ? "alert" : undefined}>
          {note}
        </div>
      ) : null}
    </div>
  );
}
