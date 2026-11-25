import { useEffect, useRef, useState } from "preact/hooks";
import { ApiError, tasksOf, type Client, type Hello, type OpenRun, type Picture } from "../../api";
import { Button } from "../../components/Button";
import { BrandArt } from "../../components/BrandArt";
import { ContinueRow } from "../../components/ContinueRow";
import { cx } from "../../components/cx";
import { DropTarget } from "../../components/DropTarget";
import { ExampleList } from "../../components/ExampleList";
import { KitPanel } from "../../components/KitPanel";
import { kitTone } from "../../components/kitTone";
import { Wordmark } from "../../components/Wordmark";
import { draftOf, readDrafts, localStore, saveDraft, type Draft } from "../../drafts";
import { taskOf } from "../../form";
import { ago, bytes, middle } from "../../format";
import { hrefOf } from "../../routes";
import { asPicture, fetchCard, LIBRARY_PAGE, listLibrary, momentWords, shownName, SHOWS, tileLine, type LibraryCard, type LibraryPicture, type Show } from "../../saved";
import { StoredPicture } from "../shared";
import { draftWith, startersOf, type Starter } from "./picked";

export interface HomeProps {
  hello: Hello | null;
  /** The client of the API; without one (a test of the shell alone) Home asks nothing. */
  client?: Client;
  go?: (hash: string) => void;
  /** A picture picked here or in search (`#/?picture=<identity>`), to start a task with. */
  picture?: string;
}

const STEPS = [
  { n: "01", title: "Pick a kit", line: "each kit is a set of tasks" },
  { n: "02", title: "Give it a picture", line: "say or draw what you want" },
  { n: "03", title: "Send it on", line: "check · fix · copy command" },
];

/** How often Home asks again for the runs still going. */
const OPEN_EVERY_MS = 2000;

