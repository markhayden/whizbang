import { useEffect, useRef, useState } from "preact/hooks";
import { ApiError, artifactPath, tasksOf, type Client, type Hello } from "../../api";
import { Button } from "../../components/Button";
import { Canvas } from "../../components/Canvas";
import { Compare, type CompareMode } from "../../components/Compare";
import { CostTable } from "../../components/CostTable";
import { ExampleDialog } from "../../components/ExampleDialog";
import { Segmented } from "../../components/Segmented";
import { SendToMenu } from "../../components/SendToMenu";
import { StepList, type StepListEntry } from "../../components/StepList";
import { Tag } from "../../components/Tag";
import { VerdictCard } from "../../components/VerdictCard";
import { draftOf, localStore, saveDraft } from "../../drafts";
import { endTag, fieldsOf, labelOf, taskOf } from "../../form";
import { dollars, elapsed, middle } from "../../format";
import { useArtifact, useArtifactState, useImageSize } from "../../hooks";
import { actionOf } from "../../keyboard";
import { hrefOf } from "../../routes";
import { EXAMPLE_NAME_LIMIT, EXAMPLE_NOTES_LIMIT, readPin, saveExample, savePin, type Pin } from "../../saved";
import { StoredPicture } from "../shared";
import "./result.css";
import { attemptWords, costLines, draftFrom, fetchCommand, fetchRecord, flagsOf, nothingFound, targetsOf, verdictOf, verdictTitle, type CommandView, type OpenDraft, type RunRecordView } from "./record";

export interface ResultProps {
  id: string;
  hello: Hello | null;
  client: Client;
  go: (hash: string) => void;
  /** The run's steps, as its events said them while it was followed. */
  steps?: StepListEntry[];
}

const IDENTITY = /^sha256:[0-9a-f]{64}$/;

/**
 * A run that ended (plan m5, task W2): its outcome or verdict, before and after, the
 * verdict card, how it got there, its cost, and what to do with it next: Save picture,
 * Send to, Run again, Copy command.
 */
