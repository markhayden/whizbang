import { useEffect, useState } from "preact/hooks";
import { ApiError, type Client } from "../api";
import { bundleChain, draftOfStep, openStep, saveChain, showBundle, type BundleSummary, type ChainPlace, type OpenedStep } from "../bundles";
import { localStore, saveDraft } from "../drafts";
import { middle } from "../format";
import { hrefOf } from "../routes";
import { save } from "../screens/history/calls";
import { Button } from "./Button";
import "./bundles.css";

export interface ChainCardProps {
  client: Client;
  place: ChainPlace;
  /** The step this result is the new run of. */
  step: number;
  run: string;
  go: (hash: string) => void;
}

/**
 * A chain's card at the top of a step's result (plan m7, task U2, item 2; the design's
 * surface 4b): Continue, which opens the next step as a draft with its linked inputs bound
 * to this run's outputs; the chain stopped where a link finds no output, or two; and the
 * end of the chain.
 */
export function ChainCard(props: ChainCardProps) {
  const [bundle, setBundle] = useState<BundleSummary | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [blocked, setBlocked] = useState<OpenedStep | null>(null);
  const [busy, setBusy] = useState(false);
  const [stopped, setStopped] = useState(props.place.stopped);
  const digest = props.place.digest;
  useEffect(() => {
    let live = true;
    showBundle(props.client, digest).then(
      (answer) => live && setBundle(answer),
      (error: unknown) => live && setProblem(error instanceof ApiError ? error.message : String(error)),
    );
    return () => {
      live = false;
    };
  }, [digest]);
  const steps = bundle?.steps ?? [];
  const named = (at: number) => steps[at]?.title ?? steps[at]?.action ?? `step ${at + 1}`;
  const last = props.step >= props.place.steps - 1;
  const proceed = () => {
    setBusy(true);
    setProblem(null);
    openStep(props.client, digest, props.step + 1, props.place.runs).then(
      (opened) => {
        setBusy(false);
        if (opened.stopped) {
          setBlocked(opened);
          return;
        }
        const draft = draftOfStep(opened, digest, Date.now());
        saveDraft(localStore(), draft);
        props.go(hrefOf({ name: "kit", kit: draft.kit || "rascal", task: draft.task }));
      },
      (error: unknown) => {
        setBusy(false);
        setProblem(error instanceof ApiError ? error.message : String(error));
      },
    );
  };
  const stop = () => {
    saveChain(localStore(), { ...props.place, stopped: true, at: Date.now() });
    setStopped(true);
  };
  const bundleNew = () => {
    const runs = props.place.runs.filter((one): one is string => typeof one === "string");
    bundleChain(props.client, runs).then(
      (blob) => save(blob, `chain-${runs[0] ?? "runs"}.whizbang-bundle`),
      (error: unknown) => setProblem(error instanceof ApiError ? error.message : String(error)),
    );
  };
  const said = problem ? (
    <p class="field__error" role="alert">
      {problem}
    </p>
  ) : null;
  if (blocked?.stopped) {
    return (
      <section class="chain-card chain-card--blocked" aria-label="The chain">
        <div class="chain-card__head">
          <h2 class="chain-card__title">{`The chain stops at step ${props.step + 2}`}</h2>
          <span class="tag tag--error mono">BLOCKED</span>
        </div>
        <p class="chain-card__line">{blocked.stopped.why}</p>
        <div class="chain-card__actions">
          <Button variant="secondary" onClick={() => props.go(hrefOf({ name: "run", id: blocked.stopped?.run ?? props.run }))}>
            {`Open ${middle(blocked.stopped.run ?? props.run, 14)}`}
          </Button>
          <Button variant="quiet" onClick={() => props.go(hrefOf({ name: "chain", digest }))}>
            Close the chain
          </Button>
        </div>
        {said}
      </section>
    );
  }
  if (last) {
    const original = steps[props.step]?.run;
    return (
      <section class="chain-card" aria-label="The chain">
        <div class="chain-card__head">
          <h2 class="chain-card__title">Chain done</h2>
          <span class="mono replay-report__meta">{`${props.place.steps} steps · ${middle(digest, 14)}`}</span>
        </div>
        <ol class="chain-steps">
          {props.place.runs.map((run, at) => (
            <li key={at} class="chain-step chain-step--ended">
              <span class="chain-step__mark mono" aria-hidden="true">
                ✓
              </span>
              <span class="chain-step__text">
                <span class="chain-step__name">{`${at + 1} · ${named(at)}`}</span>
                <span class="chain-step__line mono">{run ? middle(run, 18) : "not run"}</span>
              </span>
            </li>
          ))}
        </ol>
        <div class="chain-card__actions">
          {original ? (
            <Button variant="secondary" onClick={() => props.go(hrefOf({ name: "compare", baseline: original, attempt: props.run }))}>
              Compare last step
            </Button>
          ) : null}
          <Button variant="secondary" onClick={bundleNew}>
            Bundle the new chain
          </Button>
        </div>
        {said}
      </section>
    );
  }
  return (
    <section class="chain-card" aria-label="The chain">
      <div class="chain-card__head">
        <h2 class="chain-card__title">{`Step ${props.step + 1} · ${named(props.step)} ended`}</h2>
        <span class="tag tag--done mono">ENDED</span>
      </div>
      <p class="chain-card__line">
        {stopped
          ? "You stopped this chain here. Its runs stay; reopen it from Bundles to go on."
          : `Its outputs go to step ${props.step + 2}, ${named(props.step + 1)}. Where one differs from the one recorded, you'll see both before that step runs.`}
      </p>
      <div class="chain-card__actions">
        <Button variant="primary" busy={busy} onClick={proceed}>
          {`Continue to step ${props.step + 2}`}
        </Button>
        {stopped ? null : (
          <Button variant="secondary" onClick={stop}>
            Stop
          </Button>
        )}
      </div>
      {said}
    </section>
  );
}
