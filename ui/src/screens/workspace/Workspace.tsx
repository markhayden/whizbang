import { useEffect, useMemo, useRef, useState } from "preact/hooks";
import { ApiError, backendsOf, IDENTITY, tasksOf, type Client, type Hello, type OpenRun, type Picture, type Prepared, type Task, type TaskField } from "../../api";
import { BindingPicker, type Binding } from "../../components/BindingPicker";
import { Button } from "../../components/Button";
import { Canvas, type DrawTool, type Mark } from "../../components/Canvas";
import { ChoiceList } from "../../components/ChoiceList";
import { ContinueRow } from "../../components/ContinueRow";
import { DropTarget } from "../../components/DropTarget";
import { ExampleList } from "../../components/ExampleList";
import { Field } from "../../components/Field";
import { KitPanel } from "../../components/KitPanel";
import { kitTone } from "../../components/kitTone";
import { ListField } from "../../components/ListField";
import { NumberField } from "../../components/NumberField";
import { PictureField } from "../../components/PictureField";
import { PlanBlock } from "../../components/PlanBlock";
import { Segmented } from "../../components/Segmented";
import { SentFromBanner } from "../../components/SentFromBanner";
import { TextField } from "../../components/TextField";
import { dropDraft, draftOf, localStore, saveDraft, type JobOf } from "../../drafts";
import { argumentsOf, choicesFor, displayed, fieldsOf, given, labelOf, planNeeds, seeded, segmentQuery, shapeOf, whyNotRun, type Values } from "../../form";
import { ago, dollars, middle } from "../../format";
import { PICTURES_PAGE, useArtifact, useImageSize } from "../../hooks";
import { actionOf } from "../../keyboard";
import { createPreparer, type Preparer } from "../../preparer";
import { hrefOf } from "../../routes";
import { chainOf, saveChain, withRun } from "../../bundles";
import { fellBackWords, listLibrary, shownName, type LibraryPicture } from "../../saved";
import { EmptyScreen } from "../Empty";
import "../result/result.css";
import { toneOf } from "../result/record";
import { listRuns, runsMaking, type HistoryRun } from "../history/calls";
import { Library } from "../shared";

export interface WorkspaceProps {
  kit: string;
  task?: string;
  hello?: Hello | null;
  client?: Client;
  go?: (hash: string) => void;
}

/** The kit workspace: a task's draft, its inputs, its plan and Run (plan m5, task W1). */
export function Workspace(props: WorkspaceProps) {
  const kits = Array.isArray(props.hello?.kits) ? props.hello.kits : [];
  const tasks = tasksOf(props.hello);
  const task = props.task ? tasks.find((one) => one.name === props.task) : undefined;
  if (!props.task) {
    const index = kits.findIndex((kit) => kit.id === props.kit);
    const kit = kits[index];
    if (!kit) {
      return (
        <EmptyScreen title={props.kit} link={{ href: "#/", words: "Back to Home" }}>
          This build has no kit of that name.
        </EmptyScreen>
      );
    }
    return (
      <main class="screen" aria-labelledby="screen-title">
        <h1 id="screen-title" class="screen__title">
          {kit.name ?? kit.id}
        </h1>
        <KitPanel
          name={kit.name ?? kit.id}
          line={kit.line ?? ""}
          tone={kitTone(index)}
          tools={typeof kit.tools === "number" ? kit.tools : undefined}
          tasks={(Array.isArray(kit.tasks) ? kit.tasks : []).map((one) => ({
            title: one.title ?? one.name,
            shape: one.shape ?? "",
            uses: one.uses ? (kits.find((other) => other.id === one.uses)?.name ?? one.uses) : undefined,
            href: hrefOf({ name: "kit", kit: kit.id, task: one.name }),
          }))}
        />
        {props.client ? (
          <div class="workspace__examples">
            <ExampleList client={props.client} go={props.go ?? (() => undefined)} kit={kit.id} kitName={(id) => kits.find((one) => one.id === id)?.name ?? id ?? ""} />
          </div>
        ) : null}
      </main>
    );
  }
  if (!task || !props.client) {
    return (
      <EmptyScreen title={props.task} link={{ href: "#/", words: "Back to Home" }}>
        This build has no task of that name.
      </EmptyScreen>
    );
  }
  return <Draft key={task.name} task={task} kit={props.kit} hello={props.hello ?? null} client={props.client} go={props.go ?? (() => undefined)} />;
}

interface DraftProps {
  task: Task;
  kit: string;
  hello: Hello | null;
  client: Client;
  go: (hash: string) => void;
}

/** The field whose picture the canvas shows: the one a drawn field is on, else the first picture. */
function canvasField(fields: TaskField[]): TaskField | undefined {
  const named = fields.find((field) => field.on)?.on;
  return fields.find((field) => field.name === named) ?? fields.find((field) => field.kind === "picture") ?? fields.find((field) => field.kind === "pictures");
}

interface Tool {
  value: string;
  label: string;
  field: TaskField;
  tool: DrawTool;
}