export function Result(props: ResultProps) {
  const [record, setRecord] = useState<RunRecordView | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [view, setView] = useState<string | null>(null);
  const [mode, setMode] = useState<CompareMode>("side");
  const [zoom, setZoom] = useState<number | "fit">("fit");
  const [sending, setSending] = useState(false);
  const [command, setCommand] = useState<CommandView | null>(null);
  const [copied, setCopied] = useState<string | null>(null);
  // A draft Send to or Run again would put in place of one the person has not run yet.
  const [replacing, setReplacing] = useState<{ draft: OpenDraft; sent: boolean } | null>(null);
  // The More menu (plan m7, task U1): pin, compare, save as example, copy command.
  const [more, setMore] = useState(false);
  const [pin, setPin] = useState<Pin | null>(() => readPin(localStore()));
  const [naming, setNaming] = useState(false);
  const [savingExample, setSavingExample] = useState(false);
  const [exampleProblem, setExampleProblem] = useState<string | undefined>(undefined);
  const [said, setSaid] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    fetchRecord(props.client, props.id).then(
      (answer) => live && setRecord(answer),
      (error: unknown) => live && setProblem(error instanceof ApiError ? error.message : String(error)),
    );
    return () => {
      live = false;
    };
  }, [props.id]);

  const kits = Array.isArray(props.hello?.kits) ? props.hello.kits : [];
  const task = taskOf(tasksOf(props.hello), record?.action);
  const inputPicture = record?.inputs.find((one) => one.type === "image" && (one.port === "image" || one.port === "source"))?.id ?? record?.inputs.find((one) => one.type === "image")?.id ?? null;
  const resultPicture = record?.outputs.find((one) => one.type === "image")?.id ?? null;
  const mask = record?.outputs.find((one) => one.type === "mask")?.id ?? null;
  const attempts = record?.attempts?.list ?? [];
  const views = [
    ...(inputPicture ? [{ value: "original", label: "Original" }] : []),
    ...(inputPicture && resultPicture ? [{ value: "compare", label: "Compare" }] : []),
    ...(resultPicture ? [{ value: "result", label: "Result" }] : []),
    ...(mask ? [{ value: "mask", label: "Mask" }] : []),
    ...attempts.filter((attempt) => attempt.picture).map((attempt) => ({ value: `attempt-${attempt.attempt}`, label: `Attempt ${attempt.attempt}` })),
  ];
  const chosen = views.find((one) => one.value === view)?.value ?? (resultPicture ? "result" : (views[0]?.value ?? "original"));
  const attemptShown = attempts.find((attempt) => `attempt-${attempt.attempt}` === chosen);
  const shownId = chosen === "result" || chosen === "mask" ? resultPicture : attemptShown ? attemptShown.picture : chosen === "original" ? inputPicture : null;

  const save = () => {
    if (!resultPicture) return;
    props.client.blobUrl(artifactPath(resultPicture)).then(
      (url) => {
        const link = document.createElement("a");
        link.href = url;
        link.download = `${props.id}-result.png`;
        link.click();
      },
      (error: unknown) => setProblem(error instanceof ApiError ? error.message : String(error)),
    );
  };
  const saveRef = useRef(save);
  saveRef.current = save;
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (actionOf(event)?.kind !== "save") return;
      event.preventDefault();
      saveRef.current();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const open = (draft: OpenDraft, sent: boolean, sure = false) => {
    if (!record) return;
    const store = localStore();
    if (!sure && draftOf(store, draft.task)) {
      setSending(false);
      setReplacing({ draft, sent });
      return;
    }
    setReplacing(null);
    saveDraft(store, draftFrom(draft, record, kits, sent, Date.now()));
    props.go(hrefOf({ name: "kit", kit: draft.kit ?? "rascal", task: draft.task }));
  };
  const copy = () => {
    setProblem(null);
    fetchCommand(props.client, props.id).then(
      (answer) => {
        setCommand(answer);
        const clipboard = typeof navigator !== "undefined" ? navigator.clipboard : undefined;
        const wrote = clipboard?.writeText ? clipboard.writeText(answer.command) : Promise.reject(new Error("no clipboard"));
        wrote.then(
          () => setCopied("Copied. It runs against this store once the playground has quit."),
          () => setCopied("Select the command and copy it: the page could not reach the clipboard."),
        );
      },
      (error: unknown) => setProblem(error instanceof ApiError ? error.message : String(error)),
    );
  };

  const pinHere = () => {
    if (!record) return;
    const next = { run: record.run, action: record.action ?? "" };
    savePin(localStore(), next);
    setPin(next);
    setMore(false);
    setSaid(`Pinned as the baseline. Open another run of ${record.title ?? record.action ?? "this task"} and compare it with this one.`);
  };
  const keepExample = (name: string, notes: string) => {
    setSavingExample(true);
    setExampleProblem(undefined);
    saveExample(props.client, props.id, name, notes).then(
      (example) => {
        setSavingExample(false);
        setNaming(false);
        setSaid(`Saved as the example "${example.name}". It is listed on Home and in its kit, and opens as a draft.`);
      },
      (error: unknown) => {
        setSavingExample(false);
        setExampleProblem(error instanceof ApiError ? error.message : String(error));
      },
    );
  };

  if (!record) {
    return (
      <main class="workspace" aria-labelledby="result-title">
        <section class="workspace__left">
          <header class="workspace__head">
            <h1 id="result-title" class="workspace__title">
              {task?.title ?? "Result"}
            </h1>
            <p class="workspace__purpose mono">run {props.id}</p>
          </header>
          <p class={problem ? "field__error result__pad" : "empty result__pad"} role={problem ? "alert" : undefined}>
            {problem ?? "Reading the run's record…"}
          </p>
        </section>
      </main>
    );
  }

  const verdict = verdictOf(record.verdict?.word);
  const tag = endTag(record.outcome, record.verdict?.word);
  // A step that found nothing (a vague prompt) is said plainly; the record's own words
  // stay beside it, smaller.
  const nothing = nothingFound(record);
  const took = record.start_ms && record.end_ms ? elapsed(record.end_ms - record.start_ms) : null;
  const total = record.cost?.total ?? 0;
  const meta = [`run ${middle(record.run, 18)}`, took, total > 0 || (record.reserved ?? 0) > 0 ? `${dollars(total)}${record.reserved ? ` of up to ${dollars(record.reserved)}` : ""}` : "free"].filter(Boolean).join(" · ");
  const fields = fieldsOf(task);
  const targets = Array.isArray(record.send_to) ? record.send_to : [];

  return (
    <main class="workspace" aria-labelledby="result-title">
      <section class="workspace__left">
        <header class="workspace__head">
          <div class="workspace__title-row">
            <h1 id="result-title" class="workspace__title">
              {record.title ?? task?.title ?? record.action ?? "Result"}
            </h1>
            <Tag kind={tag} detail={nothing ? "nothing found" : undefined} />
          </div>
          <p class="workspace__purpose mono">{meta}</p>
          <a class="run__report" href={hrefOf({ name: "report", id: props.id })}>
            Report
          </a>
        </header>
        <dl class="run__inputs" aria-label="Inputs">
          {record.arguments.map((argument) => {
            const field = fields.find((one) => one.name === argument.name);
            const value = typeof argument.value === "string" || typeof argument.value === "number" || typeof argument.value === "boolean" ? String(argument.value) : "";
            const unused = field?.kind === "backend" ? unusedWhy(record, field.capability) : null;
            return (
              <div class="run__input" key={`a-${argument.name}`}>
                <dt class="field__label">{field ? labelOf(field) : argument.name}</dt>
                <dd class="run__value">{unused ? `${value} · not used (${unused})` : value}</dd>
              </div>
            );
          })}
          {record.inputs.map((input, index) => {
            const field = fields.find((one) => one.name === input.port);
            return (
              <div class="run__input" key={`i-${index}`}>
                <dt class="field__label">{field ? labelOf(field) : input.port}</dt>
                <dd class="run__value">{input.type === "image" && IDENTITY.test(input.id) ? <StoredPicture client={props.client} picture={{ id: input.id, name: middle(input.id, 20) }} /> : <span class="mono">{middle(input.id, 24)}</span>}</dd>
              </div>
            );
          })}
          <p class="workspace__purpose">Change anything and run again; this run stays in the store.</p>
        </dl>
        <footer class="workspace__foot">
          {problem ? (
            <p class="field__error" role="alert">
              {problem}
            </p>
          ) : null}
          {replacing ? (
            <div class="result__replacing" role="alert">
              <p class="workspace__purpose">{`This replaces the draft of ${replacing.draft.title} you have not run yet.`}</p>
              <div class="result__actions">
                <Button variant="secondary" onClick={() => open(replacing.draft, replacing.sent, true)}>
                  Replace it
                </Button>
                <Button variant="quiet" onClick={() => setReplacing(null)}>
                  Keep my draft
                </Button>
              </div>
            </div>
          ) : null}
          {command ? (
            <div class="result__command">
              <pre class="result__command-text mono" aria-label="The command">
                {command.command}
              </pre>
              {command.config_changed ? <p class="workspace__purpose">The config has changed since this run: the command runs under the config as it is now.</p> : null}
              {copied ? <p class="workspace__purpose">{copied}</p> : null}
            </div>
          ) : null}
          <div class="result__actions">
            <Button variant="primary" hint="⌘S" disabled={!resultPicture} why={resultPicture ? undefined : "This run made no picture."} onClick={save}>
              Save picture
            </Button>
            <div class="result__send">
              <Button variant="secondary" onClick={() => setSending((now) => !now)}>
                Send to ▾
              </Button>
              {sending ? (
                <div class="result__menu">
                  <SendToMenu
                    targets={targetsOf(targets, kits)}
                    onSend={(id) => {
                      const target = targets.find((one) => one.task === id);
                      if (target) open(target, true);
                    }}
                    onCopyCommand={() => {
                      setSending(false);
                      copy();
                    }}
                    onDownload={resultPicture ? save : undefined}
                  />
                </div>
              ) : null}
            </div>
          </div>
          {said ? (
            <p class="workspace__purpose" role="status">
              {said}
            </p>
          ) : null}
          <div class="result__actions">
            <Button variant="quiet" disabled={!record.again} why={record.again ? undefined : "This build has no such task to run again."} onClick={() => record.again && open(record.again, false)}>
              Run again
            </Button>
            <div class="result__more-wrap">
              <Button variant="quiet" onClick={() => setMore((now) => !now)}>
                {more ? "More ▴" : "More ▾"}
              </Button>
              {more ? (
                <div class="result__menu result__menu--end">
                  <div class="menu result__more" role="menu" aria-label="More">
                    <button type="button" role="menuitem" class="menu__item" onClick={pinHere}>
                      <span class="menu__title">{pin?.run === record.run ? "Pinned as baseline" : "Pin as baseline"}</span>
                    </button>
                    {pin && pin.run !== record.run ? (
                      <button
                        type="button"
                        role="menuitem"
                        class="menu__item"
                        onClick={() => {
                          setMore(false);
                          props.go(hrefOf({ name: "compare", baseline: pin.run, attempt: record.run }));
                        }}
                      >
                        <span class="menu__title">{`Compare with baseline ${middle(pin.run, 18)}`}</span>
                      </button>
                    ) : null}
                    <span class="menu__rule" aria-hidden="true" />
                    <button
                      type="button"
                      role="menuitem"
                      class="menu__item"
                      onClick={() => {
                        setMore(false);
                        setExampleProblem(undefined);
                        setNaming(true);
                      }}
                    >
                      <span class="menu__title">Save as example…</span>
                    </button>
                    <button
                      type="button"
                      role="menuitem"
                      class="menu__item"
                      onClick={() => {
                        setMore(false);
                        copy();
                      }}
                    >
                      <span class="menu__title mono">{">_ Copy command"}</span>
                    </button>
                  </div>
                </div>
              ) : null}
            </div>
          </div>
        </footer>
      </section>
      <section class="workspace__centre" aria-label="Canvas">
        <div class="workspace__tabs">{views.length > 0 ? <Segmented label="View" segments={views} value={chosen} onChange={setView} /> : null}</div>
        {chosen === "compare" && inputPicture && resultPicture ? (
          <Compared client={props.client} before={inputPicture} after={resultPicture} mode={mode} onMode={setMode} />
        ) : (
          <Shown client={props.client} id={shownId} mask={chosen === "mask" ? mask : null} record={record} zoom={zoom} onZoom={setZoom} diagnostic={attemptShown ? `Attempt ${attemptShown.attempt}: a picture the loop made on the way, not its result` : null} />
        )}
      </section>
      <aside class="workspace__right" aria-label="Summary">
        <h2 class="section-title">Summary</h2>
        {verdict ? <VerdictCard verdict={verdict} title={verdictTitle(verdict)} detail={record.attempts?.kept ? `attempt ${record.attempts.kept}` : undefined} reasons={record.verdict?.reasons ?? []} /> : null}
        {nothing ? (
          <p class="field__error result__nothing" role="status">
            {nothing}
          </p>
        ) : null}
        {record.error ? <p class={nothing ? "workspace__purpose mono" : "field__error"}>{`${record.error.kind}: ${record.error.message}`}</p> : null}
        {record.invalid && record.invalid !== record.error?.message ? <p class={nothing ? "workspace__purpose mono" : "field__error"}>{record.invalid}</p> : null}
        {attempts.length > 0 ? (
          <section aria-label="How it got there">
            <h2 class="section-title">How it got there</h2>
            <ol class="result__attempts">
              {attempts.map((attempt) => (
                <li class="result__attempt" key={attempt.attempt} data-verdict={attempt.verdict ?? "unchecked"}>
                  <span>
                    Attempt {attempt.attempt}
                    {attempt.changed ? ` · ${attempt.changed}` : ""}
                    {record.attempts?.kept === attempt.attempt ? " · kept" : ""}
                  </span>
                  <span class="mono result__attempt-word">{attempt.unchecked && record.attempts?.ended?.includes(attempt.unchecked) ? "not checked" : attemptWords(attempt)}</span>
                </li>
              ))}
            </ol>
            {record.attempts?.ended ? <p class="workspace__purpose">{record.attempts.ended}</p> : null}
          </section>
        ) : null}
        <section aria-label="Cost">
          <h2 class="section-title">Cost</h2>
          {costLines(record.cost).length > 0 ? <CostTable lines={costLines(record.cost)} /> : <p class="empty">Nothing was asked of a model: free.</p>}
          {(record.cost?.unresolved ?? 0) > 0 ? (
            <p class="workspace__purpose result__unresolved">{`Of ${dollars(total)}, ${dollars(record.cost?.unresolved ?? 0)} is of requests no answer settled: each is counted at its bound.`}</p>
          ) : null}
        </section>
        {props.steps && props.steps.length > 0 ? (
          <section aria-label="Steps">
            <h2 class="section-title">Steps</h2>
            <StepList entries={props.steps} />
          </section>
        ) : null}
        <section aria-label="Recorded">
          <h2 class="section-title">Recorded</h2>
          <p class="workspace__tools mono result__run">run {record.run}</p>
        </section>
      </aside>
      {naming ? (
        <ExampleDialog
          from={`from run ${middle(record.run, 18)} · ${record.title ?? record.action ?? ""}`}
          keeps={["task", "settings", "backends", ...record.inputs.filter((input) => input.type === "image").map((input) => `picture ${middle(input.id, 14)}`)].join(", ")}
          nameLimit={EXAMPLE_NAME_LIMIT}
          notesLimit={EXAMPLE_NOTES_LIMIT}
          saving={savingExample}
          problem={exampleProblem}
          onSave={keepExample}
          onCancel={() => setNaming(false)}
        />
      ) : null}
    </main>
  );
}

