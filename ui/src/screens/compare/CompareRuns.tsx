import { useEffect, useState } from "preact/hooks";
import { ApiError, type Client, type Hello } from "../../api";
import { Button } from "../../components/Button";
import { Compare, type CompareMode } from "../../components/Compare";
import { localStore } from "../../drafts";
import { dollars, elapsed, middle } from "../../format";
import { useArtifact, useImageSize } from "../../hooks";
import { hrefOf } from "../../routes";
import { fetchCompare, pictureOf, rowsShown, savePin, SECTIONS, type Compared, type CompareRow, type CompareSide } from "../../saved";
import { THUMB_EDGE } from "../shared";
import "./compare.css";

export interface CompareRunsProps {
  baseline: string;
  attempt?: string;
  hello: Hello | null;
  client: Client;
  go: (hash: string) => void;
}

/** A moment as a side's line says it: "today 13:41", or the day. */
function when(ms: number | null | undefined, now: number): string {
  if (!ms) return "";
  const at = new Date(ms);
  const two = (n: number) => String(n).padStart(2, "0");
  const time = `${two(at.getHours())}:${two(at.getMinutes())}`;
  return new Date(now).toDateString() === at.toDateString() ? `today ${time}` : `${at.toLocaleDateString()} ${time}`;
}

/** How a side ended, in one word: its verdict when it has one, else its outcome. */
function ended(side: CompareSide): string {
  return side.verdict?.word ?? side.outcome;
}

function tone(word: string): string {
  if (word === "pass" || word === "completed") return "pass";
  if (word === "fail" || word === "failed" || word === "invalid") return "fail";
  return "plain";
}

/**
 * Compare two runs (plan m7, task U1, item 1; the design's surface 1): a baseline pinned
 * and an attempt of the same task, their pictures at one zoom, what differs between what
 * each was asked and resolved, their inputs, how each ended, its time, and its family's
 * cost and models. It claims no winner and computes no score. The pin is in the address
 * and in the browser's storage, so a reload keeps it.
 */
export function CompareRuns(props: CompareRunsProps) {
  const [compared, setCompared] = useState<Compared | null>(null);
  const [problem, setProblem] = useState<ApiError | null>(null);
  const [all, setAll] = useState(false);
  const [mode, setMode] = useState<CompareMode>("side");
  useEffect(() => {
    setCompared(null);
    setProblem(null);
    if (!props.attempt) return;
    let live = true;
    fetchCompare(props.client, props.baseline, props.attempt).then(
      (answer) => {
        if (!live) return;
        setCompared(answer);
        savePin(localStore(), { run: answer.baseline.run, action: answer.action });
      },
      (error: unknown) => live && setProblem(error instanceof ApiError ? error : new ApiError(0, String(error))),
    );
    return () => {
      live = false;
    };
  }, [props.baseline, props.attempt]);

  const clear = () => {
    savePin(localStore(), null);
    props.go(hrefOf(props.attempt ? { name: "run", id: props.attempt } : { name: "history", query: new URLSearchParams() }));
  };
  const swap = () => props.attempt && props.go(hrefOf({ name: "compare", baseline: props.attempt, attempt: props.baseline }));
  const pinInstead = () => props.attempt && props.go(hrefOf({ name: "compare", baseline: props.attempt }));
  const now = Date.now();

  return (
    <main class="compare-runs" aria-labelledby="compare-title">
      <section class="compare-runs__left">
        <header class="compare-runs__head">
          <h1 id="compare-title" class="compare-runs__title">
            Compare two runs
          </h1>
          <p class="compare-runs__sub mono">same task · no winner, no score</p>
        </header>
        <div class="compare-runs__pick">
          <div class="compare-runs__pick-head">
            <span class="compare-runs__label mono">
              <span class="compare-runs__dot compare-runs__dot--baseline" aria-hidden="true" />
              BASELINE · PINNED
            </span>
            <button type="button" class="link-button" onClick={clear}>
              Clear
            </button>
          </div>
          <SideCard client={props.client} side={compared?.baseline} run={props.baseline} now={now} />
        </div>
        <div class="compare-runs__pick">
          <div class="compare-runs__pick-head">
            <span class="compare-runs__label mono">
              <span class="compare-runs__dot compare-runs__dot--attempt" aria-hidden="true" />
              ATTEMPT
            </span>
            {props.attempt ? (
              <button type="button" class="link-button" onClick={swap}>
                ⇅ Swap
              </button>
            ) : null}
          </div>
          {props.attempt ? <SideCard client={props.client} side={compared?.attempt} run={props.attempt} now={now} /> : <p class="compare-runs__none">Not picked yet</p>}
        </div>
        <p class="compare-runs__help">Pin a baseline from any result or from History, then open another run of the same task. The pin lives in this page's address, so a reload keeps it.</p>
        <footer class="compare-runs__foot">
          {props.attempt ? (
            <Button variant="secondary" onClick={() => props.attempt && props.go(hrefOf({ name: "run", id: props.attempt }))}>
              Open attempt
            </Button>
          ) : null}
          <Button variant="secondary" onClick={() => props.go(hrefOf({ name: "run", id: props.baseline }))}>
            Open baseline
          </Button>
        </footer>
      </section>
      <section class="compare-runs__centre" aria-label="Pictures">
        {!props.attempt ? (
          <Empty client={props.client} baseline={props.baseline} go={props.go} />
        ) : problem ? (
          <Refused problem={problem} onPin={pinInstead} onKeep={() => props.go(hrefOf({ name: "history", query: new URLSearchParams() }))} />
        ) : compared ? (
          <Pictures client={props.client} compared={compared} mode={mode} onMode={setMode} />
        ) : (
          <p class="empty compare-runs__pad">Reading the two runs…</p>
        )}
      </section>
      <aside class="compare-runs__right" aria-label="What differs">
        <div class="compare-runs__right-head">
          <h2 class="compare-runs__right-title">What differs</h2>
          <button type="button" class="link-button" aria-pressed={all ? "true" : "false"} onClick={() => setAll((now) => !now)}>
            {all ? "Only what differs" : "Show all"}
          </button>
        </div>
        {compared ? <Differs compared={compared} all={all} /> : null}
      </aside>
    </main>
  );
}

