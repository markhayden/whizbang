import type { ComponentChildren } from "preact";
import { cx } from "./cx";

export type ButtonVariant = "primary" | "secondary" | "quiet" | "danger";

export interface ButtonProps {
  variant?: ButtonVariant;
  children: ComponentChildren;
  /** A key hint shown at the button's end, such as `⌘↵`. */
  hint?: string;
  disabled?: boolean;
  /** Why the button is disabled, said below it: a disabled button always says why. */
  why?: string;
  /** A danger button that was pressed and is waiting: "Stopping…". */
  busy?: boolean;
  onClick?: () => void;
  type?: "button" | "submit";
  title?: string;
}

/** Primary (Run), secondary (Send to), quiet (Clear) and danger (Stop run). */
export function Button(props: ButtonProps) {
  const variant = props.variant ?? "secondary";
  const disabled = Boolean(props.disabled || props.busy);
  const button = (
    <button
      type={props.type ?? "button"}
      class={cx("button", `button--${variant}`, props.busy && "button--busy", props.hint && "button--hinted")}
      disabled={disabled}
      aria-busy={props.busy ? "true" : undefined}
      title={props.title}
      onClick={disabled ? undefined : props.onClick}
    >
      <span class="button__label">{props.children}</span>
      {props.hint && !disabled ? <span class="button__hint mono">{props.hint}</span> : null}
    </button>
  );
  if (!props.disabled || !props.why) return button;
  return (
    <span class="button-with-why">
      {button}
      <span class="button__why" role="note">
        {props.why}
      </span>
    </span>
  );
}
