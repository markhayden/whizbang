import { Tag, type TagProps } from "./Tag";

export interface ContinueRowProps {
  /** The task's title. */
  title: string;
  /** The kit and what was given: "squint · kitchen-wide.png". */
  what: string;
  tag: TagProps;
  /** When, already in words: "now", "12m ago". */
  when: string;
  thumb?: string;
  onOpen?: () => void;
}

/** A run still going, or a draft, on Home's "Continue". */
export function ContinueRow(props: ContinueRowProps) {
  return (
    <button type="button" class="continue-row" onClick={props.onOpen}>
      {props.thumb ? <img class="continue-row__thumb" src={props.thumb} alt="" /> : <span class="continue-row__thumb continue-row__thumb--empty" aria-hidden="true" />}
      <span class="continue-row__title">{props.title}</span>
      <span class="continue-row__what mono">{props.what}</span>
      <Tag {...props.tag} />
      <span class="continue-row__gap" />
      <span class="continue-row__when mono">{props.when}</span>
    </button>
  );
}