/** The canvas tools a task offers: each location's, and segment's point and box. */
function toolsOf(fields: TaskField[]): Tool[] {
  const tools: Tool[] = [];
  for (const field of fields) {
    if (field.canvas === "point_or_box") {
      tools.push({ value: `${field.name}|point`, label: "Point", field, tool: "point" });
      tools.push({ value: `${field.name}|box`, label: "Box", field, tool: "box" });
      continue;
    }
    if (field.kind !== "artifact" || field.artifact !== "location") continue;
    const draw = field.draw ?? [];
    if (draw.includes("box") || draw.includes("polygon")) tools.push({ value: `${field.name}|box`, label: `${labelOf(field)} · box`, field, tool: "box" });
    if (draw.includes("stroke") || draw.includes("line")) tools.push({ value: `${field.name}|stroke`, label: `${labelOf(field)} · ${draw.includes("stroke") ? "stroke" : "line"}`, field, tool: "stroke" });
  }
  return tools;
}

/** The first picture a field's value names. */
function firstId(value: unknown): string | null {
  if (typeof value === "string") return value;
  if (Array.isArray(value) && typeof value[0] === "string") return value[0];
  return null;
}

/** How many earlier runs of a task the right column lists a page. */
export const EARLIER_PAGE = 10;

/** The types of artifact a task's binding pickers take: every artifact port's but a picture's. */
function bindingTypes(fields: TaskField[]): string[] {
  const types = fields.filter((field) => field.kind === "artifact" && field.artifact && field.artifact !== "image").map((field) => field.artifact as string);
  return [...new Set(types)];
}

/** The outputs of a type that a page of runs made, newest first, each once. */
export function bindingsOf(runs: HistoryRun[], type: string, now: number): Binding[] {
  const made: Binding[] = [];
  for (const run of runs) {
    for (const output of Array.isArray(run.outputs) ? run.outputs : []) {
      if (output.type !== type || made.some((one) => one.id === output.id)) continue;
      const asked = [...(Array.isArray(run.words) ? run.words : []), ...(Array.isArray(run.pictures) ? run.pictures : [])].join(" · ");
      made.push({ id: output.id, type, run: run.run, task: run.title ?? run.action, asked: asked || undefined, when: typeof run.at_ms === "number" ? ago(run.at_ms, now) : "" });
    }
  }
  return made;
}

/** A page of the store's runs, or why it could not be had. */
interface Answered<T> {
  value: T;
  /** The host's refusal (a 400, a 501), said where the list would be. */
  error?: string;
}

/** A page of the store's runs; a client without the call (a test of the form) gives none. */
function asked<T>(call: () => Promise<T>, fallback: T): Promise<Answered<T>> {
  return Promise.resolve()
    .then(call)
    .then(
      (value) => ({ value }),
      (error: unknown) => (error instanceof ApiError ? { value: fallback, error: error.message } : { value: fallback }),
    );
}

interface Paging<T> {
  list: T[];
  more: boolean;
  /** The last run of the last page, which the next starts after. */
  last?: string;
  error?: string;
}

const NONE = { runs: [] as HistoryRun[], more: false };

/** A page put after those listed, each run once. */
function appended(now: Paging<HistoryRun> | undefined, page: Answered<typeof NONE>, after: boolean): Paging<HistoryRun> {
  const had = after ? (now?.list ?? []) : [];
  if (page.error) return { list: had, more: false, last: now?.last, error: page.error };
  const runs = page.value.runs.filter((one) => !had.some((seen) => seen.run === one.run));
  return { list: [...had, ...runs], more: page.value.more, last: page.value.runs[page.value.runs.length - 1]?.run ?? (after ? now?.last : undefined) };
}

/**
 * The runs of a task in history, newest first, a page at a time (the right column); read
 * again from the first page whenever `ended` changes (a run going has ended).
 */
function useEarlier(client: Client, task: string, ended: number): Paging<HistoryRun> & { next: () => void } {
  const [paged, setPaged] = useState<Paging<HistoryRun>>({ list: [], more: false });
  const query = new URLSearchParams({ task });
  // Each first page read starts a new list: a later page asked of the list before it is
  // dropped when it comes, so it is never put after a list it does not follow.
  const reading = useRef(0);
  useEffect(() => {
    let live = true;
    const mine = (reading.current += 1);
    void asked(() => listRuns(client, query, undefined, EARLIER_PAGE), NONE).then((page) => {
      if (live && mine === reading.current) setPaged((now) => appended(now, page, false));
    });
    return () => {
      live = false;
    };
  }, [task, ended]);
  const next = () => {
    if (!paged.last) return;
    const mine = reading.current;
    void asked(() => listRuns(client, query, paged.last, EARLIER_PAGE), NONE).then((page) => {
      if (mine === reading.current) setPaged((now) => appended(now, page, true));
    });
  };
  return { ...paged, next };
}

/**
 * The outputs of earlier runs for each binding picker, by type, from the store's history
 * (`GET /api/runs?output=<type>`), a page at a time: what a reload or a restart keeps.
 * Read again from the first page whenever `ended` changes (a run going has ended).
 */
