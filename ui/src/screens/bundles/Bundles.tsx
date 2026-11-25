// Bundles (plan m7, task U2, item 1; the design's surface 3, C): the bundles the store
// holds, a bundle file taken in (dropped on the frame, or chosen), and one bundle shown:
// its steps and links, what it holds and what it withholds, whether each step replays as
// this build decides it from its records, Start the chain, and Delete.
import { useEffect, useRef, useState } from "preact/hooks";
import { ApiError, type Client, type Hello } from "../../api";
import { bundleName, chainOf, deleteBundle, draftOfStep, importBundle, linksOf, listBundles, openStep, saveChain, showBundle, sizeWords, type BundleSummary } from "../../bundles";
import { Button } from "../../components/Button";
import { ChainSteps } from "../../components/ChainSteps";
import "../../components/bundles.css";
import { localStore, saveDraft } from "../../drafts";
import { middle } from "../../format";
import { hrefOf } from "../../routes";
import "./bundles.css";

export interface BundlesProps {
  hello?: Hello | null;
  client: Client;
  digest?: string;
  go: (hash: string) => void;
}

/** The frame a bundle file is dropped on, or that opens the file chooser. */
function BundleDrop(props: { busy: boolean; onFile: (file: File) => void }) {
  const [over, setOver] = useState(false);
  return (
    <label
      class={over ? "drop-target drop-target--over bundles__drop" : "drop-target bundles__drop"}
      onDragOver={(event) => {
        event.preventDefault();
        setOver(true);
      }}
      onDragLeave={() => setOver(false)}
      onDrop={(event) => {
        event.preventDefault();
        setOver(false);
        const file = event.dataTransfer?.files?.[0];
        if (file && !props.busy) props.onFile(file);
      }}
    >
      <span class="drop-target__label">{props.busy ? "Reading the bundle whole…" : "Drop a bundle here, or choose one"}</span>
      <input
        class="visually-hidden"
        type="file"
        accept=".whizbang-bundle,.tar,application/x-tar"
        disabled={props.busy}
        aria-label="A bundle file"
        onChange={(event) => {
          const file = (event.currentTarget as HTMLInputElement).files?.[0];
          if (file) props.onFile(file);
          (event.currentTarget as HTMLInputElement).value = "";
        }}
      />
    </label>
  );
}

export function Bundles(props: BundlesProps) {
  const [list, setList] = useState<BundleSummary[] | null>(null);
  const [shown, setShown] = useState<BundleSummary | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [refused, setRefused] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [asked, setAsked] = useState(0);
  // Which asking of a bundle an answer belongs to: one for a bundle no longer shown is dropped.
  const asking = useRef(0);
  useEffect(() => {
    let live = true;
    listBundles(props.client).then(
      (found) => live && setList(found),
      (error: unknown) => live && setProblem(error instanceof ApiError ? error.message : String(error)),
    );
    return () => {
      live = false;
    };
  }, [asked]);
  useEffect(() => {
    const mine = (asking.current += 1);
    setShown(null);
    if (!props.digest) return;
    showBundle(props.client, props.digest).then(
      (found) => mine === asking.current && setShown(found),
      (error: unknown) => mine === asking.current && setProblem(error instanceof ApiError ? error.message : String(error)),
    );
  }, [props.digest, asked]);
  const take = (file: File) => {
    setBusy(true);
    setRefused(null);
    setProblem(null);
    importBundle(props.client, file).then(
      (summary) => {
        setBusy(false);
        setAsked((n) => n + 1);
        props.go(hrefOf({ name: "bundles", digest: summary.digest }));
      },
      (error: unknown) => {
        setBusy(false);
        if (error instanceof ApiError && error.status === 422) setRefused(error.message);
        else setProblem(error instanceof ApiError ? error.message : String(error));
      },
    );
  };
  const start = (summary: BundleSummary) => {
    const store = localStore();
    const place = chainOf(store, summary.digest);
    const fresh = { digest: summary.digest, steps: summary.steps.length, runs: [], stopped: false, at: Date.now() };
    const kept = place && !place.stopped ? place : fresh;
    saveChain(store, kept);
    if (summary.steps.length > 1) {
      props.go(hrefOf({ name: "chain", digest: summary.digest }));
      return;
    }
    openStep(props.client, summary.digest, 0, []).then(
      (opened) => {
        if (opened.stopped) {
          setProblem(opened.stopped.why);
          return;
        }
        const draft = draftOfStep(opened, summary.digest, Date.now());
        saveDraft(store, draft);
        props.go(hrefOf({ name: "kit", kit: draft.kit || "rascal", task: draft.task }));
      },
      (error: unknown) => setProblem(error instanceof ApiError ? error.message : String(error)),
    );
  };
  const remove = (summary: BundleSummary) => {
    deleteBundle(props.client, summary.digest).then(
      () => {
        setAsked((n) => n + 1);
        props.go(hrefOf({ name: "bundles" }));
      },
      (error: unknown) => setProblem(error instanceof ApiError ? error.message : String(error)),
    );
  };
  return (
    <main class="screen bundles" aria-labelledby="bundles-title">
      <header class="bundles__head">
        <div>
          <h1 id="bundles-title" class="screen__title">
            Bundles
          </h1>
          <p class="history__line">A bundle carries a run, or a chain of runs, out of one store as one file another store takes in. It holds the runs' pictures and words, unencrypted, and no key or path.</p>
        </div>
        <a class="screen__link" href={hrefOf({ name: "history", query: new URLSearchParams() })}>
          History
        </a>
      </header>
      <div class="bundles__body">
        <section class="bundles__list" aria-label="The bundles this store holds">
          <BundleDrop busy={busy} onFile={take} />
          {refused ? (
            <div class="bundles__refused" role="alert">
              <p class="bundles__refused-title">Not a bundle this build can read</p>
              <p class="bundles__refused-line mono">{refused}</p>
            </div>
          ) : null}
          {problem ? (
            <p class="field__error" role="alert">
              {problem}
            </p>
          ) : null}
          {list === null ? (
            problem ? null : <p class="empty">Reading the bundles…</p>
          ) : list.length === 0 ? (
            <p class="empty">No bundle yet. Bundle a run from its result's More menu, a chain from History, or take one in here.</p>
          ) : (
            <ul class="bundles__rows">
              {list.map((one) => (
                <li key={one.digest}>
                  <a class={one.digest === props.digest ? "bundles__row bundles__row--chosen" : "bundles__row"} href={hrefOf({ name: "bundles", digest: one.digest })}>
                    <span class="bundles__row-name">{bundleName(one)}</span>
                    <span class="bundles__row-line mono">{`${one.steps.length} ${one.steps.length === 1 ? "step" : "steps"} · ${one.withheld?.length ?? 0} withheld · ${middle(one.digest, 18)}`}</span>
                  </a>
                </li>
              ))}
            </ul>
          )}
        </section>
        {shown ? <Shown summary={shown} onStart={() => start(shown)} onDelete={() => remove(shown)} /> : null}
      </div>
    </main>
  );
}

