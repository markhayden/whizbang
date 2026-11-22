// A tool's or task's record in place (SPEC.md section 8; plan m5, task W3, item 7): the
// page `docs/reference/tools/<name>.md` the binary embeds, its pictures fetched with the
// token and inlined, made HTML by the one wrapper and shown in the isolated frame.
import { useEffect, useState } from "preact/hooks";
import { ApiError, tasksOf, type Client, type Hello } from "../../api";
import { IsolatedFrame, picturesOf, recordDocument } from "../../isolated";
import { hrefOf } from "../../routes";
import { recordOf } from "../history/calls";

export interface RecordProps {
  name: string;
  hello?: Hello | null;
  client: Client;
}

export function Record(props: RecordProps) {
  const [html, setHtml] = useState<string | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  useEffect(() => {
    let live = true;
    setHtml(null);
    setProblem(null);
    recordOf(props.client, props.name, picturesOf).then(
      ({ markdown, pictures }) => live && setHtml(recordDocument(props.name, markdown, pictures)),
      (error: unknown) =>
        live && setProblem(error instanceof ApiError && error.status === 404 ? `${props.name} has no record in this build.` : error instanceof ApiError ? error.message : String(error)),
    );
    return () => {
      live = false;
    };
  }, [props.name]);
  const task = tasksOf(props.hello).find((one) => one.name === props.name);
  return (
    <main class="screen record" aria-labelledby="record-title">
      <header class="report__head">
        <div>
          <h1 id="record-title" class="screen__title">
            {task?.title ?? props.name}
          </h1>
          <p class="report__run mono">the record of {props.name}</p>
        </div>
        {task && task.kind !== "primitive" && task.kit ? (
          <a class="screen__link" href={hrefOf({ name: "kit", kit: task.kit, task: task.name })}>
            Open the task
          </a>
        ) : null}
      </header>
      {problem ? (
        <p class="report__problem" role="alert">
          {problem}
        </p>
      ) : null}
      {html !== null ? <IsolatedFrame html={html} title={`The record of ${props.name}`} /> : problem ? null : <p class="empty">Reading the record…</p>}
    </main>
  );
}
