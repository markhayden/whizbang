import { useEffect, useRef } from "preact/hooks";
import { Button } from "./Button";

export interface QuitDialogProps {
  /** How many runs the host started are still going; none while it is being read. */
  going: number | null;
  /** Quit was pressed and the host has not answered yet. */
  quitting?: boolean;
  /** Why the host could not be asked to quit, when it could not. */
  problem?: string;
  onQuit?: () => void;
  onCancel?: () => void;
}

/** What quitting does to the runs going, in words (ADR 0041, "Stopping"). */
export function quitWords(going: number | null): string {
  if (going === null) return "Any run still going is cancelled.";
  if (going === 0) return "Nothing is running, so no run is cancelled.";
  return `${going === 1 ? "The run" : `The ${going} runs`} still going ${going === 1 ? "is" : "are"} cancelled.`;
}

/**
 * "Quit the playground?": what quitting does, said before it is done. The host stops; each
 * run going is cancelled, its record says so and every request it sent is accounted for in
 * the ledger; a run that does not end within the grace is ended as interrupted later.
 */
export function QuitDialog(props: QuitDialogProps) {
  const keep = useRef<HTMLDivElement>(null);
  useEffect(() => {
    keep.current?.querySelector<HTMLButtonElement>(".button--quiet")?.focus();
  }, []);
  return (
    <div class="palette-scrim" onClick={props.onCancel}>
      <div
        ref={keep}
        class="menu quit-dialog"
        role="alertdialog"
        aria-modal="true"
        aria-labelledby="quit-title"
        aria-describedby="quit-what"
        onClick={(event) => event.stopPropagation()}
        onKeyDown={(event) => {
          if (event.key === "Escape") props.onCancel?.();
        }}
      >
        <h2 id="quit-title" class="quit-dialog__title">
          Quit the playground?
        </h2>
        <div id="quit-what" class="quit-dialog__what">
          <p>The host stops, and this page stops working until you run whizbang play again.</p>
          <p class="quit-dialog__going">{quitWords(props.going)}</p>
          {props.going !== 0 ? (
            <p>
              Each run cancelled has a record that says cancelled, and every request it sent is accounted for in the ledger, so nothing stays reserved. A run that has not ended
              within ten seconds is ended as interrupted the next time the store is opened.
            </p>
          ) : null}
          <p>Your pictures, history and drafts stay.</p>
        </div>
        {props.problem ? (
          <p class="field__error" role="alert">
            {props.problem}
          </p>
        ) : null}
        <div class="quit-dialog__actions">
          <Button variant="quiet" onClick={props.onCancel}>
            Keep playing
          </Button>
          <Button variant="danger" busy={props.quitting} onClick={props.onQuit}>
            {props.quitting ? "Quitting…" : "Quit"}
          </Button>
        </div>
      </div>
    </div>
  );
}
