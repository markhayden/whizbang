import { reportTitle, type ReplayReport as Report } from "../bundles";
import { dollars, middle } from "../format";
import "./bundles.css";

export interface ReplayReportProps {
  report: Report;
  /** Where a "Compare side by side" link goes, when the two runs can be compared. */
  compareHref?: string;
}

/** The report's chip, as the design's component "Replay report" says it. */
export function ReportChip(props: { report: Report }) {
  const words = props.report.report === "differs" ? `DIFFERS · ${props.report.differences.length + props.report.more}` : props.report.report.toUpperCase();
  return <span class={`tag tag--soft report-chip report-chip--${props.report.report} mono`}>{words}</span>;
}

/**
 * A replay's report at the top of its result (plan m7, task U2, item 3; the design's
 * surface 5, B): reproduced, differs (each difference by place), diverged, or unanswerable.
 */
export function ReplayReport(props: ReplayReportProps) {
  const report = props.report;
  const meta = `${report.mode} · ${middle(report.replay, 14)} of ${middle(report.of, 14)}${report.mode === "live" ? ` · ${dollars(report.spent_micros / 1e6)}` : ""}`;
  return (
    <section class={`replay-report replay-report--${report.report}`} aria-label="Replay report">
      <header class="replay-report__head">
        <h2 class="replay-report__title">{reportTitle(report)}</h2>
        <span class="replay-report__meta mono">{meta}</span>
      </header>
      {report.report === "reproduced" ? <p class="replay-report__line">{`Every outcome, output and measurement matches the original. ${dollars(report.spent_micros / 1e6)} spent.`}</p> : null}
      {report.report === "differs" ? (
        <ol class="replay-report__differences mono">
          {report.differences.map((one, at) => (
            <li key={at}>{`${one.place} · ${one.what} · ${middle(one.original, 24)} → ${middle(one.replay, 24)}`}</li>
          ))}
          {report.more > 0 ? <li>{`and ${report.more} more`}</li> : null}
        </ol>
      ) : null}
      {report.stopped ? (
        <p class="replay-report__line">
          {report.report === "diverged"
            ? `The replay asked the ${report.stopped.capability} something the original never asked, so there was no recorded answer to give. Nothing was called. ${report.stopped.why}`
            : report.stopped.why}
        </p>
      ) : null}
      {report.settings.length > 0 ? (
        <p class="replay-report__setting">{`Settings changed since: ${report.settings.map((one) => `${one.what} ${one.original} → ${one.replay}`).join("; ")}`}</p>
      ) : null}
      {props.compareHref ? (
        <a class="replay-report__link" href={props.compareHref}>
          Compare side by side
        </a>
      ) : null}
    </section>
  );
}
