import { dollars } from "../format";
import { cx } from "./cx";
import type { KitTone } from "./kitTone";
import { Tag, type TagProps } from "./Tag";

export interface HistoryRowProps {
  task: string;
  tone: KitTone;
  /** What was asked: its words and the picture's name. */
  asked: string;
  tag: TagProps;
  /** The models that served it, in words. */
  models?: string;
  /** Elapsed time, in words. */
  time?: string;
  cost: number;
  when: string;
  thumb?: string;
  selected?: boolean;
  onOpen?: () => void;
}

/** A run in History: its task, what was asked, result, models, time, cost and when. */
export function HistoryRow(props: HistoryRowProps) {
  return (
    <button type="button" class={cx("history-row", props.selected && "history-row--selected")} aria-pressed={props.selected ? "true" : "false"} onClick={props.onOpen}>
      {props.thumb ? <img class="history-row__thumb" src={props.thumb} alt="" /> : <span class="history-row__thumb" aria-hidden="true" />}
      <span class="history-row__task">
        <span class={`history-row__swatch tone-fill--${props.tone}`} aria-hidden="true" />
        <span>{props.task}</span>
      </span>
      <span class="history-row__asked">{props.asked}</span>
      <span class="history-row__tag">
        <Tag {...props.tag} soft />
      </span>
      <span class="history-row__models mono">{props.models ?? ""}</span>
      <span class="history-row__time mono">{props.time ?? ""}</span>
      <span class="history-row__cost mono">{dollars(props.cost)}</span>
      <span class="history-row__gap" />
      <span class="history-row__when mono">{props.when}</span>
    </button>
  );
}
