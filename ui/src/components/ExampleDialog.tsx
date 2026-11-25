import { useEffect, useRef, useState } from "preact/hooks";
import { Button } from "./Button";

export interface ExampleDialogProps {
  /** Where it is saved from: "run r-8c21 · Remove, checked". */
  from: string;
  /** What the example keeps, in words. */
  keeps: string;
  /** The most characters of the name and of the notes. */
  nameLimit: number;
  notesLimit: number;
  saving?: boolean;
  problem?: string;
  onSave?: (name: string, notes: string) => void;
  onCancel?: () => void;
}

/**
 * "Save as example" (plan m7, task U1, item 2; the design's surface 2, A): a name and
 * optional notes for a run kept as a starting point, and what it keeps. Modal while open:
 * Escape cancels, and no key reaches the page behind it.
 */
export function ExampleDialog(props: ExampleDialogProps) {
  const [name, setName] = useState("");
  const [notes, setNotes] = useState("");
  const keep = useRef<HTMLFormElement>(null);
  const cancel = useRef(props.onCancel);
  cancel.current = props.onCancel;
  useEffect(() => {
    keep.current?.querySelector<HTMLInputElement>("input")?.focus();
    const onKey = (event: KeyboardEvent) => {
      const dialog = keep.current;
      if (!dialog) return;
      event.stopPropagation();
      if (event.key === "Escape") {
        event.preventDefault();
        cancel.current?.();
        return;
      }
      if ((event.metaKey || event.ctrlKey || event.altKey) && !dialog.contains(event.target as Node)) event.preventDefault();
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, []);
  const count = Array.from(name).length;
  const empty = name.trim() === "";
  const why = empty ? "An example has a name." : count > props.nameLimit ? `A name is at most ${props.nameLimit} characters.` : undefined;
  return (
    <div class="palette-scrim" onClick={props.onCancel}>
      <form
        ref={keep}
        class="menu example-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="example-title"
        onClick={(event) => event.stopPropagation()}
        onSubmit={(event) => {
          event.preventDefault();
          if (!why && !props.saving) props.onSave?.(name, notes);
        }}
      >
        <header class="example-dialog__head">
          <h2 id="example-title" class="example-dialog__title">
            Save as example
          </h2>
          <p class="example-dialog__from mono">{props.from}</p>
        </header>
        <div class="example-dialog__body">
          <label class="example-dialog__field">
            <span class="field__label">Name</span>
            <span class="example-dialog__control">
              <input class="control" type="text" value={name} maxLength={props.nameLimit} onInput={(event) => setName((event.currentTarget as HTMLInputElement).value)} />
              <span class="example-dialog__count mono">{`${count}/${props.nameLimit}`}</span>
            </span>
          </label>
          <label class="example-dialog__field">
            <span class="field__label">
              Notes <span class="field__optional">optional</span>
            </span>
            <textarea class="control example-dialog__notes" value={notes} maxLength={props.notesLimit} rows={3} onInput={(event) => setNotes((event.currentTarget as HTMLTextAreaElement).value)} />
          </label>
          <p class="example-dialog__keeps mono">
            {`keeps: ${props.keeps}`}
            <br />
            runs nothing when opened
          </p>
          {props.problem ? (
            <p class="field__error" role="alert">
              {props.problem}
            </p>
          ) : null}
        </div>
        <footer class="example-dialog__foot">
          <Button variant="quiet" onClick={props.onCancel}>
            Cancel
          </Button>
          <Button variant="primary" type="submit" disabled={Boolean(why)} why={why} busy={props.saving}>
            Save example
          </Button>
        </footer>
      </form>
    </div>
  );
}