/** Home: the intro, Continue, the kits and their tasks, the pictures. */
export function Home(props: HomeProps) {
  const kits = Array.isArray(props.hello?.kits) ? props.hello.kits : [];
  const tasks = tasksOf(props.hello);
  const [open, setOpen] = useState<OpenRun[]>([]);
  const [drafts] = useState<Draft[]>(() => readDrafts(localStore()));
  const [problem, setProblem] = useState<string | null>(null);
  const [asked, setAsked] = useState(0);
  const client = props.client;

  useEffect(() => {
    if (!client) return;
    let live = true;
    const look = () =>
      client.openRuns().then(
        (runs) => live && setOpen(runs),
        () => undefined,
      );
    void look();
    const timer = setInterval(() => void look(), OPEN_EVERY_MS);
    return () => {
      live = false;
      clearInterval(timer);
    };
  }, [client]);

  const library = useLibrary(client, asked);
  const pictures = library.pictures;

  const upload = (files: File[]) => {
    if (!client) return;
    setProblem(null);
    void Promise.all(
      files.map((file) =>
        client.uploadPicture(file).catch((error: unknown) => {
          setProblem(error instanceof ApiError ? error.message : String(error));
        }),
      ),
    ).then(() => setAsked((n) => n + 1));
  };

  const now = Date.now();
  const titled = (action: string) => taskOf(tasks, action);
  return (
    <main class="screen home" aria-labelledby="home-title">
      <section class="home__intro">
        <div class="home__lead">
          <div class="home__brand">
            <BrandArt kind="emblem" src="brand/emblem.webp" alt="" />
            <div class="home__words">
              <h1 id="home-title" class="home__title">
                <Wordmark size="home" src="brand/wordmark.svg" />
                <span class="visually-hidden"> · Home</span>
              </h1>
              <p class="home__line">Edit pictures and check the result, with models on your Mac or in the cloud.</p>
              <p class="home__promise mono">cost shown before every run · every run recorded · any run copies as a command</p>
            </div>
          </div>
          <ol class="home__steps">
            {STEPS.map((step) => (
              <li key={step.n} class="home__step">
                <span class="home__step-n mono">{step.n}</span>
                <span class="home__step-title">{step.title}</span>
                <span class="home__step-line">{step.line}</span>
              </li>
            ))}
          </ol>
        </div>
        <div class="home__art">
          <BrandArt kind="crew" src="brand/crew.webp" alt="The whizbang crew at their machine" />
        </div>
      </section>

      <section class="home__section" aria-labelledby="continue-title">
        <h2 id="continue-title" class="section-title">
          Continue
        </h2>
        {open.length === 0 && drafts.length === 0 ? (
          <p class="empty">Nothing is running and there are no drafts. A run you start shows here until it ends.</p>
        ) : (
          <div class="home__continue">
            {open.map((run) => {
              const task = titled(run.action);
              return (
                <ContinueRow
                  key={run.run}
                  title={task?.title ?? run.action}
                  what={`${task?.kit ?? ""} · ${run.run}`}
                  tag={{ kind: "running", detail: run.state === "stopping" ? "stopping" : undefined }}
                  when={ago(run.started_ms, now)}
                  onOpen={() => props.go?.(hrefOf({ name: "run", id: run.run }))}
                />
              );
            })}
            {drafts.map((draft) => {
              const task = titled(draft.task);
              const named = Object.values(draft.pictures)[0]?.name;
              return (
                <ContinueRow
                  key={`draft-${draft.task}`}
                  title={task?.title ?? draft.task}
                  what={named ? `${draft.kit} · ${named}` : draft.kit}
                  tag={{ kind: "draft" }}
                  when={ago(draft.at, now)}
                  onOpen={() => props.go?.(hrefOf({ name: "kit", kit: draft.kit, task: draft.task }))}
                />
              );
            })}
          </div>
        )}
      </section>

      <section class="home__section" aria-labelledby="kits-title">
        <h2 id="kits-title" class="section-title">
          Kits
        </h2>
        {kits.length === 0 ? (
          <p class="empty">This build lists no kits yet.</p>
        ) : (
          <div class="home__kits">
            {kits.map((kit, index) => (
              <KitPanel
                key={kit.id}
                name={kit.name ?? kit.id}
                line={kit.line ?? ""}
                tone={kitTone(index)}
                tools={typeof kit.tools === "number" ? kit.tools : undefined}
                shown={4}
                onMore={() => props.go?.(hrefOf({ name: "kit", kit: kit.id }))}
                tasks={(Array.isArray(kit.tasks) ? kit.tasks : []).map((task) => ({
                  title: task.title ?? task.name,
                  shape: task.shape ?? "",
                  uses: task.uses ? (kits.find((other) => other.id === task.uses)?.name ?? task.uses) : undefined,
                  href: hrefOf({ name: "kit", kit: kit.id, task: task.name }),
                }))}
              />
            ))}
          </div>
        )}
      </section>

      {client ? (
        <section class="home__section">
          <ExampleList client={client} go={(hash) => props.go?.(hash)} kitName={(kit) => kits.find((one) => one.id === kit)?.name ?? kit ?? ""} />
        </section>
      ) : null}

      <section class="home__section home__library" aria-labelledby="pictures-title">
        <header class="home__library-head">
          <h2 id="pictures-title" class="section-title home__library-title">
            Pictures <span class="home__library-count mono">{`· ${library.counts.all} in this store`}</span>
          </h2>
          <div class="home__chips" role="tablist" aria-label="Which pictures">
            {SHOWS.map((show) => (
              <button
                key={show.value}
                type="button"
                role="tab"
                aria-selected={library.show === show.value ? "true" : "false"}
                class={cx("home__chip", library.show === show.value && "home__chip--on")}
                onClick={() => library.choose(show.value)}
              >
                {show.label} <span class="home__chip-count">{library.counts[show.value]}</span>
              </button>
            ))}
          </div>
        </header>
        {client && props.picture ? (
          <Picked
            client={client}
            id={props.picture}
            listed={pictures.find((one) => one.id === props.picture)}
            starters={startersOf(props.hello)}
            onClose={() => props.go?.(hrefOf({ name: "home" }))}
            go={(hash) => props.go?.(hash)}
          />
        ) : null}
        {problem ? (
          <p class="field__error" role="alert">
            {problem}
          </p>
        ) : null}
        {pictures.length === 0 && !client ? <p class="empty">No pictures yet. A picture a run is given, or one imported, is kept in the store and shows here.</p> : null}
        {client ? (
          <div class="library home__pictures">
            {pictures.length === 0 ? <p class="empty">No pictures yet. A picture a run is given, or one imported, is kept in the store and shows here, whatever gave it.</p> : null}
            {pictures.map((picture) => (
              <figure key={picture.id} class="home__tile" data-picture={picture.id}>
                <StoredPicture
                  client={client}
                  picture={asPicture(picture)}
                  selected={picture.id === props.picture}
                  onClick={() => props.go?.(hrefOf(picture.id === props.picture ? { name: "home" } : { name: "home", picture: picture.id }))}
                />
                <figcaption class="home__tile-words">
                  <span class={cx("home__tile-name", !picture.name && "mono")}>{shownName(picture)}</span>
                  <span class="home__tile-line mono">{tileLine(picture)}</span>
                </figcaption>
              </figure>
            ))}
            {library.more ? (
              <button type="button" class="home__more" onClick={library.next}>
                <span>Show more</span>
                <span class="home__more-line mono">{`${pictures.length} of ${library.counts[library.show]} shown`}</span>
              </button>
            ) : null}
            <DropTarget label="Drop a picture" max={8} onFiles={upload} />
          </div>
        ) : null}
      </section>
    </main>
  );
}

