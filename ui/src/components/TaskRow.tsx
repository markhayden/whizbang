import { cx } from "./cx";

export interface TaskRowProps {
  /** The task's title, as a person reads it: "Remove what you name". */
  title: string;
  /** What it takes and gives, in words: "image + words → image". */
  shape: string;
  /** The other kit a loop uses, said after the title: "uses Squint". */
  uses?: string;
  href?: string;
  onOpen?: () => void;
}

/** A task in a kit panel: its title, the kit it uses, and its shape. */
export function TaskRow(props: TaskRowProps) {
  const body = (
    <>
      <span class="task-row__title">
        {props.title}
        {props.uses ? <span class="task-row__uses"> · uses {props.uses}</span> : null}
      </span>
      <span class="task-row__shape mono">{props.shape}</span>
    </>
  );
  if (props.href) {
    return (
      <a class={cx("task-row")} href={props.href} onClick={props.onOpen}>
        {body}
      </a>
    );
  }
  return (
    <button type="button" class="task-row" onClick={props.onOpen}>
      {body}
    </button>
  );
}
