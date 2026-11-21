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
  children: ComponentChildren;
}

/** A label, the control it names, and one line under it: help or an error. */
export function Field(props: FieldProps) {
  const note = props.error ?? props.help;
  return (
    <div class={cx("field", props.error && "field--error", props.disabled && "field--disabled")}>
      <label class="field__label" for={props.id}>
        {props.label}
        {props.optional ? <span class="field__optional"> · optional</span> : null}
      </label>
      {props.children}
      {note ? (
        <div class={props.error ? "field__error" : "field__help"} id={`${props.id}-note`} role={props.error ? "alert" : undefined}>
          {note}
        </div>
      ) : null}
    </div>
  );
}
