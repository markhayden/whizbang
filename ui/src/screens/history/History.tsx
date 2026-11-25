// History (plan m5, task W3, items 1 and 2): every run of the store, newest first, by
// pages, under the filter the address carries (kit, result and words on screen; every
// filter of `runs stats` beside them), with the figures `runs stats` gives for the same
// filter. Opening a row opens its result.
import { useEffect, useRef, useState } from "preact/hooks";
import { ApiError, type Client, type Hello } from "../../api";
import { Button } from "../../components/Button";
import { HistoryRow } from "../../components/HistoryRow";
import { kitTone } from "../../components/kitTone";
import { Segmented } from "../../components/Segmented";
import { TextField } from "../../components/TextField";
import { ago, dollars, elapsed } from "../../format";
import { useArtifact } from "../../hooks";
import { hrefOf } from "../../routes";
import { localStore } from "../../drafts";
import { readPin, savePin, type Pin } from "../../saved";
import { bundleChain, checkChain, linksOf, type BundleSummary } from "../../bundles";
import { save } from "./calls";
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
    ["runs, steps included", String(f.runs)],
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
  // Which asking of the list an answer belongs to: a page asked before the filter changed
  // is not added to the new list.
  const asking = useRef(0);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    const client = props.client;
    if (!client) return;
    let live = true;
    const mine = (asking.current += 1);
    setLoading(false);
    setProblem(null);
    const said = (error: unknown) => live && setProblem(error instanceof ApiError ? error.message : String(error));
    listRuns(client, query).then((page) => {
      if (!live || mine !== asking.current) return;
      setRuns(page.runs);
      setMore(page.more);
      setLoaded(true);
    }, said);
    figuresOf(client, query).then((found) => live && setFigures(found), said);
    return () => {
      live = false;
    };
  }, [props.client, asked]);

  // The address holds the filter: a change of it is a new address, made from the address
  // as it is now, so two changes made one after the other both stand.
  const set = (name: string, value: string) => {
    const now = typeof location !== "undefined" && location.hash.startsWith("#/history") ? location.hash.split("?")[1] : undefined;
    const next = new URLSearchParams(now ?? query);
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
    if (!props.client || !last || loading) return;
    const mine = asking.current;
    setLoading(true);
    listRuns(props.client, query, last.run).then(
      (page) => {
        if (mine !== asking.current) return;
        setRuns((now) => [...now, ...page.runs.filter((one) => !now.some((seen) => seen.run === one.run))]);
        setMore(page.more);
        setLoading(false);
      },
      (error: unknown) => {
        if (mine !== asking.current) return;
        setLoading(false);
        setProblem(error instanceof ApiError ? error.message : String(error));
      },
    );
  };

  // The baseline pinned for a comparison (plan m7, task U1, item 1): pinned from a row,
  // and each other run of the same task offers to be compared with it.
  const [pin, setPin] = useState<Pin | null>(() => readPin(localStore()));
  const pinRun = (run: HistoryRun) => {
    const next = { run: run.run, action: run.action };
    savePin(localStore(), next);
    setPin(next);
  };
  // A chain (plan m7, task U2, item 1; the design's surface 3, B): runs picked in the order
  // they ran, each pick checked by the host, which names the links the chain holds or why it
  // refuses it; then bundled as one file.
  const [chaining, setChaining] = useState(false);
  const [picked, setPicked] = useState<string[]>([]);
  const [checked, setChecked] = useState<BundleSummary | null>(null);
  const [chainProblem, setChainProblem] = useState<string | null>(null);
  const checking = useRef(0);
  useEffect(() => {
    const mine = (checking.current += 1);
    setChecked(null);
    setChainProblem(null);
    if (!props.client || picked.length === 0) return;
    checkChain(props.client, picked).then(
      (found) => mine === checking.current && setChecked(found),
      (error: unknown) => mine === checking.current && setChainProblem(error instanceof ApiError ? error.message : String(error)),
    );
  }, [picked.join(",")]);
  const pick = (run: string) => setPicked((now) => (now.includes(run) ? now.filter((one) => one !== run) : now.length >= 16 ? now : [...now, run]));
  const bundlePicked = () => {
    if (!props.client || !checked) return;
    bundleChain(props.client, picked).then(
      (blob) => {
        save(blob, `chain-${picked[0] ?? "runs"}.whizbang-bundle`);
        setChaining(false);
        setPicked([]);
      },
      (error: unknown) => setChainProblem(error instanceof ApiError ? error.message : String(error)),
    );
  };
  const links = checked ? linksOf(checked) : [];
  const now = Date.now();
  const toneOf = (kit: string | null | undefined) => kitTone(Math.max(0, kits.findIndex((one) => one.id === kit)));
  return (
    <main class={chaining ? "screen history history--chaining" : "screen history"} aria-labelledby="history-title">
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
          <Segmented
            label="Replays"
            segments={[
              { value: "", label: "Without replays" },
              { value: "include", label: "With replays" },
              { value: "only", label: "Replays only" },
            ]}
            value={query.get("replayed") ?? ""}
            onChange={(value) => set("replayed", value)}
          />
          <Button variant={chaining ? "secondary" : "quiet"} onClick={() => setChaining((on) => !on)}>
            {chaining ? "Cancel the chain" : "Make a chain"}
          </Button>
          <a class="screen__link" href={hrefOf({ name: "bundles" })}>
            Bundles
          </a>
        </div>
      </header>
      {chaining ? (
        <section class={chainProblem ? "history__chain history__chain--refused" : "history__chain"} aria-label="Making a chain">
          <p class="history__chain-title">{`Making a chain · ${picked.length} picked${links.length ? ` · ${links.length} ${links.length === 1 ? "link" : "links"}` : ""}`}</p>
          {links.map((link) => (
            <p key={link} class="history__chain-link mono">{`↳ link: ${link}`}</p>
          ))}
          {chainProblem ? (
            <p class="history__chain-link mono" role="alert">
              {chainProblem}
            </p>
          ) : (
            <p class="history__chain-link">Pick runs in the order they ran, up to 16. A later run's input that is an earlier pick's output becomes a link.</p>
          )}
          <Button variant="primary" disabled={!checked} why={checked ? undefined : picked.length === 0 ? "Pick a run first." : "The chain is checked first."} onClick={bundlePicked}>
            Bundle chain
          </Button>
        </section>
      ) : null}
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
            <div key={run.run} role="listitem" data-run={run.run} class="history__item">
              {props.client ? <Row client={props.client} run={run} tone={toneOf(run.kit)} now={now} onOpen={() => props.go?.(hrefOf({ name: "run", id: run.run }))} /> : null}
              <span class="history__pin">
                {chaining ? (
                  <button type="button" class="history__pin-button" aria-label={`Pick run ${run.run} for the chain`} aria-pressed={picked.includes(run.run) ? "true" : "false"} onClick={() => pick(run.run)}>
                    {picked.includes(run.run) ? `Picked ${picked.indexOf(run.run) + 1}` : "Pick"}
                  </button>
                ) : null}
                {pin && pin.run !== run.run && pin.action === run.action ? (
                  <button type="button" class="history__pin-button" onClick={() => props.go?.(hrefOf({ name: "compare", baseline: pin.run, attempt: run.run }))}>
                    Compare with baseline
                  </button>
                ) : null}
                <button type="button" class="history__pin-button" aria-pressed={pin?.run === run.run ? "true" : "false"} title="Pin as baseline" onClick={() => pinRun(run)}>
                  {pin?.run === run.run ? "Pinned" : "Pin as baseline"}
                </button>
              </span>
            </div>
          ))}
        </div>
      )}
      {more ? (
        <div class="history__more">
          <Button variant="quiet" disabled={loading} why={loading ? "Asking for the next runs." : undefined} onClick={later}>
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
