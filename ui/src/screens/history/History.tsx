// History (plan m5, task W3, items 1 and 2): every run of the store, newest first, by
// pages, under the filter the address carries (kit, result and words on screen; every
// filter of `runs stats` beside them), with the figures `runs stats` gives for the same
// filter. Opening a row opens its result.
import { useEffect, useState } from "preact/hooks";
import { ApiError, type Client, type Hello } from "../../api";
import { Button } from "../../components/Button";
import { HistoryRow } from "../../components/HistoryRow";
import { kitTone } from "../../components/kitTone";
import { Segmented } from "../../components/Segmented";
import { TextField } from "../../components/TextField";
import { ago, dollars, elapsed } from "../../format";
import { useArtifact } from "../../hooks";
import { hrefOf } from "../../routes";
import { RESULTS, figuresOf, listRuns, type Figures, type HistoryRun } from "./calls";

export interface HistoryProps {
  hello?: Hello | null;
  client?: Client;
  query?: URLSearchParams;
  go?: (hash: string) => void;
}

/** How long the words wait for the typing to stop before they filter. */
const WORDS_WAIT_MS = 300;

function Row(props: { client: Client; run: HistoryRun; tone: ReturnType<typeof kitTone>; now: number; onOpen: () => void }) {
  const run = props.run;
  const thumb = useArtifact(props.client, run.thumb ?? undefined, 96);
  const asked = [...(run.words ?? []), ...(run.pictures ?? [])].join(" · ");
  return (
    <HistoryRow
      task={run.title ?? run.action}
      tone={props.tone}
      asked={asked || "—"}
      tag={{ kind: run.result }}
      models={(run.models ?? []).join(" · ")}
      time={typeof run.time_ms === "number" ? elapsed(run.time_ms) : undefined}
      cost={typeof run.cost === "number" ? run.cost : 0}
      when={typeof run.at_ms === "number" ? ago(run.at_ms, props.now) : ""}
      thumb={thumb}
      onOpen={props.onOpen}
    />
  );
}

/** The figures of `runs stats`, as one line each. */
function FiguresOf(props: { figures: Figures }) {
  const f = props.figures;
  const v = f.verdicts ?? { pass: 0, fail: 0, unsure: 0 };
  const judged = v.pass + v.fail + v.unsure;
  const lines: Array<[string, string]> = [
    ["runs", String(f.runs)],
    ["completed", String(f.ended?.completed ?? 0)],
    ["cancelled", String(f.ended?.cancelled ?? 0)],
    ["error", String((f.ended?.error ?? 0) + (f.ended?.invalid ?? 0))],
    ["interrupted", String(f.ended?.interrupted ?? 0)],
    ["verdicts", `${judged} · pass ${v.pass} · fail ${v.fail} · unsure ${v.unsure}`],
    ["retries · repairs", `${f.retries ?? 0} · ${f.repairs ?? 0}`],
    ["time", f.time ? `median ${elapsed(f.time.median_ms)} · 95th ${elapsed(f.time.p95_ms)}` : "—"],
    ["cost", `${dollars((f.settled_micros ?? 0) / 1e6)} settled · ${dollars((f.unresolved_micros ?? 0) / 1e6)} unresolved`],
    ["cache hits", String(f.cache_hits ?? 0)],
  ];
  if (f.loops) lines.splice(7, 0, ["loops", `${f.loops.runs} runs · ${f.loops.attempts} attempts · ${f.loops.retries} retries`]);
  return (
    <dl class="history__figures" aria-label="Figures of these runs">
      {lines.map(([name, value]) => (
        <div key={name} class="history__figure">
          <dt>{name}</dt>
          <dd class="mono">{value}</dd>
        </div>
      ))}
    </dl>
  );
}