/** A run on the left: its thumbnail, its id and how it ended, when it ran, its time and cost. */
function SideCard(props: { client: Client; side: CompareSide | undefined; run: string; now: number }) {
  const picture = props.side ? pictureOf(props.side) : null;
  const src = useArtifact(props.client, picture, THUMB_EDGE);
  const side = props.side;
  const line = side ? [when(side.start_ms, props.now), side.time_ms != null ? elapsed(side.time_ms) : null, dollars(side.cost.total)].filter(Boolean).join(" · ") : "";
  return (
    <div class="compare-runs__card">
      {src ? <img class="compare-runs__thumb" src={src} alt="" /> : <span class="compare-runs__thumb" aria-hidden="true" />}
      <span class="compare-runs__card-words">
        <span class="compare-runs__card-run">{`run ${middle(props.run, 18)}${side ? ` · ${ended(side)}` : ""}`}</span>
        <span class="compare-runs__card-line mono">{line}</span>
      </span>
    </div>
  );
}

/** One run picked: its picture, and where to pick an attempt. */
function Empty(props: { client: Client; baseline: string; go: (hash: string) => void }) {
  return (
    <div class="compare-runs__empty">
      <div class="compare-runs__slot" role="status">
        <strong>Pick an attempt</strong>
        <span>Run the task again, or open another run of the same task from History and press Compare with baseline.</span>
        <Button variant="secondary" onClick={() => props.go(hrefOf({ name: "history", query: new URLSearchParams() }))}>
          Open History
        </Button>
      </div>
    </div>
  );
}

/** The comparison the host refused: two tasks, a child run, or a run not ended. */
function Refused(props: { problem: ApiError; onPin: () => void; onKeep: () => void }) {
  const title = props.problem.status === 409 ? "This run has not ended yet" : "These two runs can't be compared";
  return (
    <div class="compare-runs__refused" role="alert">
      <h2 class="compare-runs__refused-title">{title}</h2>
      <p>{props.problem.status === 409 ? `${props.problem.message} It ends first, then it can be compared.` : props.problem.message}</p>
      {props.problem.status === 422 ? (
        <div class="compare-runs__refused-actions">
          <Button variant="secondary" onClick={props.onPin}>
            Pin this one instead
          </Button>
          <Button variant="quiet" onClick={props.onKeep}>
            Keep baseline
          </Button>
        </div>
      ) : null}
    </div>
  );
}

