import type { KitTone } from "./kitTone";

export interface BreadcrumbProps {
  kit: string;
  tone: KitTone;
  /** The task's title, or nothing on a kit's own page. */
  task?: string;
  onKit?: () => void;
  onTask?: () => void;
  onSearch?: () => void;
}

/** "Kit ▾ / Task ▾ ⌘K": the kit opens the switcher, the task its siblings, ⌘K search. */
export function Breadcrumb(props: BreadcrumbProps) {
  return (
    <nav class="breadcrumb" aria-label="Where you are">
      <button type="button" class="breadcrumb__kit" aria-haspopup="menu" onClick={props.onKit}>
        <span class={`breadcrumb__swatch tone-fill--${props.tone}`} aria-hidden="true" />
        <span class="breadcrumb__kit-name">{props.kit}</span>
        <span class="breadcrumb__caret" aria-hidden="true">
          ▾
        </span>
      </button>
      {props.task ? (
        <>
          <span class="breadcrumb__slash" aria-hidden="true">
            /
          </span>
          <button type="button" class="breadcrumb__task" aria-haspopup="menu" onClick={props.onTask}>
            <span>{props.task}</span>
            <span class="breadcrumb__caret" aria-hidden="true">
              ▾
            </span>
          </button>
        </>
      ) : null}
      <button type="button" class="breadcrumb__search mono" aria-label="Search" onClick={props.onSearch}>
        ⌘K
      </button>
    </nav>
  );
}