function useBindings(client: Client, types: string[], ended: number): { of: (type: string) => Binding[]; more: (type: string) => (() => void) | undefined; error: (type: string) => string | undefined } {
  const [pages, setPages] = useState<Record<string, Paging<HistoryRun>>>({});
  const key = types.join(",");
  // Each first page asked is numbered: an answer to one older than the newest asked is
  // dropped, so a slow first read never puts back a list a later one replaced.
  const newest = useRef<Record<string, number>>({});
  const load = (type: string, before?: string) => {
    const mine = before === undefined ? (newest.current[type] = (newest.current[type] ?? 0) + 1) : newest.current[type];
    return asked(() => runsMaking(client, type, before), NONE).then((page) => {
      if (newest.current[type] !== mine) return;
      setPages((now) => ({ ...now, [type]: appended(now[type], page, before !== undefined) }));
    });
  };
  useEffect(() => {
    for (const type of types) void load(type);
  }, [key, ended]);
  const now = Date.now();
  return {
    of: (type) => bindingsOf(pages[type]?.list ?? [], type, now),
    more: (type) => {
      const page = pages[type];
      return page?.more && page.last ? () => void load(type, page.last) : undefined;
    },
    error: (type) => pages[type]?.error,
  };
}

/** How often the workspace asks for the runs still going. */
const OPEN_EVERY_MS = 2000;

/**
 * The runs still going, asked every two seconds, and a count that goes up whenever what
 * was listed of history may be behind: once the first look has answered, and each time a
 * run that was going is no longer, or a run the page followed ends (`Client.settled`).
 * The first look counts because history is read when the screen opens, beside it: a run
 * that ends between that read and the first look was never seen going, so its going
 * would never be seen to end, and the lists would stay without it until a reload.
 */
function useOpen(client: Client): { open: OpenRun[]; ended: number } {
  const [state, setState] = useState<{ open: OpenRun[]; ended: number; looked: boolean }>({ open: [], ended: 0, looked: false });
  useEffect(() => {
    let live = true;
    const look = () =>
      client.openRuns().then(
        (runs) =>
          live &&
          setState((now) => {
            const gone = now.open.some((was) => !runs.some((still) => still.run === was.run));
            // A run that ended here or anywhere (another tab, the command line) changed
            // the books: the top bar reads its allowance again.
            if (gone && typeof client.settled === "function") queueMicrotask(() => client.settled());
            return { open: runs, ended: gone || !now.looked ? now.ended + 1 : now.ended, looked: true };
          }),
        () => undefined,
      );
    void look();
    const timer = setInterval(() => void look(), OPEN_EVERY_MS);
    const settled = typeof client.onSettled === "function" ? client.onSettled(() => live && setState((now) => ({ ...now, ended: now.ended + 1 }))) : undefined;
    return () => {
      live = false;
      clearInterval(timer);
      settled?.();
    };
  }, [client]);
  return { open: state.open, ended: state.ended };
}

