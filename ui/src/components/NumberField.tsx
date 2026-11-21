import { useState } from "preact/hooks";
import { Field } from "./Field";

export interface NumberFieldProps {
  id: string;
  label: string;
  /** The value, or null when the field is empty. */
  value: number | null;
  /** Whole numbers only. */
  integer?: boolean;
  min?: number;
  max?: number;
  optional?: boolean;
  help?: string;
  error?: string;
  disabled?: boolean;
  onInput?: (value: number | null) => void;
}

/** What a typed number says, or why it is not one this field takes. */
export function readNumber(text: string, integer: boolean, min?: number, max?: number): { value: number | null; error?: string } {
  const trimmed = text.trim();
  if (trimmed === "") return { value: null };
  const value = Number(trimmed);
  if (!Number.isFinite(value)) return { value: null, error: "Type a number." };
  if (integer && !Number.isInteger(value)) return { value: null, error: "Type a whole number." };
  if (min !== undefined && value < min) return { value: null, error: `At least ${min}.` };
  if (max !== undefined && value > max) return { value: null, error: `At most ${max}.` };
  return { value };
}

/** A number or an integer, held to its bounds; the bounds are said under it. */
export function NumberField(props: NumberFieldProps) {
  const bounds =
    props.min !== undefined && props.max !== undefined
      ? `${props.min} to ${props.max}`
      : props.min !== undefined
        ? `at least ${props.min}`
        : props.max !== undefined
          ? `at most ${props.max}`
          : undefined;
  const [text, setText] = useState(props.value === null ? "" : String(props.value));
  const [typed, setTyped] = useState<string | undefined>(undefined);
  const error = props.error ?? typed;
  return (
    <Field id={props.id} label={props.label} optional={props.optional} help={props.help ?? bounds} error={error} disabled={props.disabled}>
      <input
        id={props.id}
        class="control mono"
        type="text"
        inputMode={props.integer ? "numeric" : "decimal"}
        value={text}
        disabled={props.disabled}
        aria-invalid={error ? "true" : undefined}
        onInput={(event) => {
          const next = (event.currentTarget as HTMLInputElement).value;
          setText(next);
          const read = readNumber(next, Boolean(props.integer), props.min, props.max);
          setTyped(read.error);
          if (!read.error) props.onInput?.(read.value);
        }}
      />
    </Field>
  );
}