/** The two pictures at one zoom: swiped or as a difference only when both are of one size. */
function Pictures(props: { client: Client; compared: Compared; mode: CompareMode; onMode: (mode: CompareMode) => void }) {
  const { baseline, attempt } = props.compared;
  const left = useArtifact(props.client, pictureOf(baseline));
  const right = useArtifact(props.client, pictureOf(attempt));
  const leftSize = useImageSize(left);
  const rightSize = useImageSize(right);
  const lines = (
    <div class="compare-runs__ends">
      <span class={`compare-runs__end compare-runs__end--${tone(ended(baseline))} mono`}>{[ended(baseline), baseline.verdict?.why].filter(Boolean).join(" · ")}</span>
      <span class={`compare-runs__end compare-runs__end--${tone(ended(attempt))} mono`}>{[ended(attempt), attempt.verdict?.why].filter(Boolean).join(" · ")}</span>
    </div>
  );
  if (!pictureOf(baseline) || !pictureOf(attempt)) {
    return (
      <div class="compare-runs__stage">
        <p class="empty compare-runs__pad">One of these runs made no picture and was given none: there is nothing to show side by side.</p>
        {lines}
      </div>
    );
  }
  if (!left || !right || !leftSize || !rightSize) return <p class="empty compare-runs__pad">Loading the pictures…</p>;
  const same = leftSize.width === rightSize.width && leftSize.height === rightSize.height;
  return (
    <div class="compare-runs__stage">
      <p class="compare-runs__size mono">{same ? `fit · both ${leftSize.width}×${leftSize.height}` : `fit · ${leftSize.width}×${leftSize.height} and ${rightSize.width}×${rightSize.height}`}</p>
      {same ? (
        <Compare before={left} after={right} width={leftSize.width} height={leftSize.height} mode={props.mode} zoom="fit" onMode={props.onMode} labels={{ before: "Baseline", after: "Attempt" }} />
      ) : (
        <div class="compare compare-runs__apart">
          <p class="workspace__purpose">Swipe and Difference need two pictures of one size: these are shown side by side.</p>
          <div class="compare-runs__apart-pictures">
            <figure class="compare__figure compare-runs__apart-figure">
              <img src={left} alt="Baseline" />
              <figcaption class="compare__badge mono">BASELINE</figcaption>
            </figure>
            <figure class="compare__figure compare-runs__apart-figure">
              <img src={right} alt="Attempt" />
              <figcaption class="compare__badge mono">ATTEMPT</figcaption>
            </figure>
          </div>
        </div>
      )}
      {lines}
    </div>
  );
}

/** One row of the right column: its name, the baseline's value and the attempt's. */
function Row(props: { name: string; baseline: string | null; attempt: string | null; differs: boolean; tones?: [string, string] }) {
  const same = !props.differs;
  return (
    <div class={same ? "compare-runs__row" : "compare-runs__row compare-runs__row--differs"} data-differs={props.differs ? "true" : "false"}>
      <span class="compare-runs__row-name">{props.name}</span>
      <span class={`compare-runs__row-value mono compare-runs__row-value--${props.tones?.[0] ?? "plain"}`} title={props.baseline ?? ""}>
        {props.baseline ?? "—"}
      </span>
      <span class={`compare-runs__row-value mono ${same ? "compare-runs__row-value--same" : `compare-runs__row-value--${props.tones?.[1] ?? "plain"}`}`} title={props.attempt ?? ""}>
        {same ? "same" : (props.attempt ?? "—")}
      </span>
    </div>
  );
}

/** A value as the column has room for it: an identity shortened, any other whole. */
function short(value: string | null): string | null {
  return value && /^sha256:[0-9a-f]{64}/.test(value) ? middle(value, 14) : value;
}

/** What differs, by section, then how each ended, its time, and its family's cost and models. */
function Differs(props: { compared: Compared; all: boolean }) {
  const { baseline, attempt } = props.compared;
  const shown = rowsShown(props.compared.rows, props.all);
  const time = (side: CompareSide) => (side.time_ms != null ? elapsed(side.time_ms) : null);
  const cost = (side: CompareSide) => `${dollars(side.cost.total)}${(side.cost.unresolved ?? 0) > 0 ? `, ${dollars(side.cost.unresolved ?? 0)} unresolved` : ""}${side.whole ? "" : ", not all read"}`;
  const outcome: Array<{ name: string; baseline: string | null; attempt: string | null; tones?: [string, string] }> = [
    { name: "outcome", baseline: ended(baseline), attempt: ended(attempt), tones: [tone(ended(baseline)), tone(ended(attempt))] },
    { name: "time", baseline: time(baseline), attempt: time(attempt) },
    { name: "cost, whole run", baseline: cost(baseline), attempt: cost(attempt) },
    { name: "models", baseline: baseline.models.join(", ") || "none", attempt: attempt.models.join(", ") || "none" },
  ];
  return (
    <div class="compare-runs__differs">
      <div class="compare-runs__columns mono" aria-hidden="true">
        <span />
        <span class="compare-runs__column compare-runs__column--baseline">BASELINE</span>
        <span class="compare-runs__column compare-runs__column--attempt">ATTEMPT</span>
      </div>
      {SECTIONS.map(({ section, title }) => {
        const rows = shown.filter((row: CompareRow) => row.section === section);
        if (rows.length === 0) return null;
        return (
          <section key={section} class="compare-runs__section" aria-label={title}>
            <h3 class="compare-runs__section-title">{title}</h3>
            {rows.map((row) => (
              <Row key={`${section}-${row.name}`} name={row.name} baseline={short(row.baseline)} attempt={short(row.attempt)} differs={row.differs} />
            ))}
          </section>
        );
      })}
      {shown.length === 0 ? <p class="empty compare-runs__pad">They were asked the same, resolved the same, and given the same inputs.</p> : null}
      <section class="compare-runs__section" aria-label="Outcome">
        <h3 class="compare-runs__section-title">Outcome</h3>
        {outcome.map((row) => (
          <Row key={row.name} name={row.name} baseline={row.baseline} attempt={row.attempt} differs={row.baseline !== row.attempt} tones={row.tones} />
        ))}
      </section>
    </div>
  );
}
