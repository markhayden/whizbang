import type { KitTone } from "./kitTone";

export interface SentFromBannerProps {
  /** "Sent from Rascal · Remove, checked · attempt 2". */
  title: string;
  /** What was filled in and that nothing runs yet. */
  line: string;
  tone: KitTone;
  onBack?: () => void;
}

/** The banner of a draft that Send to opened, with the way back to the sending run. */
export function SentFromBanner(props: SentFromBannerProps) {
  return (
    <aside class="sent-from" aria-label="Sent from another run">
      <span class={`sent-from__swatch tone-fill--${props.tone}`} aria-hidden="true" />
      <span class="sent-from__text">
        <span class="sent-from__title">{props.title}</span>
        <span class="sent-from__line">{props.line}</span>
      </span>
      {props.onBack ? (
        <button type="button" class="link-button link-button--ink" onClick={props.onBack}>
          ‹ Back to the run
        </button>
      ) : null}
    </aside>
  );
}