/**
 * Why a backend the call named was not used, when it was not: the run asked nothing of
 * its capability (no cost line names it) and the record says the capability was not
 * ready. "no key" when what it says is a key the env file does not set.
 */
export function unusedWhy(record: RunRecordView, capability: string | null | undefined): string | null {
  if (!capability) return null;
  const lines = Array.isArray(record.cost?.lines) ? record.cost.lines : [];
  if (lines.some((line) => line.capability === capability)) return null;
  const said = [record.attempts?.ended, ...(record.attempts?.list ?? []).map((one) => one.unchecked), record.error?.message, record.invalid].filter((one): one is string => typeof one === "string");
  const role = capability === "fill" ? "filler" : capability === "segment" ? "segmenter" : capability;
  const about = said.filter((one) => one.includes(`the ${role} needs`) || one.includes(`no ${capability}`) || one.includes(`${capability} backend`));
  if (about.length === 0) return null;
  return about.some((one) => one.includes("env file")) ? "no key" : "not ready";
}

/** Before and after at one zoom: the two pictures fetched with the token, then compared. */
function Compared(props: { client: Client; before: string; after: string; mode: CompareMode; onMode: (mode: CompareMode) => void }) {
  const before = useArtifact(props.client, props.before);
  const after = useArtifact(props.client, props.after);
  const size = useImageSize(after);
  if (!before || !after || !size) return <p class="empty workspace__nothing">Loading the pictures…</p>;
  // One zoom for both, at which the two fit the stage side by side, as it is measured.
  return <Compare before={before} after={after} width={size.width} height={size.height} mode={props.mode} zoom="fit" onMode={props.onMode} />;
}

