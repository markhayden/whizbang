import { useEffect, useState } from "preact/hooks";
import { ApiError, tasksOf, type Client, type Hello, type Polled, type RunOutput } from "../../api";
import { Button } from "../../components/Button";
import { Canvas } from "../../components/Canvas";
import { Segmented } from "../../components/Segmented";
import { SpendMeter } from "../../components/SpendMeter";
import { StepList } from "../../components/StepList";
import { Tag } from "../../components/Tag";
import { attemptOf, endTag, fieldsOf, hear, labelOf, stepsOf, taskOf, type Heard } from "../../form";
import { ago, middle } from "../../format";
import { useArtifact, useImageSize, useNow } from "../../hooks";
import { rememberOutputs } from "../../outputs";
import { hrefOf } from "../../routes";
import { EmptyScreen } from "../Empty";
import { StoredPicture } from "../shared";

export interface RunProps {
  id: string;
  hello?: Hello | null;
  client?: Client;
}

/** What the screen keeps of a run as it is polled. */
interface Followed {
  last: Polled | null;
  heard: Heard;
  action: string | null;
  arguments: Record<string, unknown> | null;
}

const IDENTITY = /^sha256:[0-9a-f]{64}$/;

/** A run: going, with its steps and its spend, or ended, with its outputs (plan m5, task W1, item 6). */
export function Run(props: RunProps) {
  if (!props.client) {
    return (
      <EmptyScreen title={`Run ${props.id}`} link={{ href: "#/", words: "Back to Home" }}>
        The page cannot reach the host to show this run.
      </EmptyScreen>
    );
  }
  return <Following key={props.id} id={props.id} hello={props.hello ?? null} client={props.client} />;
}

