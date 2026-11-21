import type { KitTone } from "./kitTone";
import { TaskRow, type TaskRowProps } from "./TaskRow";

export interface KitPanelProps {
  /** The kit's name as shown: "Rascal". */
  name: string;
  /** Its short line: "edit pictures". */
  line: string;
  tone: KitTone;
  tasks: TaskRowProps[];
  /** How many tools the kit offers, when known. */
  tools?: number;
  /** At most this many tasks are listed; the rest are counted in the last row. */
  shown?: number;
  onMore?: () => void;
}

/** What the kit panel's header counts: "6 tasks · 40 tools". */
export function kitCounts(tasks: number, tools?: number): string {
  const words = `${tasks} ${tasks === 1 ? "task" : "tasks"}`;
  return tools === undefined ? words : `${words} · ${tools} ${tools === 1 ? "tool" : "tools"}`;
}

/** A kit on Home: its mark, name and line, and its tasks. */
export function KitPanel(props: KitPanelProps) {
  const shown = props.shown ?? props.tasks.length;
  const listed = props.tasks.slice(0, shown);
  const rest = props.tasks.length - listed.length;
  return (
    <section class="kit-panel" aria-label={props.name}>
      <header class="kit-panel__head">
        <span class="kit-panel__identity">
          <span class={`kit-panel__mark tone-fill--${props.tone}`} aria-hidden="true">
            <svg width="14" height="14" viewBox="0 0 20 20">
              <path d="M4 13l6-9 6 9" fill="none" stroke="currentColor" stroke-width="2" stroke-linejoin="round" />
              <path d="M3 16h14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" />
            </svg>
          </span>
          <h2 class="kit-panel__name">{props.name}</h2>
          <span class="kit-panel__line">{props.line}</span>
        </span>
        <span class="kit-panel__counts mono">{kitCounts(props.tasks.length, props.tools)}</span>
      </header>
      {listed.length === 0 ? <p class="kit-panel__empty">This kit has no tasks in this build.</p> : null}
      {listed.map((task) => (
        <TaskRow key={task.title} {...task} />
      ))}
      {rest > 0 ? (
        <button type="button" class="kit-panel__more mono" onClick={props.onMore}>
          +{rest} {rest === 1 ? "task" : "tasks"} ›
        </button>
      ) : null}
    </section>
  );
}
