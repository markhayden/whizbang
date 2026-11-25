// Guided chain replay (plan m7, task U2, item 2; the design's surface 4): a chain's steps
// with their states, read again from the bundle and from the runs the browser kept for it;
// the next step opened as a draft for review, its linked inputs bound to the new run of the
// step before; Stop. Nothing runs two steps by itself: each runs when Run is pressed.
import { useEffect, useRef, useState } from "preact/hooks";
import { ApiError, type Client, type Hello } from "../../api";
import { chainOf, draftOfStep, openStep, saveChain, showBundle, stepStates, type BundleSummary, type ChainPlace, type OpenedStep } from "../../bundles";
import { Button } from "../../components/Button";
import { ChainSteps } from "../../components/ChainSteps";
import "../../components/bundles.css";
import { localStore, saveDraft } from "../../drafts";
import { middle } from "../../format";
import { hrefOf } from "../../routes";
import "../bundles/bundles.css";

export interface ChainProps {
  hello?: Hello | null;
  client: Client;
  digest: string;
  go: (hash: string) => void;
}

export function Chain(props: ChainProps) {
  const store = localStore();
  const [bundle, setBundle] = useState<BundleSummary | null>(null);
  const [place, setPlace] = useState<ChainPlace | null>(() => chainOf(store, props.digest));
  const [problem, setProblem] = useState<string | null>(null);
  const [blocked, setBlocked] = useState<OpenedStep | null>(null);
  const [busy, setBusy] = useState(false);
  const asking = useRef(0);
  useEffect(() => {
    const mine = (asking.current += 1);
    setBundle(null);
    showBundle(props.client, props.digest).then(
      (found) => {
        if (mine !== asking.current) return;
        setBundle(found);
        setPlace((now) => now ?? { digest: props.digest, steps: found.steps.length, runs: [], stopped: false, at: Date.now() });
      },
      (error: unknown) => mine === asking.current && setProblem(error instanceof ApiError ? error.message : String(error)),
    );
  }, [props.digest]);
  if (!bundle || !place) {
    return (
      <main class="screen bundles" aria-labelledby="chain-title">
        <h1 id="chain-title" class="screen__title">
          Chain
        </h1>
        {problem ? (
          <p class="field__error" role="alert">
            {problem}
          </p>
        ) : (
          <p class="empty">Reading the chain…</p>
        )}
      </main>
    );
  }
  const next = place.runs.findIndex((run) => !run);
  const current = next === -1 ? (place.runs.length < bundle.steps.length ? place.runs.length : -1) : next;
  const states = stepStates({ ...place, steps: bundle.steps.length }, blocked ? current : null);
  const review = () => {
    if (current < 0) return;
    setBusy(true);
    setProblem(null);
    openStep(props.client, props.digest, current, place.runs).then(
      (opened) => {
        setBusy(false);
        if (opened.stopped) {
          setBlocked(opened);
          return;
        }
        const draft = draftOfStep(opened, props.digest, Date.now());
        saveDraft(store, draft);
        props.go(hrefOf({ name: "kit", kit: draft.kit || "rascal", task: draft.task }));
      },
      (error: unknown) => {
        setBusy(false);
        setProblem(error instanceof ApiError ? error.message : String(error));
      },
    );
  };
  const stop = () => {
    const stopped = { ...place, stopped: true, at: Date.now() };
    saveChain(store, stopped);
    setPlace(stopped);
  };
  const resume = () => {
    const going = { ...place, stopped: false, at: Date.now() };
    saveChain(store, going);
    setPlace(going);
  };
  const ended = place.runs.filter(Boolean).length;
  return (
    <main class="screen bundles chain" aria-labelledby="chain-title">
      <header class="bundles__head">
        <div>
          <h1 id="chain-title" class="screen__title">
            {`Chain · ${middle(props.digest, 18)}`}
          </h1>
          <p class="history__line">{`${ended} of ${bundle.steps.length} ended. Nothing runs two steps by itself; closing the tab loses no run, and the chain reopens here.`}</p>
        </div>
        <a class="screen__link" href={hrefOf({ name: "bundles", digest: props.digest })}>
          The bundle
        </a>
      </header>
      <div class="chain__body">
        <ChainSteps steps={bundle.steps} states={states} runs={place.runs} onOpen={(run) => props.go(hrefOf({ name: "run", id: run }))} />
        {blocked?.stopped ? (
          <section class="chain-card chain-card--blocked" aria-label="The chain">
            <div class="chain-card__head">
              <h2 class="chain-card__title">{`The chain stops at step ${current + 1}`}</h2>
              <span class="tag tag--error mono">BLOCKED</span>
            </div>
            <p class="chain-card__line">{blocked.stopped.why}</p>
          </section>
        ) : null}
        {problem ? (
          <p class="field__error" role="alert">
            {problem}
          </p>
        ) : null}
        <div class="chain-card__actions">
          {place.stopped ? (
            <Button variant="secondary" onClick={resume}>
              Go on with the chain
            </Button>
          ) : current >= 0 ? (
            <>
              <Button variant="primary" busy={busy} onClick={review}>
                {`Review step ${current + 1}`}
              </Button>
              <Button variant="quiet" onClick={stop}>
                Stop the chain
              </Button>
            </>
          ) : (
            <p class="history__line">The chain is done: every step ran.</p>
          )}
        </div>
      </div>
    </main>
  );
}
