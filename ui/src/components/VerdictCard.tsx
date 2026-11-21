export type Verdict = "pass" | "fail" | "unsure";

export interface VerdictCardProps {
  verdict: Verdict;
  /** The verdict in words: "Check passed", "The check still sees it", "Not sure". */
  title: string;
  /** Which try and which judge: "try 2 · Gemini". */
  detail?: string;
  /** The judge's reasons, each shown as text. */
  reasons?: string[];
}

/** Squint's verdict: pass, fail or unsure, its words and its reasons. */
export function VerdictCard(props: VerdictCardProps) {
  return (
    <section class={`verdict verdict--${props.verdict}`} aria-label="Verdict">
      <div class="verdict__head">
        <span class="verdict__swatch" aria-hidden="true">
          {props.verdict === "pass" ? "✓" : ""}
        </span>
        <span class="verdict__title">{props.title}</span>
        {props.detail ? <span class="verdict__detail">{props.detail}</span> : null}
      </div>
      {props.reasons && props.reasons.length > 0 ? (
        <ul class="verdict__reasons">
          {props.reasons.map((reason, index) => (
            <li key={index}>{reason}</li>
          ))}
        </ul>
      ) : null}
    </section>
  );
}