function Shown(props: { summary: BundleSummary; onStart: () => void; onDelete: () => void }) {
  const summary = props.summary;
  // Delete asks once: the bundle goes, and what a run of this store took from it stays.
  const [asking, setAsking] = useState(false);
  const links = linksOf(summary);
  const place = chainOf(localStore(), summary.digest);
  const runs = place?.runs ?? [];
  const states = summary.steps.map((_, at) => (runs[at] ? ("ended" as const) : ("waiting" as const)));
  return (
    <section class="bundle-card" aria-label="The bundle">
      <header class="bundle-card__head">
        <h2 class="bundle-card__title">{summary.steps.length > 1 ? `A chain of ${summary.steps.length} steps` : (summary.steps[0]?.title ?? summary.steps[0]?.action ?? "A bundle")}</h2>
        <p class="bundle-card__line mono">{`${summary.digest} · checked whole${summary.bytes ? ` · ${sizeWords(summary.bytes)}` : ""}`}</p>
      </header>
      <ChainSteps steps={summary.steps} states={states} runs={runs} />
      <div class="bundle-card__section">
        {summary.steps.map((step) => (
          <p key={step.step} class="holds__item mono">
            {step.replayable ? (
              <>
                <span class="holds__yes" aria-hidden="true">✓</span>
                {` step ${step.step + 1} replays offline`}
              </>
            ) : (
              <>
                <span class="holds__no" aria-hidden="true">✕</span>
                {` step ${step.step + 1}: ${step.why ?? "not replayable"}`}
              </>
            )}
          </p>
        ))}
        {links.length > 0 ? <p class="holds__item mono">{`↳ link: ${links.join(", ")}`}</p> : null}
      </div>
      <div class="bundle-card__section holds">
        <p class="holds__title">Holds</p>
        <p class="holds__item mono">{`${summary.holds.records} records · ${summary.holds.records} journals · ${summary.holds.files} files`}</p>
        <p class="holds__title">Withholds</p>
        {summary.withheld.length === 0 ? (
          <p class="holds__item holds__item--never">nothing</p>
        ) : (
          summary.withheld.map((one, at) => (
            <p key={at} class="holds__item holds__item--never mono">
              <span class="holds__no" aria-hidden="true">✕</span>
              {` ${one.what} of ${middle(one.run, 14)}: ${one.why}`}
            </p>
          ))
        )}
      </div>
      {asking ? (
        <div class="bundle-card__section" role="alert">
          <p class="holds__item">Delete this bundle? Its runs and files go from this store; what a run here took from it stays.</p>
          <div class="chain-card__actions">
            <Button variant="danger" onClick={props.onDelete}>
              Delete it
            </Button>
            <Button variant="quiet" onClick={() => setAsking(false)}>
              Keep it
            </Button>
          </div>
        </div>
      ) : null}
      <footer class="bundle-card__foot">
        <Button variant="quiet" onClick={() => setAsking(true)}>
          Delete bundle
        </Button>
        <Button variant="primary" onClick={props.onStart}>
          {summary.steps.length > 1 ? "Start the chain" : "Open its step"}
        </Button>
      </footer>
    </section>
  );
}