/** One picture with its marks: the mask over the result, or the verdict's places. */
function Shown(props: { client: Client; id: string | null; mask: string | null; record: RunRecordView; zoom: number | "fit"; onZoom: (zoom: number | "fit") => void; diagnostic: string | null }) {
  const shown = useArtifactState(props.client, props.id);
  const src = shown.url;
  const maskSrc = useArtifact(props.client, props.mask);
  const size = useImageSize(src);
  const note = props.diagnostic ? <p class="workspace__purpose result__diagnostic">{props.diagnostic}</p> : null;
  if (!props.id) return <p class="empty workspace__nothing">This run made no picture to show.</p>;
  if (shown.failed && !src) {
    return (
      <p class="field__error workspace__nothing" role="alert">
        {`The picture could not be loaded: ${shown.failed}`}
      </p>
    );
  }
  if (!src || !size) {
    return (
      <>
        {note}
        <p class="empty workspace__nothing">Loading the picture…</p>
      </>
    );
  }
  const marks = maskSrc ? [{ kind: "mask" as const, src: maskSrc, label: "mask" }] : flagsOf(props.record.verdict?.places, props.id, size.width, size.height);
  return (
    <>
      {note}
      <Canvas src={src} name={props.diagnostic ?? "the picture"} width={size.width} height={size.height} marks={marks} zoom={props.zoom} onZoom={props.onZoom} />
    </>
  );
}