function Draft(props: DraftProps) {
  const { task, client } = props;
  const fields = fieldsOf(task);
  const backends = backendsOf(props.hello);
  const store = localStore();
  const kept = useMemo(() => draftOf(store, task.name), [task.name]);
  // What a choice, a yes-or-no or a backend shows is what is sent: each is seeded with it.
  const [values, setValues] = useState<Values>(() => seeded(task, kept?.values ?? {}, backends));
  const backendsKey = JSON.stringify(backends.map((one) => [one.capability, one.name, one.default]));
  useEffect(() => setValues((now) => seeded(task, now, backends)), [backendsKey]);
  const [pictures, setPictures] = useState<Record<string, Picture>>(kept?.pictures ?? {});
  const [library, setLibrary] = useState<Picture[] | null>(null);
  const [picking, setPicking] = useState<string | null>(null);
  const [prepared, setPrepared] = useState<Prepared | null>(null);
  const [preparing, setPreparing] = useState(false);
  // Whether a preparation has settled yet, answered or failed: until then a backend hello
  // is not sure of says CHECKING, and after it, NOT READY.
  const [settled, setSettled] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const [drawn, setDrawn] = useState<Record<string, Mark>>({});
  const tools = toolsOf(fields);
  const [tool, setTool] = useState<string>(tools[0]?.value ?? "");
  const [zoom, setZoom] = useState<number | "fit">("fit");
  const [starting, setStarting] = useState(false);
  const going = useOpen(client);
  const open = going.open.filter((run) => run.action === task.name);
  const earlier = useEarlier(client, task.name, going.ended);
  const bindings = useBindings(client, bindingTypes(fields), going.ended);
  const [view, setView] = useState("original");
  const preparer = useRef<Preparer | null>(null);
  // The call the preparer was last told of. A render whose call differs comes before the
  // effect that tells it, and its preparation is of another call: Run waits for it, or a
  // click in that frame would start with the old handle and be refused as stale.
  const changedFor = useRef<string | null>(null);
  // A draft Send to opened (plan m5, task W2, item 3): where it came from, the fields it
  // filled, tagged until changed, and the run whose job a check takes, until the brief
  // is changed. A check run again keeps its run's job the same way (item 4).
  const sent = kept?.sent;
  const [fromFields, setFromFields] = useState<string[]>(sent?.fields ?? []);
  const [jobOf, setJobOf] = useState<JobOf | null>(sent?.jobOf ?? kept?.jobOf ?? null);
  // What the call a draft was made from held that no field shows (Run again, an example,
  // a chain's step): sent as it was, so a draft left as it is makes that call (plan m7,
  // N1's pass, finding 4).
  const recorded = kept?.kept && Object.keys(kept.kept).length > 0 ? kept.kept : undefined;
  // A draft an example opened (plan m7, task U1, item 2): which example, and each choice
  // of backend it made that this config does not offer now.
  const example = kept?.example;
  // A draft a chain's step opened (plan m7, task U2, item 2): the step, and each linked
  // input bound to the new run of the step before, beside the one recorded. It runs as
  // any draft, only when Run is pressed, naming the step.
  // "Run it as an ordinary draft" lets go of the step: the draft then carries no chain.
  const [chain, setChain] = useState(kept?.chain);

  const set = (name: string, value: unknown) => {
    setValues((now) => ({ ...now, [name]: value }));
    setFromFields((now) => now.filter((field) => field !== name));
    if (name === "brief") setJobOf(null);
  };
  const args = { ...recorded, ...withJob(argumentsOf(task, values), jobOf) };
  const argsKey = JSON.stringify(args);

  useEffect(() => {
    const made = createPreparer({
      prepare: (action, sent) => client.prepare(action, sent),
      onAnswer: (answer) => {
        setPrepared(answer);
        setSettled(true);
      },
      onError: (error) => {
        setSettled(true);
        setProblem(error instanceof ApiError ? error.message : String(error));
      },
      onBusy: setPreparing,
    });
    preparer.current = made;
    return () => made.stop();
  }, [client]);

  useEffect(() => {
    changedFor.current = argsKey;
    preparer.current?.change(task.name, args);
    // A draft is what the person gave: what every field shows of itself alone is none.
    const shownAlone = JSON.stringify(argumentsOf(task, seeded(task, {}, backends)));
    if (Object.keys(args).length > 0 && argsKey !== shownAlone) {
      const from = sent ? { sent: { ...sent, fields: fromFields, jobOf: jobOf ?? undefined } } : jobOf ? { jobOf } : {};
      saveDraft(store, { kit: props.kit, task: task.name, values, pictures, at: Date.now(), ...from, ...(recorded ? { kept: recorded } : {}), ...(example ? { example } : {}), ...(chain ? { chain } : {}) });
    }
  }, [argsKey]);

  // A field's library is Home's (plan m7, N1's pass, finding 1): every picture the store
  // holds as an input of a run or an import, whatever door gave it, a page at a time.
  const [libraryMore, setLibraryMore] = useState(false);
  const asPicture = (one: LibraryPicture): Picture => ({ id: one.id, name: shownName(one) });
  const moreLibrary = () => {
    const last = library?.[library.length - 1];
    if (!last) return;
    listLibrary(client, "all", last.id, PICTURES_PAGE).then(
      (page) => {
        setLibrary((now) => [...(now ?? []), ...page.pictures.map(asPicture).filter((one) => !(now ?? []).some((seen) => seen.id === one.id))]);
        setLibraryMore(page.more);
      },
      () => setLibraryMore(false),
    );
  };
  const openLibrary = (name: string) => {
    setPicking((now) => (now === name ? null : name));
    listLibrary(client, "all", undefined, PICTURES_PAGE).then(
      (page) => {
        setLibrary(page.pictures.map(asPicture));
        setLibraryMore(page.more);
      },
      () => {
        setLibrary([]);
        setLibraryMore(false);
      },
    );
  };

  const keep = (picture: Picture) => setPictures((now) => ({ ...now, [picture.id]: picture }));
  const upload = async (files: File[]): Promise<Picture[]> => {
    setProblem(null);
    const made: Picture[] = [];
    for (const file of files) {
      try {
        const picture = await client.uploadPicture(file);
        keep(picture);
        made.push(picture);
      } catch (error) {
        setProblem(error instanceof ApiError ? error.message : String(error));
      }
    }
    return made;
  };

  const shown = canvasField(fields);
  const shownId = shown ? firstId(values[shown.name]) : null;
  const shownPicture = shownId ? pictures[shownId] : undefined;
  const shownSrc = useArtifact(client, shownId);
  // A picture a result sent comes with no size: it is read from the picture.
  const loadedSize = useImageSize(shownPicture?.width ? undefined : shownSrc);
  const shownSize = shownPicture?.width && shownPicture.height ? { width: shownPicture.width, height: shownPicture.height } : loadedSize;
  const current = tools.find((one) => one.value === tool) ?? tools[0];
  // Mask and Result, once they exist: the draft's own mask, else the last run's; the last
  // run's result (plan m5, task W1, item 4).
  const last = earlier.list[0];
  const lastOutput = (type: string) => (Array.isArray(last?.outputs) ? last.outputs.find((output) => output.type === type)?.id : undefined) ?? null;
  const draftMask = fields.filter((field) => field.kind === "artifact" && field.artifact === "mask").map((field) => values[field.name]).find((value): value is string => typeof value === "string") ?? null;
  const maskShown = draftMask ? { mask: draftMask, on: shownId } : lastOutput("mask") ? { mask: lastOutput("mask"), on: last?.thumb ?? null } : null;
  const resultShown = lastOutput("image");
  const views = [
    { value: "original", label: "Original" },
    ...(maskShown ? [{ value: "mask", label: "Mask" }] : []),
    ...(resultShown ? [{ value: "result", label: "Result" }] : []),
  ];
  const chosen = views.find((one) => one.value === view)?.value ?? "original";

  const onDraw = (mark: Mark) => {
    if (!current) return;
    const field = current.field;
    if (field.canvas === "point_or_box") {
      const query = segmentQuery(mark);
      if (!query) return;
      setValues((now) => ({ ...now, [field.name]: query.query, ...(fields.some((one) => one.name === "kind") ? { kind: query.kind } : {}) }));
      setDrawn((now) => ({ ...now, [field.name]: mark }));
      return;
    }
    const shape = shapeOf(mark, field.draw);
    const picture = firstId(values[field.on ?? shown?.name ?? ""]);
    if (!shape || !picture) return;
    setProblem(null);
    client.commitLocation(picture, shape).then(
      (made) => {
        set(field.name, made.id);
        setDrawn((now) => ({ ...now, [field.name]: mark.kind === "box" ? { ...mark, label: labelOf(field) } : mark }));
      },
      (error: unknown) => setProblem(error instanceof ApiError ? error.message : String(error)),
    );
  };

  const why = whyNotRun(task, values, prepared, preparing || changedFor.current !== argsKey);
  const bound = prepared?.bound ?? 0;
  const run = () => {
    if (why !== null || starting || !prepared?.handle) return;
    setStarting(true);
    setProblem(null);
    const replay = chain ? { replay: { bundle: chain.digest, step: chain.step } } : {};
    client.start({ action: task.name, arguments: args, handle: prepared.handle, ...replay }).then(
      (started) => {
        dropDraft(store, task.name);
        if (chain) {
          const place = chainOf(store, chain.digest) ?? { digest: chain.digest, steps: chain.of, runs: [], stopped: false, at: Date.now() };
          const titles = chain.titles ?? place.titles;
          saveChain(store, withRun({ ...place, ...(titles ? { titles } : {}) }, chain.step, started.run));
        }
        props.go(hrefOf({ name: "run", id: started.run }));
      },
      (error: unknown) => {
        setStarting(false);
        if (error instanceof ApiError && error.status === 409 && error.kind === "stale") {
          setPrepared(null);
          setProblem(`${error.message} The call was prepared again: look at the plan and press Run once more.`);
          preparer.current?.now(task.name, args);
          return;
        }
        setProblem(error instanceof ApiError ? error.message : String(error));
      },
    );
  };
  const runRef = useRef(run);
  runRef.current = run;
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (actionOf(event)?.kind !== "run") return;
      event.preventDefault();
      runRef.current();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const marks = Object.values(drawn);
  const kits = Array.isArray(props.hello?.kits) ? props.hello.kits : [];
  const sentKit = sent ? kits.find((kit) => kit.name === sent.kit || kit.id === sent.kit) : undefined;
  const fromWords = sent ? `FROM ${sent.kit.toUpperCase()}` : undefined;
  return (
    <main class={sent ? "workspace workspace--sent" : "workspace"} aria-labelledby="task-title">
      {sent ? (
        <div class="workspace__banner">
          <SentFromBanner
            title={`Sent from ${sent.kit} · ${sent.title}`}
            line={`run ${sent.run} · ${ago(sent.at, Date.now())} · ${fromFields.length > 0 ? `${fromFields.join(", ")} came with it` : "nothing it sent is left unchanged"}`}
            tone={toneOf(kits, sentKit?.id)}
            onBack={() => props.go(hrefOf({ name: "run", id: sent.run }))}
          />
        </div>
      ) : null}
      <section class="workspace__left">
        <header class="workspace__head">
          <h1 id="task-title" class="workspace__title">
            {task.title ?? task.name}
          </h1>
          {task.purpose ? <p class="workspace__purpose">{task.purpose}</p> : null}
          {chain ? (
            <p class="workspace__example workspace__chain">
              <span class="workspace__example-tag mono">{`STEP ${chain.step + 1} OF ${chain.of}`}</span>
              <a class="workspace__example-name" href={hrefOf({ name: "chain", digest: chain.digest })}>
                {`chain ${middle(chain.digest, 18)}`}
              </a>
              <span class="workspace__chain-line">nothing runs until you press Run</span>
              <button
                type="button"
                class="link-button workspace__chain-line"
                onClick={() => {
                  setChain(undefined);
                  saveDraft(store, { kit: props.kit, task: task.name, values, pictures, at: Date.now(), ...(jobOf ? { jobOf } : {}), ...(recorded ? { kept: recorded } : {}), ...(example ? { example } : {}) });
                }}
              >
                Run it as an ordinary draft
              </button>
            </p>
          ) : null}
          {example ? (
            <p class="workspace__example">
              <span class="workspace__example-tag mono">EXAMPLE</span>
              <span class="workspace__example-name">{example.name}</span>
            </p>
          ) : null}
        </header>
        <div class="workspace__fields">
          {chain?.bindings.map((one) => (
            <div key={one.port} class={one.differs ? "chain-binding chain-binding--differs" : "chain-binding"} role="note">
              <p class="chain-binding__head">{`${one.port} · linked to step ${one.from.step + 1}`}</p>
              <p class="chain-binding__line">{`${one.from.port} from ${middle(one.run, 18)}, step ${one.from.step + 1}'s new run`}</p>
              {one.differs ? (
                <p class="chain-binding__ids mono">
                  <span class="chain-binding__differs">differs from recorded</span>
                  <br />
                  {`now ${middle(one.now, 18)}`}
                  <br />
                  {`recorded ${middle(one.recorded, 18)}`}
                </p>
              ) : (
                <p class="chain-binding__ids mono">same as recorded</p>
              )}
            </div>
          ))}
          {recorded ? (
            <p class="workspace__fell-back workspace__kept" role="note">
              {`Kept from the call this draft was made from, which no field here shows, and sent as it was: ${Object.keys(recorded).join(", ")}.`}
            </p>
          ) : null}
          {jobOf !== null && !sent ? (
            <p class="workspace__fell-back workspace__kept" role="note">
              The check's job is the one recorded, requirements and all, until you change its brief.
            </p>
          ) : null}
          {example?.fellBack.map((one) => (
            <p key={one.argument} class="workspace__fell-back" role="note">
              {fellBackWords(one, one.capability === "fill" ? "filler" : one.capability === "segment" ? "segmenter" : one.capability)}
            </p>
          ))}
          {fields.map((field) => (
            <Input
              key={field.name}
              field={field}
              value={values[field.name]}
              backends={backends}
              prepared={prepared}
              settled={settled}
              pictures={pictures}
              client={client}
              picking={picking === field.name}
              library={library}
              drawn={Boolean(drawn[field.name])}
              bindings={field.kind === "artifact" && field.artifact ? bindings.of(field.artifact) : []}
              onMoreBindings={field.kind === "artifact" && field.artifact ? bindings.more(field.artifact) : undefined}
              bindingsError={field.kind === "artifact" && field.artifact ? bindings.error(field.artifact) : undefined}
              from={fromFields.includes(field.name) ? fromWords : undefined}
              onChange={(value) => set(field.name, value)}
              onClear={() => {
                set(field.name, undefined);
                setDrawn((now) => {
                  const next = { ...now };
                  delete next[field.name];
                  return next;
                });
              }}
              onUpload={upload}
              onLibrary={() => openLibrary(field.name)}
              onMoreLibrary={library && library.length > 0 && libraryMore ? moreLibrary : undefined}
              onPicked={(picture) => {
                keep(picture);
                setPicking(null);
              }}
            />
          ))}
        </div>
        <footer class="workspace__foot">
          <PlanBlock needs={planNeeds(prepared, backendsOf(props.hello))} bound={bound} attempts={prepared?.attempts ?? undefined} stale={preparing} ready={prepared ? prepared.ready : undefined} />
          {problem ? (
            <p class="field__error" role="alert">
              {problem}
            </p>
          ) : null}
          <div class="workspace__run">
            <span class="workspace__bound mono">{prepared?.ready ? (bound > 0 ? `up to ${dollars(bound)}` : "free") : ""}</span>
            <Button variant="primary" hint="⌘↵" disabled={why !== null || starting} why={why ?? undefined} busy={starting} onClick={run}>
              Run
            </Button>
          </div>
        </footer>
      </section>
      <section class="workspace__centre" aria-label="Canvas">
        <div class="workspace__tabs">
          <Segmented label="View" segments={views} value={chosen} onChange={setView} />
          {tools.length > 0 && chosen === "original" ? <Segmented label="Draw" segments={tools.map((one) => ({ value: one.value, label: one.label }))} value={current?.value ?? ""} onChange={setTool} /> : null}
        </div>
        {chosen === "mask" && maskShown ? (
          <Other client={client} id={maskShown.on} mask={maskShown.mask} zoom={zoom} onZoom={setZoom} note={draftMask ? "The mask this draft is given, over its picture." : `The mask of the last run, ${last?.run ?? ""}, over its picture.`} />
        ) : chosen === "result" && resultShown ? (
          <Other client={client} id={resultShown} mask={null} zoom={zoom} onZoom={setZoom} note={`The result of the last run, ${last?.run ?? ""}.`} />
        ) : shownId && shownSrc && shownSize ? (
          <Canvas src={shownSrc} name={shownPicture?.name ?? "the picture"} width={shownSize.width} height={shownSize.height} marks={marks} zoom={zoom} tool={current?.tool ?? null} onDraw={onDraw} onZoom={setZoom} />
        ) : (
          <p class="empty workspace__nothing">{shown ? "Give the task a picture to see it here." : "This task takes no picture."}</p>
        )}
      </section>
      <aside class="workspace__right">
        <h2 class="section-title">Runs of this task</h2>
        {earlier.error ? (
          <p class="field__error" role="alert">
            {`History could not be read: ${earlier.error}`}
          </p>
        ) : null}
        {open.length === 0 && earlier.list.length === 0 ? (
          earlier.error ? null : <p class="empty">Not run yet. The plan on the left is what will happen.</p>
        ) : (
          <div class="workspace__runs">
            {open.map((one) => (
              <ContinueRow key={one.run} title={task.title ?? task.name} what={one.run} tag={{ kind: "running" }} when={ago(one.started_ms, Date.now())} onOpen={() => props.go(hrefOf({ name: "run", id: one.run }))} />
            ))}
            {earlier.list
              .filter((one) => !open.some((going) => going.run === one.run))
              .map((one) => (
                <ContinueRow
                  key={one.run}
                  title={[...(one.words ?? []), ...(one.pictures ?? [])].join(" · ") || (task.title ?? task.name)}
                  what={one.run}
                  tag={{ kind: one.result, soft: true }}
                  when={typeof one.at_ms === "number" ? ago(one.at_ms, Date.now()) : ""}
                  onOpen={() => props.go(hrefOf({ name: "run", id: one.run }))}
                />
              ))}
            {earlier.more ? (
              <button type="button" class="link-button" onClick={earlier.next}>
                Earlier runs
              </button>
            ) : null}
          </div>
        )}
        <h2 class="section-title">Tools this task uses</h2>
        <p class="workspace__tools mono">
          {Array.isArray(task.tools) && task.tools.length > 0
            ? task.tools.map((tool, at) => (
                <span key={tool}>
                  {at > 0 ? " · " : null}
                  <a href={hrefOf({ name: "record", record: tool })}>{tool}</a>
                </span>
              ))
            : "none"}
        </p>
        <a class="screen__link workspace__record" href={hrefOf({ name: "record", record: task.name })}>
          Read this task's record
        </a>
      </aside>
    </main>
  );
}

