import { Field } from "./Field";

export interface TextFieldProps {
  id: string;
  label: string;
  value: string;
  placeholder?: string;
  optional?: boolean;
  help?: string;
  error?: string;
  disabled?: boolean;
  /** At most this many characters. */
  maxLength?: number;
  /** A tag beside the label, such as FROM RASCAL. */
  from?: string;
  onInput?: (value: string) => void;
}

/** A line of text: words for a task, such as what to remove. */
export function TextField(props: TextFieldProps) {
  return (
    <Field id={props.id} label={props.label} optional={props.optional} help={props.help} error={props.error} disabled={props.disabled} from={props.from}>
      <input
        id={props.id}
        class="control"
        type="text"
        value={props.value}
        placeholder={props.placeholder}
        disabled={props.disabled}
        maxLength={props.maxLength}
        aria-invalid={props.error ? "true" : undefined}
        aria-describedby={props.error || props.help ? `${props.id}-note` : undefined}
        onInput={(event) => props.onInput?.((event.currentTarget as HTMLInputElement).value)}
      />
    </Field>
  );
}
