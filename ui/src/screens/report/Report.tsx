// A run's report, shown in place (plan m5, task W3, items 3 to 5): the document the host
// made from the record, in the isolated frame, with "Export", which downloads it as one
// file that opens alone, and "Export run", the record and outputs as one archive.
import { useEffect, useState } from "preact/hooks";
import { ApiError, type Client, type Hello } from "../../api";
import { Button } from "../../components/Button";
import { IsolatedFrame } from "../../isolated";
import { hrefOf } from "../../routes";
import { reportOf, save, taskOfReport } from "../history/calls";

export interface ReportProps {
  id: string;
  hello?: Hello | null;
  client: Client;
}

export function Report(props: ReportProps) {
  const [html, setHtml] = useState<string | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  useEffect(() => {
    let live = true;
    setHtml(null);
    setProblem(null);
    reportOf(props.client, props.id).then(
      (made) => live && setHtml(made),
      (error: unknown) => live && setProblem(error instanceof ApiError ? error.message : String(error)),
    );
    return () => {
      live = false;
    };
  }, [props.id]);
  const task = (html && taskOfReport(html)) ?? "run";
  const exportRun = () =>
    props.client.bytes(`/api/runs/${encodeURIComponent(props.id)}/export`).then(
      (blob) => save(blob, `${task}-${props.id}.tar`),
      (error: unknown) => setProblem(error instanceof ApiError ? error.message : String(error)),
    );
  return (
    <main class="screen report" aria-labelledby="report-title">
      <header class="report__head">
        <div>
          <h1 id="report-title" class="screen__title">
            Report
          </h1>
          <p class="report__run mono">{props.id}</p>
        </div>
        <div class="report__actions">
          <a class="screen__link" href={hrefOf({ name: "run", id: props.id })}>
            Back to the run
          </a>
          <Button disabled={html === null} why={html === null ? "The report is not here yet." : undefined} onClick={() => html !== null && save(new Blob([html], { type: "text/html" }), `${task}-${props.id}.html`)}>
            Export
          </Button>
          <Button variant="quiet" onClick={exportRun}>
            Export run
          </Button>
        </div>
      </header>
      {problem ? (
        <p class="report__problem" role="alert">
          {problem}
        </p>
      ) : null}
      {html !== null ? <IsolatedFrame html={html} title={`The report of run ${props.id}`} /> : problem ? null : <p class="empty">Making the report…</p>}
    </main>
  );
}