/** A picked picture as the page knows it: still being looked up, its card, or not in the store. */
type Found = { state: "looking" } | { state: "kept"; card: LibraryCard } | { state: "none" } | { state: "failed"; why: string };

/** The card of a picked identity: what the library knows of it, with the runs that took it. */
function useFound(client: Client, id: string, listed: LibraryPicture | undefined): Found {
  const [found, setFound] = useState<Found>({ state: "looking" });
  useEffect(() => {
    let live = true;
    setFound(listed ? { state: "kept", card: { ...listed, runs_list: [] } } : { state: "looking" });
    fetchCard(client, id).then(
      (card) => live && setFound({ state: "kept", card }),
      (error: unknown) => live && setFound(error instanceof ApiError && error.status === 404 ? { state: "none" } : { state: "failed", why: error instanceof ApiError ? error.message : String(error) }),
    );
    return () => {
      live = false;
    };
  }, [id]);
  return found;
}

/** The library's pictures under one chip, a page at a time, and how many each chip holds. */
function useLibrary(client: Client | undefined, asked: unknown) {
  const [show, setShow] = useState<Show>("all");
  const [pictures, setPictures] = useState<LibraryPicture[]>([]);
  const [more, setMore] = useState(false);
  const [counts, setCounts] = useState<Record<Show, number>>({ all: 0, named: 0, inputs: 0, imported: 0 });
  useEffect(() => {
    if (!client || typeof client.request !== "function") return;
    let live = true;
    listLibrary(client, show).then(
      (page) => {
        if (!live) return;
        setPictures(page.pictures);
        setMore(page.more);
        setCounts(page.counts);
      },
      () => undefined,
    );
    return () => {
      live = false;
    };
  }, [client, asked, show]);
  const next = () => {
    const last = pictures[pictures.length - 1];
    if (!client || !last) return;
    listLibrary(client, show, last.id, LIBRARY_PAGE).then(
      (page) => {
        setPictures((now) => [...now, ...page.pictures.filter((one) => !now.some((seen) => seen.id === one.id))]);
        setMore(page.more);
        setCounts(page.counts);
      },
      () => setMore(false),
    );
  };
  return { show, choose: setShow, pictures, more, counts, next };
}

/**
 * A picture picked from the library or from search: shown larger, with every task that
 * takes a picture; picking one opens its draft with the picture given. A draft of that
 * task not yet run is replaced only once the person says so. A picture the store does
 * not hold is said to be missing, and no task is offered.
 */
