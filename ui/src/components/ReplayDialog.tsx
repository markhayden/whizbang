import { useEffect, useState } from "preact/hooks";
import { ApiError, newRequestId, type Client, type Prepared } from "../api";
import { startReplay } from "../bundles";
import { dollars } from "../format";
import { Button } from "./Button";
import { Dialog } from "./Dialog";
import "./bundles.css";

export interface ReplayDialogProps {
  client: Client;
  run: string;
  /** The run's task, as a person reads it. */
  title: string;
  /** The draft Run again gives: what a live replay's cost is reviewed from. */
  again?: { task: string; values: Record<string, unknown> } | null;
  onStarted: (run: string) => void;
  onCancel: () => void;
}

/**
 * "Replay…" (plan m7, task U2, item 3; the design's surface 5, A): offline, every model's
 * answer the one the run recorded, nothing called; or live, under the config as it is now,
 * its cost reviewed first as any paid run's. Always offered: when the engine refuses the
 * replay, the dialog says why in its words (the plan's G1 table).
 */
export function ReplayDialog(props: ReplayDialogProps) {
  const [live, setLive] = useState(false);
  const [plan, setPlan] = useState<Prepared | null>(null);
  const [planProblem, setPlanProblem] = useState<string | null>(null);
  const [starting, setStarting] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const [requestId] = useState(() => newRequestId());
  useEffect(() => {
    if (!live || plan || !props.again) return;
    let current = true;
    props.client.prepare(props.again.task, props.again.values).then(
      (answer) => current && setPlan(answer),
      (error: unknown) => current && setPlanProblem(error instanceof ApiError ? error.message : String(error)),
    );
    return () => {
      current = false;
    };
  }, [live]);
  const bound = plan?.bound ?? 0;
  const start = () => {
    setStarting(true);
    setProblem(null);
    startReplay(props.client, props.run, live, requestId).then(
      (started) => props.onStarted(started.run),
      (error: unknown) => {
        setStarting(false);
        setProblem(error instanceof ApiError ? error.message : String(error));
      },
    );
  };
  const liveWhy = live && !plan && !planProblem ? "The cost is being reviewed." : undefined;
  return (
    <Dialog
      id="replay-title"
      title={`Replay run ${props.run}`}
      line={`${props.title} · a new run, compared with this one`}
      class="replay-dialog"
      onCancel={props.onCancel}
      foot={
        <>
          <Button variant="quiet" onClick={props.onCancel}>
            Cancel
          </Button>
          <Button variant="primary" disabled={Boolean(liveWhy)} why={liveWhy} busy={starting} onClick={start}>
            {live ? `Replay live · up to ${dollars(bound)}` : "Replay offline"}
          </Button>
        </>
      }
    >
      <div class="replay-dialog__modes" role="radiogroup" aria-label="How to replay">
        <label class={live ? "replay-mode" : "replay-mode replay-mode--chosen"}>
          <input type="radio" name="replay-mode" checked={!live} onChange={() => setLive(false)} />
          <span class="replay-mode__text">
            <span class="replay-mode__name">
              Offline <span class="replay-mode__free mono">free</span>
            </span>
            <span class="replay-mode__line">Every model answer comes from what this run recorded. Nothing is called or spent; the pixel work runs again.</span>
          </span>
        </label>
        <label class={live ? "replay-mode replay-mode--chosen" : "replay-mode"}>
          <input type="radio" name="replay-mode" checked={live} onChange={() => setLive(true)} />
          <span class="replay-mode__text">
            <span class="replay-mode__name">
              Live <span class="replay-mode__free mono">{plan ? `up to ${dollars(bound)}` : "may spend"}</span>
            </span>
            <span class="replay-mode__line">Calls the models again under today's setup and spends like any run. A model's verdict isn't promised to repeat.</span>
          </span>
        </label>
      </div>
      {live && plan ? (
        <div class="replay-dialog__cost" aria-label="Live, with the cost reviewed">
          <p class="replay-dialog__cost-title">Live, with the cost reviewed</p>
          {(plan.needs ?? []).map((need) => (
            <p key={need.capability} class="mono replay-dialog__need">{`${need.capability} ${need.backend}${need.model ? ` · ${need.model}` : ""} ${need.local ? "free" : `up to ${dollars(need.bound)}`}`}</p>
          ))}
          {plan.ready ? null : <p class="replay-dialog__note">{plan.why ?? "A backend it needs is not ready here; the replay says why when it is asked."}</p>}
        </div>
      ) : null}
      {planProblem ? <p class="field__error">{planProblem}</p> : null}
      {problem ? (
        <p class="field__error replay-dialog__refused" role="alert">
          {problem}
        </p>
      ) : null}
    </Dialog>
  );
}
