import { cx } from "./cx";

/** An output of an earlier run that a port of this type can take. */
export interface Binding {
  /** The artifact's identity. */
  id: string;
  /** Its type: "mask", "location", "band_profile", "series", "plan", … */
  type: string;
  /** The run that made it, and its task, as text. */
  run: string;
  task: string;
  /** When, already in words. */
  when: string;
  thumb?: string;
}

export interface BindingPickerProps {
  /** The type the port takes. */
  type: string;
  /** Earlier outputs; only those of the port's type are offered. */
  bindings: Binding[];
  value: string | null;
  disabled?: boolean;
  onPick?: (id: string) => void;
  /** Why the outputs of earlier runs could not be read, said in place of the list. */
  error?: string;
  /** Asks for the outputs of earlier runs past the last shown, when there may be more. */
  onMore?: () => void;
}

/** The outputs of earlier runs of one type, newest first, for a port of that type. */
export function BindingPicker(props: BindingPickerProps) {
  const offered = props.bindings.filter((binding) => binding.type === props.type);
  const failed = props.error ? (
    <p class="field__error" role="alert">
      {`Earlier ${props.type.replace(/_/g, " ")} outputs could not be read: ${props.error}`}
    </p>
  ) : null;
  if (offered.length === 0 && failed) return failed;
  if (offered.length === 0) {
    return <p class="binding-picker__empty">No earlier run made a {props.type.replace(/_/g, " ")} yet.</p>;
  }
  const more = props.onMore ? (
    <button type="button" class="link-button" onClick={props.onMore}>
      More {props.type.replace(/_/g, " ")} outputs
    </button>
  ) : null;
  return (
    <>
    <ul class="binding-picker" role="listbox" aria-label={`Earlier ${props.type.replace(/_/g, " ")} outputs`}>
      {offered.map((binding) => {
        const selected = binding.id === props.value;
        return (
          <li key={binding.id} role="option" aria-selected={selected ? "true" : "false"}>
            <button type="button" class={cx("binding-picker__row", selected && "binding-picker__row--selected")} disabled={props.disabled} onClick={() => props.onPick?.(binding.id)}>
              {binding.thumb ? <img class="binding-picker__thumb" src={binding.thumb} alt="" /> : <span class="binding-picker__thumb" aria-hidden="true" />}
              <span class="binding-picker__task">{binding.task}</span>
              <span class="binding-picker__run mono">{binding.run}</span>
              <span class="binding-picker__when mono">{binding.when}</span>
            </button>
          </li>
        );
      })}
    </ul>
    {more}
    {failed}
    </>
  );
}