function Picked(props: { client: Client; id: string; listed: LibraryPicture | undefined; starters: Starter[]; onClose: () => void; go: (hash: string) => void }) {
  const [replacing, setReplacing] = useState<Starter | null>(null);
  const found = useFound(props.client, props.id, props.listed);
  const panel = useRef<HTMLDivElement>(null);
  // Brought into view once for each picture picked, not again on every render (Home
  // renders again as it polls the runs going).
  useEffect(() => {
    panel.current?.scrollIntoView?.({ block: "nearest" });
  }, [props.id]);
  const card = found.state === "kept" ? found.card : null;
  const picture: Picture | null = card ? asPicture(card) : null;
  const start = (starter: Starter, sure = false) => {
    if (!picture) return;
    const store = localStore();
    if (!sure && draftOf(store, starter.task.name)) {
      setReplacing(starter);
      return;
    }
    saveDraft(store, draftWith(starter, picture, Date.now()));
    props.go(hrefOf({ name: "kit", kit: starter.kit, task: starter.task.name }));
  };
  const close = (
    <button type="button" class="link-button" onClick={props.onClose}>
      Close
    </button>
  );
  if (!picture) {
    return (
      <div class="picked" ref={panel} role="region" aria-label="A picture picked">
        <div class="picked__tasks">
          {found.state === "looking" ? <p class="empty">Looking for the picture in the store…</p> : null}
          {found.state === "none" ? (
            <p class="field__error" role="alert">
              The store holds no picture of that identity as an input of a run or an import, so no task can start with it.
            </p>
          ) : null}
          {found.state === "failed" ? (
            <p class="field__error" role="alert">
              {found.why}
            </p>
          ) : null}
          {close}
        </div>
      </div>
    );
  }
  return (
    <div class="picked" ref={panel} role="region" aria-label={`Start a task with ${picture.name}`}>
      <div class="picked__picture">
        <StoredPicture client={props.client} picture={picture} />
        <span class="picked__name mono">{card?.name ? card.name : middle(props.id, 24)}</span>
        {card ? <PictureFacts card={card} /> : null}
      </div>
      <div class="picked__tasks">
        <p class="picked__line">Start a task with this picture:</p>
        {props.starters.length === 0 ? <p class="empty">No task of this build takes a picture.</p> : null}
        <ul class="picked__list">
          {props.starters.map((starter) => (
            <li key={`${starter.kit}/${starter.task.name}`}>
              <button type="button" class="picked__task" onClick={() => start(starter)}>
                <span class="picked__title">{starter.task.title ?? starter.task.name}</span>
                <span class="picked__as mono">{`${starter.kitName} · as its ${starter.label.toLowerCase()}`}</span>
              </button>
            </li>
          ))}
        </ul>
        {replacing ? (
          <div class="picked__replacing" role="alert">
            <p class="workspace__purpose">{`This replaces the draft of ${replacing.task.title ?? replacing.task.name} you have not run yet.`}</p>
            <Button variant="secondary" onClick={() => start(replacing, true)}>
              Replace it
            </Button>
            <Button variant="quiet" onClick={() => props.go(hrefOf({ name: "kit", kit: replacing.kit, task: replacing.task.name }))}>
              Open my draft
            </Button>
          </div>
        ) : null}
        {close}
      </div>
    </div>
  );
}

/**
 * What the library knows of a picture (the design's surface 6, read in the engine's terms):
 * its name or none, its size, when it was first used, and the runs that took it, each a link.
 */
function PictureFacts(props: { card: LibraryCard }) {
  const card = props.card;
  const size = card.width && card.height ? `${card.width} × ${card.height}` : null;
  const facts = [card.name ? null : "No name given", size, typeof card.bytes === "number" ? bytes(card.bytes) : null].filter(Boolean).join(" · ");
  return (
    <div class="picked__facts">
      {facts ? <span class="picked__fact">{facts}</span> : null}
      {card.first_ms ? <span class="picked__fact">{`First used ${momentWords(card.first_ms)}`}</span> : null}
      {card.imported && card.runs === 0 ? <span class="picked__fact">Imported, not run yet</span> : null}
      {card.runs_list.length > 0 ? (
        <div class="picked__runs" aria-label="Runs that took it">
          <span class="picked__runs-title mono">{card.runs === 1 ? "INPUT OF 1 RUN" : `INPUT OF ${card.runs} RUNS`}</span>
          {card.runs_list.map((run) => (
            <a key={run.run} class="picked__run" href={hrefOf({ name: "run", id: run.run })}>
              <span class="mono">{middle(run.run, 16)}</span>
              <span class="picked__run-task">{run.title ?? run.action}</span>
              <span class={cx("picked__run-outcome mono", run.outcome === "completed" && "picked__run-outcome--done")}>{run.outcome}</span>
            </a>
          ))}
        </div>
      ) : null}
    </div>
  );
}
