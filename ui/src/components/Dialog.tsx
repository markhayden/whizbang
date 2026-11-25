import type { ComponentChildren } from "preact";
import { useEffect, useRef } from "preact/hooks";

export interface DialogProps {
  /** The dialog's title, and the id its heading is named by. */
  title: string;
  id: string;
  /** A line under the title. */
  line?: string;
  children: ComponentChildren;
  /** The buttons at its foot. */
  foot: ComponentChildren;
  onCancel?: () => void;
  /** A class of its own beside `menu dialog`. */
  class?: string;
}

/**
 * A modal dialog (plan m7, task U2): its head, its body and its foot. While it is open
 * Escape cancels, no key reaches the page behind it, and focus goes to its first control;
 * when it closes, focus returns where it was.
 */
export function Dialog(props: DialogProps) {
  const box = useRef<HTMLDivElement>(null);
  const cancel = useRef(props.onCancel);
  cancel.current = props.onCancel;
  useEffect(() => {
    const before = document.activeElement as HTMLElement | null;
    box.current?.querySelector<HTMLElement>("input, button:not([disabled])")?.focus();
    const onKey = (event: KeyboardEvent) => {
      const dialog = box.current;
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
    return () => {
      window.removeEventListener("keydown", onKey, true);
      before?.focus?.();
    };
  }, []);
  return (
    <div class="palette-scrim" onClick={props.onCancel}>
      <div ref={box} class={`menu dialog ${props.class ?? ""}`} role="dialog" aria-modal="true" aria-labelledby={props.id} onClick={(event) => event.stopPropagation()}>
        <header class="dialog__head">
          <h2 id={props.id} class="dialog__title">
            {props.title}
          </h2>
          {props.line ? <p class="dialog__line mono">{props.line}</p> : null}
        </header>
        <div class="dialog__body">{props.children}</div>
        <footer class="dialog__foot">{props.foot}</footer>
      </div>
    </div>
  );
}