interface InputProps {
  field: TaskField;
  value: unknown;
  backends: ReturnType<typeof backendsOf>;
  prepared: Prepared | null;
  /** Whether a preparation has settled yet. */
  settled: boolean;
  pictures: Record<string, Picture>;
  client: Client;
  picking: boolean;
  library: Picture[] | null;
  drawn: boolean;
  /** The outputs of earlier runs this port can take, from the store, and the next page. */
  bindings: Binding[];
  onMoreBindings?: () => void;
  /** Why the earlier outputs could not be read, when they could not. */
  bindingsError?: string;
  onChange: (value: unknown) => void;
  onClear: () => void;
  onUpload: (files: File[]) => Promise<Picture[]>;
  onLibrary: () => void;
  /** The library's next page, when the last was full. */
  onMoreLibrary?: () => void;
  onPicked: (picture: Picture) => void;
  /** FROM <KIT>, when the value came from another run (plan m5, task W2). */
  from?: string;
}

/** One field of the form, by its kind. */
function Input(props: InputProps) {
  const { field } = props;
  const id = `field-${field.name}`;
  const label = labelOf(field);
  const optional = !field.required;
  const help = field.purpose;
  switch (field.kind) {
    case "text":
      return <TextField id={id} label={label} value={typeof props.value === "string" ? props.value : ""} optional={optional} help={help} from={props.from} onInput={props.onChange} />;
    case "number":
    case "integer": {
      const integer = field.kind === "integer";
      const min = field.min ?? undefined;
      const max = field.max ?? undefined;
      if (integer && min !== undefined && max !== undefined && max - min <= 8) {
        const seen = displayed(field, props.value, props.backends);
      const shown = typeof seen === "number" ? seen : min;
        const segments = Array.from({ length: max - min + 1 }, (_, at) => ({ value: String(min + at), label: String(min + at) }));
        return (
          <Field id={id} label={label} optional={optional} help={help}>
            <Segmented label={label} look="numbers" segments={segments} value={String(shown)} onChange={(next) => props.onChange(Number(next))} />
          </Field>
        );
      }
      return <NumberField id={id} label={label} value={typeof props.value === "number" ? props.value : null} integer={integer} min={min} max={max} optional={optional} help={help} onInput={props.onChange} />;
    }
    case "boolean": {
      const shown = displayed(field, props.value, props.backends) === true;
      return (
        <Field id={id} label={label} optional={optional} help={help}>
          <Segmented
            label={label}
            segments={[
              { value: "yes", label: "Yes" },
              { value: "no", label: "No" },
            ]}
            value={shown ? "yes" : "no"}
            onChange={(next) => props.onChange(next === "yes")}
          />
        </Field>
      );
    }
    case "choice": {
      const choices = Array.isArray(field.choices) ? field.choices : [];
      const seen = displayed(field, props.value, props.backends);
      const shown = typeof seen === "string" ? seen : "";
      return (
        <Field id={id} label={label} optional={optional} help={help}>
          <Segmented label={label} segments={choices.map((choice) => ({ value: choice, label: choice }))} value={shown} onChange={props.onChange} />
        </Field>
      );
    }
    case "backend": {
      const choices = choicesFor(field, props.prepared, props.backends, props.settled);
      const seen = displayed(field, props.value, props.backends);
      const shown = typeof seen === "string" ? seen : null;
      return (
        <Field id={id} label={label} optional={optional} help={choices.length === 0 ? "The config offers no backend for this." : help}>
          <ChoiceList label={label} name={id} choices={choices} value={shown} onChange={props.onChange} />
        </Field>
      );
    }
    case "picture":
      return <PictureInput {...props} id={id} label={label} />;
    case "pictures":
      return <PicturesInput {...props} id={id} label={label} />;
    case "artifact": {
      const drawable = field.artifact === "location" && Array.isArray(field.draw) && field.draw.length > 0;
      return (
        <Field id={id} label={label} optional={optional} help={help} from={props.from}>
          {drawable ? (
            <div class="workspace__drawn">
              <span class="mono">{props.drawn || given(props.value) ? "drawn on the picture" : `draw a ${(field.draw ?? []).join(" or ")} on the picture`}</span>
              {given(props.value) ? (
                <button type="button" class="link-button" onClick={props.onClear}>
                  Clear
                </button>
              ) : null}
            </div>
          ) : null}
          <BindingPicker type={field.artifact ?? ""} bindings={offered(props.bindings, field.artifact ?? "", props.value, props.drawn)} value={typeof props.value === "string" ? props.value : null} onPick={props.onChange} onMore={props.onMoreBindings} error={props.bindingsError} thumb={(binding) => <OutputThumb client={props.client} id={binding.id} />} />
        </Field>
      );
    }
  }
}

