import { cx } from "./cx";
import type { KitTone } from "./kitTone";

export interface SendTarget {
  id: string;
  /** "Squint · Check an edit". */
  title: string;
  /** What the new draft gets: "Source and this result filled in". */
  line: string;
  tone: KitTone;
}

export interface SendToMenuProps {
  targets: SendTarget[];
  /** The target the keyboard is on. */
  active?: string;
  onSend?: (id: string) => void;
  onCopyCommand?: () => void;
  onDownload?: () => void;
}

/** "Send the result to…": each task the result's outputs fill, then copy and download. */
export function SendToMenu(props: SendToMenuProps) {
  return (
    <div class="menu send-to" role="menu" aria-label="Send the result to">
      <div class="menu__heading">
        Send the result to… <span class="menu__heading-note">opens a new draft</span>
      </div>
      {props.targets.length === 0 ? <div class="menu__empty">No task takes this result.</div> : null}
      {props.targets.map((target) => (
        <button
          key={target.id}
          type="button"
          role="menuitem"
          class={cx("menu__item", target.id === props.active && "menu__item--active")}
          onClick={() => props.onSend?.(target.id)}
        >
          <span class={`menu__swatch tone-fill--${target.tone}`} aria-hidden="true" />
          <span class="menu__text">
            <span class={cx("menu__title", target.id === props.active && "menu__title--strong")}>{target.title}</span>
            <span class="menu__line">{target.line}</span>
          </span>
        </button>
      ))}
      <div class="menu__foot menu__foot--split">
        {props.onCopyCommand ? (
          <button type="button" role="menuitem" class="link-button link-button--ink" onClick={props.onCopyCommand}>
            Copy as command
          </button>
        ) : (
          <span />
        )}
        {props.onDownload ? (
          <button type="button" role="menuitem" class="link-button link-button--ink" onClick={props.onDownload}>
            Download
          </button>
        ) : null}
      </div>
    </div>
  );
}