function Following(props: { id: string; hello: Hello | null; client: Client }) {
  const [followed, setFollowed] = useState<Followed>({ last: null, heard: [], action: null, arguments: null });
  const [problem, setProblem] = useState<ApiError | null>(null);
  const [stopping, setStopping] = useState(false);
  const [view, setView] = useState("result");
  const [zoom, setZoom] = useState<number | "fit">("fit");

  useEffect(() => {
    let after = 0;
    const stop = props.client.pollRun(
      props.id,
      (answer) => {
        setProblem(null);
        setFollowed((now) => {
          const heard = hear(now.heard, Array.isArray(answer.events) ? answer.events : [], answer.skipped ?? 0, after);
          after = answer.next;
          return { last: answer, heard, action: now.action ?? answer.action ?? null, arguments: now.arguments ?? answer.arguments ?? null };
        });
      },
      (error) => setProblem(error),
    );
    return () => stop?.();
  }, [props.id]);

  const last = followed.last;
  const going = last ? last.state === "running" || last.state === "stopping" : true;
  const now = useNow(going);
  const tasks = tasksOf(props.hello);
  const task = taskOf(tasks, followed.action);
  const ended = last !== null && !going;
  const outputs: RunOutput[] = Array.isArray(last?.outputs) ? last.outputs : [];

  useEffect(() => {
    if (ended && outputs.length > 0) rememberOutputs(props.id, task?.title ?? followed.action ?? "", outputs, ago(Date.now(), Date.now()));
  }, [ended]);

  if (!last && problem) {
    return (
      <EmptyScreen title={`Run ${middle(props.id, 24)}`} link={{ href: "#/", words: "Back to Home" }}>
        {problem.status === 404 ? "No run has that id in this store." : problem.message}
      </EmptyScreen>
    );
  }

  const fields = fieldsOf(task);
  const args = followed.arguments ?? {};
  const pictureField = fields.find((field) => field.kind === "picture") ?? fields.find((field) => field.kind === "pictures");
  const inputValue = pictureField ? args[pictureField.name] : undefined;
  const inputId = typeof inputValue === "string" ? inputValue : Array.isArray(inputValue) && typeof inputValue[0] === "string" ? inputValue[0] : null;
  const resultId = outputs.find((output) => output.type === "image")?.id ?? null;
  const maskId = outputs.find((output) => output.type === "mask")?.id ?? null;
  const views = [
    ...(inputId ? [{ value: "original", label: "Original" }] : []),
    ...(maskId ? [{ value: "mask", label: "Mask" }] : []),
    ...(resultId ? [{ value: "result", label: "Result" }] : []),
  ];
  const chosen = views.find((one) => one.value === view)?.value ?? (resultId ? "result" : (views[0]?.value ?? "original"));
  const shownId = chosen === "result" ? resultId : inputId;
  const waiting = task?.kind === "recipe" && Array.isArray(task.tools) ? task.tools : [];
  const entries = stepsOf(followed.heard, waiting, now, going);
  const attempt = attemptOf(followed.heard);
  const tag = ended ? endTag(last?.outcome, last?.verdict) : null;
  const backHref = task ? hrefOf({ name: "kit", kit: task.kit ?? "tools", task: task.name }) : "#/";

  const stop = () => {
    setStopping(true);
    props.client.cancelRun(props.id).catch((error: unknown) => {
      setStopping(false);
      setProblem(error instanceof ApiError ? error : new ApiError(0, String(error)));
    });
  };

  return (
    <main class="workspace" aria-labelledby="run-title">
      <section class="workspace__left">
        <header class="workspace__head">
          <div class="workspace__title-row">
            <h1 id="run-title" class="workspace__title">
              {task?.title ?? followed.action ?? "Run"}
            </h1>
            {tag ? <Tag kind={tag} /> : <Tag kind="running" detail={last?.state === "stopping" || stopping ? "stopping" : undefined} />}
          </div>
          <p class="workspace__purpose mono">run {props.id}</p>
        </header>
        <dl class="run__inputs" aria-label="Inputs">
          {Object.entries(args).map(([name, value]) => {
            const field = fields.find((one) => one.name === name);
            const label = field ? labelOf(field) : name;
            const ids = (Array.isArray(value) ? value : [value]).filter((one): one is string => typeof one === "string" && IDENTITY.test(one));
            return (
              <div class="run__input" key={name}>
                <dt class="field__label">{label}</dt>
                <dd class="run__value">
                  {ids.length > 0 && (field?.kind === "picture" || field?.kind === "pictures")
                    ? ids.map((id) => <StoredPicture key={id} client={props.client} picture={{ id, name: middle(id, 20) }} />)
                    : ids.length > 0
                      ? ids.map((id) => middle(id, 24)).join(", ")
                      : typeof value === "string" || typeof value === "number" || typeof value === "boolean"
                        ? String(value)
                        : ""}
                </dd>
              </div>
            );
          })}
        </dl>
        <footer class="workspace__foot">
          {(last?.reserved ?? 0) > 0 || (last?.spent ?? 0) > 0 ? <SpendMeter title="Settled so far" spent={last?.spent ?? 0} reserved={last?.reserved ?? 0} /> : <p class="empty mono">nothing reserved: free</p>}
          {problem ? (
            <p class="field__error" role="alert">
              {problem.message}
            </p>
          ) : null}
          {going ? (
            <Button variant="danger" busy={stopping || last?.state === "stopping"} onClick={stop}>
              {stopping || last?.state === "stopping" ? "Stopping…" : "Stop run"}
            </Button>
          ) : (
            <a class="screen__link" href={backHref}>
              Back to the task
            </a>
          )}
        </footer>
      </section>
      <section class="workspace__centre" aria-label="Canvas">
        <div class="workspace__tabs">{views.length > 0 ? <Segmented label="View" segments={views} value={chosen} onChange={setView} /> : null}</div>
        <Shown client={props.client} id={shownId} mask={chosen === "mask" ? maskId : null} zoom={zoom} onZoom={setZoom} name={chosen} />
      </section>
      <aside class="workspace__right">
        <div class="run__steps-head">
          <h2 class="section-title">Steps</h2>
          {attempt > 0 ? <span class="mono run__attempt">attempt {attempt}</span> : null}
        </div>
        {entries.length === 0 ? <p class="empty">{going ? "Waiting for the first step." : "This run said no steps."}</p> : <StepList entries={entries} />}
      </aside>
    </main>
  );
}

/** The picture of the view chosen, with a mask over it when the view is the mask. */
function Shown(props: { client: Client; id: string | null; mask: string | null; zoom: number | "fit"; onZoom: (zoom: number | "fit") => void; name: string }) {
  const src = useArtifact(props.client, props.id);
  const maskSrc = useArtifact(props.client, props.mask);
  const size = useImageSize(src);
  if (!props.id) return <p class="empty workspace__nothing">Nothing to show yet.</p>;
  if (!src || !size) return <p class="empty workspace__nothing">Loading the picture…</p>;
  return (
    <Canvas
      src={src}
      name={props.name === "result" ? "the result" : "the original"}
      width={size.width}
      height={size.height}
      marks={maskSrc ? [{ kind: "mask", src: maskSrc, label: "mask" }] : []}
      zoom={props.zoom}
      onZoom={props.onZoom}
    />
  );
}