function PictureInput(props: InputProps & { id: string; label: string }) {
  const id = typeof props.value === "string" ? props.value : null;
  const known = id ? props.pictures[id] : undefined;
  const src = useArtifact(props.client, id, 256);
  return (
    <div class="workspace__picture">
      <PictureField
        id={props.id}
        label={props.label}
        optional={!props.field.required}
        help={props.field.purpose}
        from={props.from}
        picture={id ? { id, src: src ?? "", name: known?.name ?? id, width: known?.width, height: known?.height } : null}
        onDrop={(files) => {
          void props.onUpload(files.slice(0, 1)).then((made) => {
            if (made[0]) props.onChange(made[0].id);
          });
        }}
        onChoose={props.onLibrary}
        onClear={props.onClear}
      />
      {props.picking && !id ? (
        <Library
          client={props.client}
          pictures={props.library ?? []}
          onMore={props.onMoreLibrary}
          label={`Pictures for ${props.label}`}
          onPick={(picture) => {
            props.onPicked(picture);
            props.onChange(picture.id);
          }}
        />
      ) : null}
    </div>
  );
}

function PicturesInput(props: InputProps & { id: string; label: string }) {
  const ids = Array.isArray(props.value) ? props.value.filter((one): one is string => typeof one === "string") : [];
  const min = props.field.count?.min ?? (props.field.required ? 1 : 0);
  const max = props.field.count?.max ?? null;
  const room = max === null ? 16 : Math.max(0, max - ids.length);
  const add = (made: Picture[]) => props.onChange([...ids, ...made.map((picture) => picture.id)].slice(0, max ?? undefined));
  return (
    <div class="workspace__picture">
      <ListField
        id={props.id}
        label={props.label}
        min={min}
        max={max}
        items={ids.map((one, index) => ({ key: `${index}-${one}`, label: props.pictures[one]?.name ?? one }))}
        onMove={(from, to) => {
          const next = [...ids];
          const [moved] = next.splice(from, 1);
          if (moved !== undefined) next.splice(to, 0, moved);
          props.onChange(next);
        }}
        onRemove={(index) => props.onChange(ids.filter((_, at) => at !== index))}
        adder={
          <div class="workspace__adder">
            <DropTarget label="Drop pictures" max={room} onFiles={(files) => void props.onUpload(files.slice(0, room)).then(add)} />
            <button type="button" class="link-button" onClick={props.onLibrary}>
              From the library
            </button>
          </div>
        }
      />
      {props.picking && room > 0 ? (
        <Library
          client={props.client}
          pictures={props.library ?? []}
          onMore={props.onMoreLibrary}
          label={`Pictures for ${props.label}`}
          onPick={(picture) => {
            props.onPicked(picture);
            add([picture]);
          }}
        />
      ) : null}
    </div>
  );
}