/** History: every run, newest first, with its figures. */
export function History(props: HistoryProps) {
  const query = props.query ?? new URLSearchParams();
  const asked = query.toString();
  const kits = Array.isArray(props.hello?.kits) ? props.hello.kits : [];
  const [runs, setRuns] = useState<HistoryRun[]>([]);
  const [more, setMore] = useState(false);
  const [figures, setFigures] = useState<Figures | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [loaded, setLoaded] = useState(false);
  const [words, setWords] = useState(query.get("words") ?? "");

  useEffect(() => {
    const client = props.client;
    if (!client) return;
    let live = true;
    setProblem(null);
    const said = (error: unknown) => live && setProblem(error instanceof ApiError ? error.message : String(error));
    listRuns(client, query).then((page) => {
      if (!live) return;
      setRuns(page.runs);
      setMore(page.more);
      setLoaded(true);
    }, said);
    figuresOf(client, query).then((found) => live && setFigures(found), said);
    return () => {
      live = false;
    };
  }, [props.client, asked]);

  // The address holds the filter: a change of it is a new address.
  const set = (name: string, value: string) => {
    const next = new URLSearchParams(query);
    if (value) next.set(name, value);
    else next.delete(name);
    props.go?.(hrefOf({ name: "history", query: next }));
  };
  useEffect(() => {
    if (words === (query.get("words") ?? "")) return;
    const timer = setTimeout(() => set("words", words.trim()), WORDS_WAIT_MS);
    return () => clearTimeout(timer);
  }, [words]);

  const later = () => {
    const last = runs[runs.length - 1];
    if (!props.client || !last) return;
    listRuns(props.client, query, last.run).then(
      (page) => {
        setRuns((now) => [...now, ...page.runs]);
        setMore(page.more);
      },
      (error: unknown) => setProblem(error instanceof ApiError ? error.message : String(error)),
    );
  };

  const now = Date.now();
  const toneOf = (kit: string | null | undefined) => kitTone(Math.max(0, kits.findIndex((one) => one.id === kit)));
  return (
    <main class="screen history" aria-labelledby="history-title">
      <header class="history__head">
        <div>
          <h1 id="history-title" class="screen__title">
            History
          </h1>
          <p class="history__line">Every run is recorded. Open one to see it, its report, or run it again.</p>
        </div>
        <div class="history__filters">
          <Segmented
            label="Kit"
            segments={[{ value: "", label: "All kits" }, ...kits.map((kit) => ({ value: kit.id, label: kit.name ?? kit.id }))]}
            value={query.get("kit") ?? ""}
            onChange={(value) => set("kit", value)}
          />
          <label class="history__result">
            <span class="visually-hidden">Result</span>
            <select class="control" value={query.get("result") ?? ""} onChange={(event) => set("result", (event.currentTarget as HTMLSelectElement).value)}>
              <option value="">Any result</option>
              {RESULTS.map((result) => (
                <option key={result} value={result}>
                  {result}
                </option>
              ))}
            </select>
          </label>
          <div class="history__words">
            <TextField id="history-words" label="Words" value={words} placeholder="Filter by picture or words" maxLength={200} onInput={setWords} />
          </div>
        </div>
      </header>
      {problem ? (
        <p class="history__problem" role="alert">
          {problem}
        </p>
      ) : null}
      {loaded && runs.length === 0 ? (
        <p class="empty">{asked ? "No run matches this filter." : "No runs yet. Every run you start is recorded and listed here."}</p>
      ) : (
        <div class="history__list" role="list" aria-label="Runs">
          {runs.map((run) => (
            <div key={run.run} role="listitem" data-run={run.run}>
              {props.client ? <Row client={props.client} run={run} tone={toneOf(run.kit)} now={now} onOpen={() => props.go?.(hrefOf({ name: "run", id: run.run }))} /> : null}
            </div>
          ))}
        </div>
      )}
      {more ? (
        <div class="history__more">
          <Button variant="quiet" onClick={later}>
            More runs
          </Button>
        </div>
      ) : null}
      <footer class="history__foot mono">
        <span>
          {runs.length} {runs.length === 1 ? "run" : "runs"} shown{figures ? ` · ${figures.runs} in the figures, child runs included` : ""}
        </span>
      </footer>
      {figures ? <FiguresOf figures={figures} /> : null}
    </main>
  );
}