/** The call's arguments with the job a check takes from a run, and no brief beside it. */
export function withJob(args: Record<string, unknown>, jobOf: JobOf | null): Record<string, unknown> {
  if (!jobOf) return args;
  const next: Record<string, unknown> = { ...args, job_of: jobOf };
  delete next.brief;
  return next;
}

/**
 * What a binding picker offers: the earlier outputs of its type, and first, when the port
 * holds one that is not among them (sent from a result, or kept in a draft) and was not
 * drawn here, that one, so the picker shows what the draft is given.
 */
export function offered(bindings: Binding[], type: string, value: unknown, drawn: boolean): Binding[] {
  if (typeof value !== "string" || drawn || bindings.some((one) => one.id === value)) return bindings;
  return [{ id: value, type, run: middle(value, 20), task: "given to this draft", when: "" }, ...bindings];
}

/** An earlier output's thumbnail in a binding picker: a mask as a grey picture, fetched with the token. */
function OutputThumb(props: { client: Client; id: string }) {
  const src = useArtifact(props.client, IDENTITY.test(props.id) ? props.id : null, 64);
  return src ? <img class="binding-picker__thumb" src={src} alt="" /> : <span class="binding-picker__thumb" aria-hidden="true" />;
}

/** A picture other than the draft's own (the last run's result, or a mask over its picture). */
function Other(props: { client: Client; id: string | null; mask: string | null; zoom: number | "fit"; onZoom: (zoom: number | "fit") => void; note: string }) {
  const src = useArtifact(props.client, props.id);
  const maskSrc = useArtifact(props.client, props.mask);
  const size = useImageSize(src);
  const note = <p class="workspace__purpose workspace__note">{props.note}</p>;
  if (!props.id) return <p class="empty workspace__nothing">Give the task a picture to see its mask over it.</p>;
  if (!src || !size) {
    return (
      <>
        {note}
        <p class="empty workspace__nothing">Loading the picture…</p>
      </>
    );
  }
  return (
    <>
      {note}
      <Canvas src={src} name={props.note} width={size.width} height={size.height} marks={maskSrc ? [{ kind: "mask", src: maskSrc, label: "mask" }] : []} zoom={props.zoom} onZoom={props.onZoom} />
    </>
  );
}
